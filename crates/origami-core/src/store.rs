//! Local store: one global SQLite database (WAL) with app-owned keys
//! (ADR-0001), an FTS5 index over all messages, the outbox queue, and
//! per-folder sync checkpoints (ADR-0003).
//!
//! `Store` is `Clone` (shared `Arc<Mutex<Connection>>`); call it from
//! blocking contexts (`spawn_blocking`) — the sync engine already is one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde_json;

use crate::message::ParsedMessage;
use crate::model::{
    Address, Correspondent, Envelope, EnvelopeSource, Flag, KeywordCount, Mailbox, MailboxRole,
    OutboxEntry, OutboxOp, SavedSearch, SyncState,
};
use crate::{private_fs, Error, Result};

type LogicalCacheIdentity = (
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    i64,
);

const SCHEMA_V1: &str = "
CREATE TABLE accounts (
    id         TEXT PRIMARY KEY,
    config_id  TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    email      TEXT NOT NULL
);

CREATE TABLE folders (
    id         TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    role       TEXT NOT NULL DEFAULT 'other',
    UNIQUE(account_id, name)
);

CREATE TABLE sync_state (
    folder_id       TEXT PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE,
    uid_validity    INTEGER NOT NULL DEFAULT 0,
    highest_modseq  INTEGER NOT NULL DEFAULT 0,
    last_uid        INTEGER NOT NULL DEFAULT 0,
    last_sync_at    INTEGER
);

CREATE TABLE messages (
    id             TEXT PRIMARY KEY,
    folder_id      TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    server_uid     INTEGER NOT NULL,
    message_id     TEXT,
    thread_id      TEXT,
    subject        TEXT NOT NULL DEFAULT '',
    from_json      TEXT NOT NULL DEFAULT '[]',
    to_json        TEXT NOT NULL DEFAULT '[]',
    date           TEXT,
    received_at    INTEGER,
    size           INTEGER NOT NULL DEFAULT 0,
    flags_json     TEXT NOT NULL DEFAULT '[]',
    has_attachment INTEGER NOT NULL DEFAULT 0,
    blob_hash      TEXT,
    indexed_at     INTEGER,
    UNIQUE(folder_id, server_uid)
);
CREATE INDEX idx_messages_folder ON messages(folder_id, server_uid DESC);
CREATE INDEX idx_messages_received_at ON messages(received_at);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    subject, from_text, to_text, body,
    tokenize = 'porter unicode61'
);

CREATE TABLE outbox (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    op_json    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);
";

fn delete_folder_rows(conn: &Connection, folder_id: &str) -> rusqlite::Result<()> {
    // FTS5 is maintained manually, so foreign-key cascades cannot remove
    // its rows when the owning folder is deleted.
    conn.execute(
        "DELETE FROM messages_fts WHERE rowid IN
           (SELECT rowid FROM messages WHERE folder_id = ?1)",
        params![folder_id],
    )?;
    conn.execute("DELETE FROM folders WHERE id = ?1", params![folder_id])?;
    Ok(())
}

/// Origami's local database. Cheap to clone, safe to share across tasks.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

/// A locally known message that is eligible for background body prefetch.
#[derive(Debug, Clone)]
pub struct PrefetchCandidate {
    pub envelope: Envelope,
    pub mailbox: String,
}

impl Store {
    /// Open (and migrate) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            private_fs::create_private_dir(parent)?;
        }
        let conn = Connection::open(path)?;
        private_fs::secure_existing_file(path)?;
        let store = Self::init(conn)?;
        // SQLite creates WAL sidecars with the process umask; tighten them too.
        for sidecar in ["-wal", "-shm", "-journal"] {
            let mut name = path.as_os_str().to_os_string();
            name.push(sidecar);
            private_fs::secure_existing_file(&PathBuf::from(name))?;
        }
        Ok(store)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000_i64)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| Error::Backend("store lock poisoned".into()))
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;
        let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            // The column may already exist from a partial upgrade that
            // crashed before the user_version bump took effect.
            let has_col: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('messages') WHERE name = 'keywords_json'",
                )?
                .exists([])?;
            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE messages ADD COLUMN keywords_json TEXT NOT NULL DEFAULT '[]';",
                )?;
            }
            conn.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS drafts (
                    id         TEXT PRIMARY KEY,
                    draft_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )?;
            conn.pragma_update(None, "user_version", 3)?;
        }
        if version < 4 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS saved_searches (
                    id         TEXT PRIMARY KEY,
                    name       TEXT NOT NULL,
                    query      TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );",
            )?;
            conn.pragma_update(None, "user_version", 4)?;
        }
        if version < 5 {
            for (column, definition) in [
                ("account_id", "TEXT"),
                ("remote_mailbox", "TEXT"),
                ("remote_uid", "INTEGER"),
            ] {
                let exists: bool = conn
                    .prepare(&format!(
                        "SELECT 1 FROM pragma_table_info('drafts') WHERE name = '{column}'"
                    ))?
                    .exists([])?;
                if !exists {
                    conn.execute_batch(&format!(
                        "ALTER TABLE drafts ADD COLUMN {column} {definition};"
                    ))?;
                }
            }
            conn.pragma_update(None, "user_version", 5)?;
        }
        if version < 6 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS message_cache (
                    message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
                    parsed_json TEXT NOT NULL,
                    cached_at   INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_outbox_account_id ON outbox(account_id, id);",
            )?;
            conn.pragma_update(None, "user_version", 6)?;
        }
        if version < 7 {
            let has_col: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name = 'received_at'")?
                .exists([])?;
            if !has_col {
                conn.execute_batch("ALTER TABLE messages ADD COLUMN received_at INTEGER;")?;
            }

            let existing_dates: Vec<(String, Option<String>)> = {
                let mut stmt = conn.prepare("SELECT id, date FROM messages")?;
                let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for (message_id, date) in existing_dates {
                if let Some(received_at) = received_at_from_date(date.as_deref()) {
                    conn.execute(
                        "UPDATE messages SET received_at = ?2 WHERE id = ?1",
                        params![message_id, received_at],
                    )?;
                }
            }
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_messages_received_at
                   ON messages(received_at);",
            )?;
            conn.pragma_update(None, "user_version", 7)?;
        }
        if version < 8 {
            // The display MIME parser previously allowed a text/plain
            // alternative to occupy the cached HTML field before the real
            // text/html section was visited. Parsed metadata is disposable;
            // invalidate it once so existing messages are rebuilt with the
            // corrected section selection. Full RFC 822 blobs are separate
            // and remain available for local reparsing.
            conn.execute("DELETE FROM message_cache", [])?;
            conn.pragma_update(None, "user_version", 8)?;
        }
        if version < 9 {
            // Terminal failure state for poisoned outbox ops (NULL = active).
            let has_col: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('outbox') WHERE name = 'failed_at'")?
                .exists([])?;
            if !has_col {
                conn.execute("ALTER TABLE outbox ADD COLUMN failed_at INTEGER", [])?;
            }
            conn.pragma_update(None, "user_version", 9)?;
        }
        Ok(())
    }

    // --------------------------------------------------------------
    // Accounts & folders
    // --------------------------------------------------------------

    /// Upsert an account by its config id; returns the app-owned UUID.
    pub fn upsert_account(&self, config_id: &str, name: &str, email: &str) -> Result<String> {
        let conn = self.conn()?;
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM accounts WHERE config_id = ?1",
                params![config_id],
                |r| r.get(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE accounts SET name = ?2, email = ?3 WHERE id = ?1",
                params![id, name, email],
            )?;
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO accounts (id, config_id, name, email) VALUES (?1, ?2, ?3, ?4)",
            params![id, config_id, name, email],
        )?;
        Ok(id)
    }

    /// Upsert a folder by (account, name); returns the app-owned UUID.
    pub fn upsert_folder(&self, account_id: &str, name: &str, role: MailboxRole) -> Result<String> {
        let conn = self.conn()?;
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM folders WHERE account_id = ?1 AND name = ?2",
                params![account_id, name],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO folders (id, account_id, name, role) VALUES (?1, ?2, ?3, ?4)",
            params![id, account_id, name, role.as_str()],
        )?;
        // Fresh folder → fresh sync checkpoint row.
        conn.execute(
            "INSERT OR IGNORE INTO sync_state (folder_id) VALUES (?1)",
            params![id],
        )?;
        Ok(id)
    }

    /// List folders of an account with live message counts.
    pub fn list_folders(&self, account_id: &str) -> Result<Vec<Mailbox>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT f.id, f.name, f.role,
                    (SELECT COUNT(*) FROM messages m WHERE m.folder_id = f.id),
                    (SELECT COUNT(*) FROM messages m WHERE m.folder_id = f.id
                       AND m.flags_json NOT LIKE '%\"Seen\"%')
               FROM folders f WHERE f.account_id = ?1 ORDER BY f.name",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok(Mailbox {
                id: r.get(0)?,
                account_id: account_id.to_string(),
                name: r.get(1)?,
                role: r.get::<_, String>(2)?.parse().unwrap_or(MailboxRole::Other),
                total: r.get::<_, i64>(3)? as u32,
                unread: r.get::<_, i64>(4)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Count logical messages across physical folders without counting a
    /// provider label copy more than once.
    pub fn logical_envelope_counts(&self, folder_ids: &[String]) -> Result<(u32, u32)> {
        if folder_ids.is_empty() {
            return Ok((0, 0));
        }
        let logical = self.list_envelopes_in_folders(folder_ids, 1, u32::MAX, false)?;
        let unread = logical
            .iter()
            .filter(|envelope| !envelope.flags.contains(&Flag::Seen))
            .count();
        Ok((logical.len() as u32, unread as u32))
    }

    /// Remove folders that were not present in a successful remote listing.
    /// Returns the names removed so callers can refresh selected-folder state.
    pub fn remove_folders_not_in(
        &self,
        account_id: &str,
        remote_names: &[String],
    ) -> Result<Vec<String>> {
        let remote_names: HashSet<&str> = remote_names.iter().map(String::as_str).collect();
        let stale: Vec<(String, String)> = {
            let conn = self.conn()?;
            let mut stmt = conn.prepare("SELECT id, name FROM folders WHERE account_id = ?1")?;
            let rows = stmt.query_map(params![account_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let stale: Vec<(String, String)> = stale
            .into_iter()
            .filter(|(_, name)| !remote_names.contains(name.as_str()))
            .collect();
        let conn = self.conn()?;
        for (folder_id, _) in &stale {
            delete_folder_rows(&conn, folder_id)?;
        }
        Ok(stale.into_iter().map(|(_, name)| name).collect())
    }

    pub fn folder_id(&self, account_id: &str, name: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT id FROM folders WHERE account_id = ?1 AND name = ?2",
                params![account_id, name],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn rename_folder(&self, folder_id: &str, name: &str) -> Result<()> {
        self.conn()?.execute(
            "UPDATE folders SET name = ?2 WHERE id = ?1",
            params![folder_id, name],
        )?;
        Ok(())
    }

    pub fn delete_folder(&self, folder_id: &str) -> Result<()> {
        let conn = self.conn()?;
        delete_folder_rows(&conn, folder_id)?;
        Ok(())
    }

    /// Resolve a folder UUID to (account_db_id, folder name).
    pub fn folder_account_and_name(&self, folder_id: &str) -> Result<Option<(String, String)>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT account_id, name FROM folders WHERE id = ?1",
                params![folder_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?)
    }

    pub fn folder_role(&self, folder_id: &str) -> Result<Option<MailboxRole>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT role FROM folders WHERE id = ?1",
                params![folder_id],
                |row| {
                    Ok(row
                        .get::<_, String>(0)?
                        .parse()
                        .unwrap_or(MailboxRole::Other))
                },
            )
            .optional()?)
    }

    /// Resolve an account UUID to its config id (TOML key).
    pub fn account_config_id(&self, account_db_id: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT config_id FROM accounts WHERE id = ?1",
                params![account_db_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    // --------------------------------------------------------------
    // Sync state
    // --------------------------------------------------------------

    pub fn sync_state(&self, folder_id: &str) -> Result<Option<SyncState>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT uid_validity, highest_modseq, last_uid
                   FROM sync_state WHERE folder_id = ?1",
                params![folder_id],
                |r| {
                    Ok(SyncState {
                        uid_validity: r.get::<_, i64>(0)? as u32,
                        highest_modseq: r.get::<_, i64>(1)? as u64,
                        last_uid: r.get::<_, i64>(2)? as u32,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_sync_state(&self, folder_id: &str, state: &SyncState) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO sync_state (folder_id, uid_validity, highest_modseq, last_uid, last_sync_at)
             VALUES (?1, ?2, ?3, ?4, unixepoch())
             ON CONFLICT(folder_id) DO UPDATE SET
               uid_validity = excluded.uid_validity,
               highest_modseq = excluded.highest_modseq,
               last_uid = excluded.last_uid,
               last_sync_at = excluded.last_sync_at",
            params![
                folder_id,
                state.uid_validity as i64,
                state.highest_modseq as i64,
                state.last_uid as i64
            ],
        )?;
        Ok(())
    }

    // --------------------------------------------------------------
    // Messages
    // --------------------------------------------------------------

    /// Remove all messages of a folder (UIDVALIDITY-flip path, ADR-0003 §1).
    pub fn clear_folder_messages(&self, folder_id: &str) -> Result<u64> {
        let conn = self.conn()?;
        // Keep the FTS index consistent.
        conn.execute(
            "DELETE FROM messages_fts WHERE rowid IN
               (SELECT rowid FROM messages WHERE folder_id = ?1)",
            params![folder_id],
        )?;
        let n = conn.execute(
            "DELETE FROM messages WHERE folder_id = ?1",
            params![folder_id],
        )?;
        Ok(n as u64)
    }

    /// Insert or update an envelope keyed by (folder, server_uid).
    /// Returns `(message_uuid, inserted)`.
    pub fn upsert_envelope(&self, folder_id: &str, envelope: &Envelope) -> Result<(String, bool)> {
        let conn = self.conn()?;
        let server_uid = envelope
            .server_uid
            .ok_or_else(|| Error::Backend("envelope without server_uid".into()))?;
        let flags_json = serde_json::to_string(&envelope.flags)
            .map_err(|e| Error::Backend(format!("flags json: {e}")))?;
        let keywords_json = serde_json::to_string(&envelope.keywords)
            .map_err(|e| Error::Backend(format!("keywords json: {e}")))?;
        let from_json = serde_json::to_string(&envelope.from)
            .map_err(|e| Error::Backend(format!("from json: {e}")))?;
        let to_json = serde_json::to_string(&envelope.to)
            .map_err(|e| Error::Backend(format!("to json: {e}")))?;
        let received_at = envelope
            .received_at
            .or_else(|| received_at_from_date(envelope.date.as_deref()));

        if let Some(id) = conn
            .query_row(
                "SELECT id FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE messages SET subject = ?3, from_json = ?4, to_json = ?5,
                   date = ?6, received_at = ?13, size = ?7, flags_json = ?8,
                   keywords_json = ?10, message_id = ?9, thread_id = ?11,
                   has_attachment = ?12
                 WHERE id = ?1 AND folder_id = ?2",
                params![
                    id,
                    folder_id,
                    envelope.subject,
                    from_json,
                    to_json,
                    envelope.date,
                    envelope.size as i64,
                    flags_json,
                    envelope.message_id,
                    keywords_json,
                    envelope.thread_id,
                    envelope.has_attachment as i64,
                    received_at,
                ],
            )?;
            return Ok((id, false));
        }

        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO messages
               (id, folder_id, server_uid, message_id, thread_id, subject, from_json, to_json,
                 date, received_at, size, flags_json, keywords_json, has_attachment)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                folder_id,
                server_uid as i64,
                envelope.message_id,
                envelope.thread_id,
                envelope.subject,
                from_json,
                to_json,
                envelope.date,
                received_at,
                envelope.size as i64,
                flags_json,
                keywords_json,
                envelope.has_attachment as i64,
            ],
        )?;
        Ok((id, true))
    }

    /// Upsert a page of envelopes in one SQLite transaction. Initial syncs
    /// otherwise pay a connection lock and transaction cost for every row.
    pub fn upsert_envelopes(&self, folder_id: &str, envelopes: &[Envelope]) -> Result<u32> {
        if envelopes.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO messages
                   (id, folder_id, server_uid, message_id, thread_id, subject, from_json, to_json,
                    date, received_at, size, flags_json, keywords_json, has_attachment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(folder_id, server_uid) DO UPDATE SET
                   message_id = excluded.message_id,
                   thread_id = excluded.thread_id,
                   subject = excluded.subject,
                   from_json = excluded.from_json,
                   to_json = excluded.to_json,
                   date = excluded.date,
                   received_at = excluded.received_at,
                   size = excluded.size,
                   flags_json = excluded.flags_json,
                   keywords_json = excluded.keywords_json,
                   has_attachment = excluded.has_attachment",
            )?;
            for envelope in envelopes {
                let server_uid = envelope
                    .server_uid
                    .ok_or_else(|| Error::Backend("envelope without server_uid".into()))?;
                let id = uuid::Uuid::now_v7().to_string();
                let flags_json = serde_json::to_string(&envelope.flags)
                    .map_err(|error| Error::Backend(format!("flags json: {error}")))?;
                let keywords_json = serde_json::to_string(&envelope.keywords)
                    .map_err(|error| Error::Backend(format!("keywords json: {error}")))?;
                let from_json = serde_json::to_string(&envelope.from)
                    .map_err(|error| Error::Backend(format!("from json: {error}")))?;
                let to_json = serde_json::to_string(&envelope.to)
                    .map_err(|error| Error::Backend(format!("to json: {error}")))?;
                let received_at = envelope
                    .received_at
                    .or_else(|| received_at_from_date(envelope.date.as_deref()));
                stmt.execute(params![
                    id,
                    folder_id,
                    server_uid as i64,
                    envelope.message_id,
                    envelope.thread_id,
                    envelope.subject,
                    from_json,
                    to_json,
                    envelope.date,
                    received_at,
                    envelope.size as i64,
                    flags_json,
                    keywords_json,
                    envelope.has_attachment as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(envelopes.len() as u32)
    }

    /// Replace the flag set of a known message. Returns false when the
    /// message is unknown OR the flags were already as given (the write
    /// is skipped in that case, keeping sync stats meaningful).
    pub fn update_flags(&self, folder_id: &str, server_uid: u32, flags: &[Flag]) -> Result<bool> {
        let conn = self.conn()?;
        let flags_json =
            serde_json::to_string(flags).map_err(|e| Error::Backend(format!("flags json: {e}")))?;
        let n = conn.execute(
            "UPDATE messages SET flags_json = ?3
             WHERE folder_id = ?1 AND server_uid = ?2 AND flags_json != ?3",
            params![folder_id, server_uid as i64, flags_json],
        )?;
        Ok(n > 0)
    }

    /// Replace user-defined IMAP keywords for one message.
    pub fn update_keywords(
        &self,
        folder_id: &str,
        server_uid: u32,
        keywords: &[String],
    ) -> Result<bool> {
        let conn = self.conn()?;
        let keywords_json = serde_json::to_string(keywords)
            .map_err(|e| Error::Backend(format!("keywords json: {e}")))?;
        let n = conn.execute(
            "UPDATE messages SET keywords_json = ?3
             WHERE folder_id = ?1 AND server_uid = ?2 AND keywords_json != ?3",
            params![folder_id, server_uid as i64, keywords_json],
        )?;
        Ok(n > 0)
    }

    /// Delete a message by server UID (expunge/move-away events).
    pub fn delete_message_by_uid(&self, folder_id: &str, server_uid: u32) -> Result<bool> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM messages_fts WHERE rowid IN
               (SELECT rowid FROM messages WHERE folder_id = ?1 AND server_uid = ?2)",
            params![folder_id, server_uid as i64],
        )?;
        let n = conn.execute(
            "DELETE FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
            params![folder_id, server_uid as i64],
        )?;
        Ok(n > 0)
    }

    /// Highest server UID stored for a folder (0 when empty).
    pub fn max_server_uid(&self, folder_id: &str) -> Result<u32> {
        let conn = self.conn()?;
        let max: Option<i64> = conn.query_row(
            "SELECT MAX(server_uid) FROM messages WHERE folder_id = ?1",
            params![folder_id],
            |r| r.get(0),
        )?;
        Ok(max.unwrap_or(0) as u32)
    }

    /// All server UIDs currently stored for a folder, in ascending order.
    pub fn message_uids(&self, folder_id: &str) -> Result<Vec<u32>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT server_uid FROM messages
             WHERE folder_id = ?1 ORDER BY server_uid ASC",
        )?;
        let rows = stmt.query_map(params![folder_id], |row| row.get::<_, i64>(0))?;
        rows.map(|row| row.map(|uid| uid as u32).map_err(Into::into))
            .collect()
    }

    /// Expand a logical special-use folder into its physical source folders.
    pub fn folder_source_ids(&self, folder_id: &str) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let Some((account_id, role)) = conn
            .query_row(
                "SELECT account_id, role FROM folders WHERE id = ?1",
                params![folder_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        else {
            return Ok(Vec::new());
        };

        if role != MailboxRole::Sent.as_str() {
            return Ok(vec![folder_id.to_string()]);
        }

        let mut stmt = conn.prepare(
            "SELECT id FROM folders
              WHERE account_id = ?1 AND role = ?2
              ORDER BY name",
        )?;
        let rows = stmt.query_map(params![account_id, role], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<String>>>()
            .map_err(Into::into)
    }

    /// Page envelopes from one local folder, newest first by server-received
    /// date, with rowid only providing deterministic ordering for ties.
    pub fn list_envelopes(
        &self,
        folder_id: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Envelope>> {
        self.list_envelopes_in_folders(&[folder_id.to_string()], page, page_size, false)
    }

    /// Page envelopes from multiple physical folders as one logical view.
    ///
    /// The query deliberately reads all matching physical rows before applying
    /// pagination. Provider labels are separate IMAP rows, so paginating first
    /// would allow one logical message to occupy several page slots.
    ///
    /// When `unread_only` is true, filter after logical dedupe (unread if any
    /// retained copy lacks Seen) and before skip/take, so label copies keep
    /// their merged `sources`.
    pub fn list_envelopes_in_folders(
        &self,
        folder_ids: &[String],
        page: u32,
        page_size: u32,
        unread_only: bool,
    ) -> Result<Vec<Envelope>> {
        if folder_ids.is_empty() || page_size == 0 {
            return Ok(Vec::new());
        }

        let conn = self.conn()?;
        let placeholders = vec!["?"; folder_ids.len()].join(", ");
        let statement = format!(
            "SELECT m.id, m.folder_id, m.server_uid, m.message_id, m.thread_id, m.subject,
                    m.from_json, m.to_json, m.date, m.received_at, m.size, m.flags_json,
                    m.keywords_json, m.has_attachment, f.account_id
               FROM messages m
               JOIN folders f ON f.id = m.folder_id
              WHERE m.folder_id IN ({placeholders})
             ORDER BY m.received_at DESC, m.rowid DESC"
        );
        let bind_values = folder_ids
            .iter()
            .map(|folder_id| Value::Text(folder_id.clone()))
            .collect::<Vec<_>>();
        let mut stmt = conn.prepare(&statement)?;
        let rows = stmt.query_map(params_from_iter(bind_values), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, String>(12)?,
                r.get::<_, i64>(13)?,
                r.get::<_, String>(14)?,
            ))
        })?;
        let mut physical = Vec::new();
        for row in rows {
            let (
                id,
                mailbox_id,
                uid,
                message_id,
                thread_id,
                subject,
                from_json,
                to_json,
                date,
                received_at,
                size,
                flags_json,
                keywords_json,
                has_att,
                account_id,
            ) = row?;
            physical.push((
                account_id,
                Envelope {
                    id,
                    mailbox_id: mailbox_id.clone(),
                    subject,
                    from: serde_json::from_str(&from_json).unwrap_or_default(),
                    to: serde_json::from_str(&to_json).unwrap_or_default(),
                    date,
                    received_at,
                    flags: serde_json::from_str(&flags_json).unwrap_or_default(),
                    keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
                    has_attachment: has_att != 0,
                    size: size as u32,
                    server_uid: Some(uid as u32),
                    message_id,
                    thread_id,
                    sources: vec![EnvelopeSource {
                        mailbox_id,
                        server_uid: uid as u32,
                    }],
                },
            ));
        }

        let mut logical = deduplicate_envelopes(physical);
        if unread_only {
            logical.retain(|envelope| !envelope.flags.contains(&Flag::Seen));
        }
        let offset = page.max(1).saturating_sub(1).saturating_mul(page_size) as usize;
        Ok(logical
            .into_iter()
            .skip(offset)
            .take(page_size as usize)
            .collect())
    }

    /// Bound the parsed-display cache: drop rows older than `max_age_days`
    /// (by message received time, falling back to cache time), then keep only
    /// the newest `max_rows`. `message_cache` is a pure cache — blobs, FTS,
    /// and envelope rows are untouched. Returns the number of rows deleted.
    pub fn evict_display_cache(&self, max_rows: u32, max_age_days: u32) -> Result<u32> {
        let conn = self.conn()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| Error::Backend(format!("system clock before Unix epoch: {error}")))?
            .as_secs() as i64;
        let cutoff = now - i64::from(max_age_days) * 24 * 60 * 60;
        let aged = conn.execute(
            "DELETE FROM message_cache WHERE message_id IN (
                 SELECT c.message_id FROM message_cache c
                 JOIN messages m ON m.id = c.message_id
                 WHERE COALESCE(m.received_at, c.cached_at) < ?1)",
            params![cutoff],
        )?;
        let capped = conn.execute(
            "DELETE FROM message_cache WHERE message_id IN (
                 SELECT c.message_id FROM message_cache c
                 JOIN messages m ON m.id = c.message_id
                 ORDER BY COALESCE(m.received_at, c.cached_at) DESC
                 LIMIT -1 OFFSET ?1)",
            params![max_rows],
        )?;
        Ok((aged + capped) as u32)
    }

    /// List recent messages whose display data is not cached locally.
    ///
    /// The received timestamp is populated from the normalized message date
    /// during envelope sync and persisted separately so this query stays
    /// indexed and does not need to parse RFC 822 dates on every sync cycle.
    pub fn recent_uncached_messages(
        &self,
        account_id: &str,
        since: i64,
        limit: u32,
        prefer_folder_id: Option<&str>,
    ) -> Result<Vec<PrefetchCandidate>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT m.id, m.folder_id, m.server_uid, m.message_id, m.thread_id, m.subject,
                    m.from_json, m.to_json, m.date, m.received_at, m.size, m.flags_json,
                    m.keywords_json, m.has_attachment, f.name
               FROM messages m
               JOIN folders f ON f.id = m.folder_id
               LEFT JOIN message_cache c ON c.message_id = m.id
              WHERE f.account_id = ?1
                AND m.received_at >= ?2
                AND m.blob_hash IS NULL
                AND c.message_id IS NULL
             ORDER BY
               CASE
                 WHEN ?3 IS NOT NULL AND m.folder_id = ?3 THEN 0
                 WHEN f.role = 'inbox' THEN 1
                 WHEN f.role IN ('sent', 'drafts', 'archive') THEN 2
                 ELSE 3
               END,
               m.received_at DESC, m.rowid DESC
              LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![account_id, since, prefer_folder_id, limit as i64],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<i64>>(9)?,
                    r.get::<_, i64>(10)?,
                    r.get::<_, String>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, i64>(13)?,
                    r.get::<_, String>(14)?,
                ))
            },
        )?;
        let mut candidates = Vec::new();
        for row in rows {
            let (
                id,
                mailbox_id,
                uid,
                message_id,
                thread_id,
                subject,
                from_json,
                to_json,
                date,
                received_at,
                size,
                flags_json,
                keywords_json,
                has_attachment,
                mailbox,
            ) = row?;
            candidates.push(PrefetchCandidate {
                envelope: Envelope {
                    id,
                    mailbox_id: mailbox_id.clone(),
                    subject,
                    from: serde_json::from_str(&from_json).unwrap_or_default(),
                    to: serde_json::from_str(&to_json).unwrap_or_default(),
                    date,
                    received_at,
                    flags: serde_json::from_str(&flags_json).unwrap_or_default(),
                    keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
                    has_attachment: has_attachment != 0,
                    size: size as u32,
                    server_uid: Some(uid as u32),
                    message_id,
                    thread_id,
                    sources: vec![EnvelopeSource {
                        mailbox_id,
                        server_uid: uid as u32,
                    }],
                },
                mailbox,
            });
        }
        Ok(candidates)
    }

    /// Look up one envelope using the folder/UID unique index.
    pub fn get_envelope(&self, folder_id: &str, server_uid: u32) -> Result<Option<Envelope>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, message_id, thread_id, subject, from_json, to_json, date,
                        received_at, size, flags_json, keywords_json, has_attachment
                   FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, Option<i64>>(7)?,
                        r.get::<_, i64>(8)?,
                        r.get::<_, String>(9)?,
                        r.get::<_, String>(10)?,
                        r.get::<_, i64>(11)?,
                    ))
                },
            )
            .optional()?;

        Ok(row.map(
            |(
                id,
                message_id,
                thread_id,
                subject,
                from_json,
                to_json,
                date,
                received_at,
                size,
                flags_json,
                keywords_json,
                has_attachment,
            )| Envelope {
                id,
                mailbox_id: folder_id.to_string(),
                subject,
                from: serde_json::from_str(&from_json).unwrap_or_default(),
                to: serde_json::from_str(&to_json).unwrap_or_default(),
                date,
                received_at,
                flags: serde_json::from_str(&flags_json).unwrap_or_default(),
                keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
                has_attachment: has_attachment != 0,
                size: size as u32,
                server_uid: Some(server_uid),
                message_id,
                thread_id,
                sources: vec![EnvelopeSource {
                    mailbox_id: folder_id.to_string(),
                    server_uid,
                }],
            },
        ))
    }

    /// Load one logical envelope from its physical folder/UID references.
    /// Missing references are ignored because a provider may expunge one
    /// label between the list and the open operation.
    pub fn get_envelope_from_sources(
        &self,
        sources: &[EnvelopeSource],
    ) -> Result<Option<Envelope>> {
        let mut logical = None;
        for source in sources {
            let Some(envelope) = self.get_envelope(&source.mailbox_id, source.server_uid)? else {
                continue;
            };
            if let Some(existing) = logical.as_mut() {
                merge_envelope_sources(existing, envelope);
            } else {
                logical = Some(envelope);
            }
        }
        Ok(logical)
    }

    /// Page all account Inbox folders as one local view.
    pub fn list_unified_inbox(
        &self,
        page: u32,
        page_size: u32,
        unread_only: bool,
    ) -> Result<Vec<Envelope>> {
        if page_size == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id FROM folders WHERE role = 'inbox'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let folder_ids = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        drop(conn);
        self.list_envelopes_in_folders(&folder_ids, page, page_size, unread_only)
    }

    /// Record the blob hash of a fetched body and index the message in FTS.
    pub fn set_body(
        &self,
        folder_id: &str,
        server_uid: u32,
        blob_hash: &str,
        body_text: &str,
    ) -> Result<()> {
        let conn = self.conn()?;
        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |r| r.get(0),
            )
            .optional()?;
        let Some(rowid) = rowid else {
            return Err(Error::Backend(format!(
                "message uid {server_uid} not found in folder {folder_id}"
            )));
        };
        conn.execute(
            "UPDATE messages SET blob_hash = ?1, indexed_at = unixepoch() WHERE rowid = ?2",
            params![blob_hash, rowid],
        )?;
        // Reindex: replace any stale FTS row, then index metadata + body.
        conn.execute("DELETE FROM messages_fts WHERE rowid = ?1", params![rowid])?;
        let (subject, from_json, to_json): (String, String, String) = conn.query_row(
            "SELECT subject, from_json, to_json FROM messages WHERE rowid = ?1",
            params![rowid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let from_text = address_text(&from_json);
        let to_text = address_text(&to_json);
        conn.execute(
            "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![rowid, subject, from_text, to_text, body_text],
        )?;
        Ok(())
    }

    /// Atomically persist a fetched body, its FTS row, parsed MIME cache, and
    /// derived threading/attachment metadata.
    pub fn set_body_and_cache(
        &self,
        folder_id: &str,
        server_uid: u32,
        blob_hash: &str,
        parsed: &ParsedMessage,
        thread_id: &str,
    ) -> Result<()> {
        let parsed_json = serde_json::to_string(parsed)
            .map_err(|error| Error::Backend(format!("parsed message json: {error}")))?;
        let body_text = crate::message::body_text_for_index(parsed);
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let rowid: Option<i64> = tx
            .query_row(
                "SELECT rowid FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |row| row.get(0),
            )
            .optional()?;
        let Some(rowid) = rowid else {
            return Err(Error::Backend(format!(
                "message uid {server_uid} not found in folder {folder_id}"
            )));
        };
        tx.execute(
            "UPDATE messages SET blob_hash = ?1, indexed_at = unixepoch(),
                                has_attachment = ?2, thread_id = ?3
              WHERE rowid = ?4",
            params![
                blob_hash,
                (!parsed.attachments.is_empty()) as i64,
                thread_id,
                rowid
            ],
        )?;
        tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", params![rowid])?;
        let (subject, from_json, to_json): (String, String, String) = tx.query_row(
            "SELECT subject, from_json, to_json FROM messages WHERE rowid = ?1",
            params![rowid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        tx.execute(
            "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                rowid,
                subject,
                address_text(&from_json),
                address_text(&to_json),
                body_text
            ],
        )?;
        let message_id: String = tx.query_row(
            "SELECT id FROM messages WHERE rowid = ?1",
            params![rowid],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO message_cache (message_id, parsed_json, cached_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(message_id) DO UPDATE SET
               parsed_json = excluded.parsed_json,
               cached_at = excluded.cached_at",
            params![message_id, parsed_json],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Cache the normalized parsed representation for a fetched message.
    pub fn set_parsed_message(
        &self,
        folder_id: &str,
        server_uid: u32,
        parsed: &ParsedMessage,
    ) -> Result<()> {
        let parsed_json = serde_json::to_string(parsed)
            .map_err(|error| Error::Backend(format!("parsed message json: {error}")))?;
        let conn = self.conn()?;
        let message_id: String = conn
            .query_row(
                "SELECT id FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                Error::Backend(format!(
                    "message uid {server_uid} not found in folder {folder_id}"
                ))
            })?;
        conn.execute(
            "INSERT INTO message_cache (message_id, parsed_json, cached_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(message_id) DO UPDATE SET
               parsed_json = excluded.parsed_json,
               cached_at = excluded.cached_at",
            params![message_id, parsed_json],
        )?;
        Ok(())
    }

    /// Cache display-only MIME data and index its available text without
    /// claiming that a complete RFC 822 blob exists locally.
    pub fn set_parsed_message_and_index(
        &self,
        folder_id: &str,
        server_uid: u32,
        parsed: &ParsedMessage,
        thread_id: &str,
    ) -> Result<()> {
        let parsed_json = serde_json::to_string(parsed)
            .map_err(|error| Error::Backend(format!("parsed message json: {error}")))?;
        let body_text = crate::message::body_text_for_index(parsed);
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let rowid: Option<i64> = tx
            .query_row(
                "SELECT rowid FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |row| row.get(0),
            )
            .optional()?;
        let Some(rowid) = rowid else {
            return Err(Error::Backend(format!(
                "message uid {server_uid} not found in folder {folder_id}"
            )));
        };
        tx.execute(
            "UPDATE messages SET has_attachment = ?1, thread_id = ?2,
                                indexed_at = unixepoch()
              WHERE rowid = ?3",
            params![(!parsed.attachments.is_empty()) as i64, thread_id, rowid],
        )?;
        tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", params![rowid])?;
        let (subject, from_json, to_json): (String, String, String) = tx.query_row(
            "SELECT subject, from_json, to_json FROM messages WHERE rowid = ?1",
            params![rowid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        tx.execute(
            "INSERT INTO messages_fts (rowid, subject, from_text, to_text, body)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                rowid,
                subject,
                address_text(&from_json),
                address_text(&to_json),
                body_text
            ],
        )?;
        let message_id: String = tx.query_row(
            "SELECT id FROM messages WHERE rowid = ?1",
            params![rowid],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO message_cache (message_id, parsed_json, cached_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(message_id) DO UPDATE SET
               parsed_json = excluded.parsed_json,
               cached_at = excluded.cached_at",
            params![message_id, parsed_json],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Read cached normalized message data without touching the blob store.
    pub fn parsed_message(
        &self,
        folder_id: &str,
        server_uid: u32,
    ) -> Result<Option<ParsedMessage>> {
        let conn = self.conn()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT c.parsed_json
                   FROM message_cache c
                   JOIN messages m ON m.id = c.message_id
                  WHERE m.folder_id = ?1 AND m.server_uid = ?2",
                params![folder_id, server_uid as i64],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| Error::Backend(format!("parsed message cache: {error}")))
        })
        .transpose()
    }

    /// Read cached display data from any physical copy of the same logical
    /// message before a caller falls back to the network. Copies in the same
    /// physical folder are intentionally excluded because duplicate Message-ID
    /// rows in one folder are not safe to merge.
    pub fn parsed_message_for_logical_message(
        &self,
        folder_id: &str,
        server_uid: u32,
    ) -> Result<Option<ParsedMessage>> {
        if let Some(parsed) = self.parsed_message(folder_id, server_uid)? {
            return Ok(Some(parsed));
        }
        let conn = self.conn()?;
        let identity: Option<LogicalCacheIdentity> = conn
            .query_row(
                "SELECT f.account_id, m.message_id, m.subject, m.from_json, m.to_json,
                        m.date, m.size
                   FROM messages m
                   JOIN folders f ON f.id = m.folder_id
                  WHERE m.folder_id = ?1 AND m.server_uid = ?2",
                params![folder_id, server_uid as i64],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((account_id, message_id, subject, from_json, to_json, date, size)) = identity
        else {
            return Ok(None);
        };

        let json: Option<String> = conn
            .query_row(
                "SELECT c.parsed_json
                   FROM message_cache c
                   JOIN messages m ON m.id = c.message_id
                   JOIN folders f ON f.id = m.folder_id
                  WHERE f.account_id = ?1
                    AND m.folder_id != ?2
                    AND (
                        (?3 IS NOT NULL AND m.message_id = ?3 AND m.subject = ?4
                         AND m.from_json = ?5 AND m.to_json = ?6 AND m.size = ?7)
                        OR
                        (?3 IS NULL AND m.subject = ?4 AND m.from_json = ?5
                         AND m.to_json = ?6 AND m.date IS ?8 AND m.size = ?7)
                    )
                  ORDER BY c.cached_at DESC
                  LIMIT 1",
                params![
                    account_id, folder_id, message_id, subject, from_json, to_json, size, date,
                ],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| Error::Backend(format!("parsed message cache: {error}")))
        })
        .transpose()
    }

    /// Reflect parsed MIME metadata in the envelope list without changing
    /// server-owned flags or the cached body.
    pub fn set_has_attachment(
        &self,
        folder_id: &str,
        server_uid: u32,
        has_attachment: bool,
    ) -> Result<()> {
        self.conn()?.execute(
            "UPDATE messages SET has_attachment = ?3
              WHERE folder_id = ?1 AND server_uid = ?2",
            params![folder_id, server_uid as i64, has_attachment as i64],
        )?;
        Ok(())
    }

    /// Blob hash of a message body, if fetched.
    pub fn blob_hash(&self, folder_id: &str, server_uid: u32) -> Result<Option<String>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT blob_hash FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Set the computed thread id of a message.
    pub fn set_thread_id(&self, message_uuid: &str, thread_id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE messages SET thread_id = ?2 WHERE id = ?1",
            params![message_uuid, thread_id],
        )?;
        Ok(())
    }

    /// Look up the app-owned UUID of a message by (folder, server_uid).
    pub fn message_uuid(&self, folder_id: &str, server_uid: u32) -> Result<Option<(String,)>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT id FROM messages WHERE folder_id = ?1 AND server_uid = ?2",
                params![folder_id, server_uid as i64],
                |r| Ok((r.get::<_, String>(0)?,)),
            )
            .optional()?)
    }

    /// Search envelope metadata plus indexed bodies with lightweight
    /// `field:value` filters.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Envelope>> {
        self.search_page(query, 1, limit)
    }

    /// Search one page of envelope metadata plus indexed bodies.
    pub fn search_page(&self, query: &str, page: u32, page_size: u32) -> Result<Vec<Envelope>> {
        let conn = self.conn()?;
        let page_size = page_size.max(1);
        let mut sql = String::from(
            "SELECT m.id, m.folder_id, m.server_uid, m.message_id, m.thread_id, m.subject,
                    m.from_json, m.to_json, m.date, m.received_at, m.size, m.flags_json,
                    m.keywords_json, m.has_attachment, a.id
               FROM messages m
               JOIN folders f ON f.id = m.folder_id
               JOIN accounts a ON a.id = f.account_id
              WHERE 1 = 1",
        );
        let mut values = Vec::<Value>::new();
        for token in query.split_whitespace() {
            let (field, value) = token.split_once(':').unwrap_or(("", token));
            let like = || Value::Text(like_pattern(value));
            match field.to_ascii_lowercase().as_str() {
                "from" => {
                    sql.push_str(" AND m.from_json LIKE ? ESCAPE '\\'");
                    values.push(like());
                }
                "to" => {
                    sql.push_str(" AND m.to_json LIKE ? ESCAPE '\\'");
                    values.push(like());
                }
                "subject" => {
                    sql.push_str(" AND m.subject LIKE ? ESCAPE '\\'");
                    values.push(like());
                }
                "label" | "tag" => {
                    sql.push_str(" AND m.keywords_json LIKE ? ESCAPE '\\'");
                    values.push(like());
                }
                "account" => {
                    sql.push_str(" AND (a.name LIKE ? ESCAPE '\\' OR a.email LIKE ? ESCAPE '\\')");
                    values.push(like());
                    values.push(like());
                }
                "folder" => {
                    sql.push_str(" AND f.name LIKE ? ESCAPE '\\'");
                    values.push(like());
                }
                "is" if value.eq_ignore_ascii_case("unread") => {
                    sql.push_str(" AND m.flags_json NOT LIKE '%\"Seen\"%'");
                }
                "is" if value.eq_ignore_ascii_case("read") => {
                    sql.push_str(" AND m.flags_json LIKE '%\"Seen\"%'");
                }
                "is" if value.eq_ignore_ascii_case("starred") => {
                    sql.push_str(" AND m.flags_json LIKE '%\"Flagged\"%'");
                }
                "has" if value.eq_ignore_ascii_case("attachment") => {
                    sql.push_str(" AND m.has_attachment = 1");
                }
                _ => {
                    sql.push_str(
                        " AND (m.subject LIKE ? ESCAPE '\\'
                              OR m.from_json LIKE ? ESCAPE '\\'
                              OR m.to_json LIKE ? ESCAPE '\\'
                              OR m.rowid IN (
                                  SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?
                              ))",
                    );
                    values.push(like());
                    values.push(like());
                    values.push(like());
                    values.push(Value::Text(format!("\"{}\"", token.replace('"', "\"\""))));
                }
            }
        }
        sql.push_str(" ORDER BY m.received_at DESC, m.rowid DESC");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, String>(12)?,
                r.get::<_, i64>(13)?,
                r.get::<_, String>(14)?,
            ))
        })?;
        let mut physical = Vec::new();
        for row in rows {
            let (
                id,
                folder_id,
                uid,
                message_id,
                thread_id,
                subject,
                from_json,
                to_json,
                date,
                received_at,
                size,
                flags_json,
                keywords_json,
                has_attachment,
                account_id,
            ) = row?;
            physical.push((
                account_id,
                Envelope {
                    id,
                    mailbox_id: folder_id.clone(),
                    subject,
                    from: serde_json::from_str(&from_json).unwrap_or_default(),
                    to: serde_json::from_str(&to_json).unwrap_or_default(),
                    date,
                    received_at,
                    flags: serde_json::from_str(&flags_json).unwrap_or_default(),
                    keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
                    has_attachment: has_attachment != 0,
                    size: size as u32,
                    server_uid: Some(uid as u32),
                    message_id,
                    thread_id,
                    sources: vec![EnvelopeSource {
                        mailbox_id: folder_id,
                        server_uid: uid as u32,
                    }],
                },
            ));
        }

        let logical = deduplicate_envelopes(physical);
        let offset = page.max(1).saturating_sub(1).saturating_mul(page_size) as usize;
        Ok(logical
            .into_iter()
            .skip(offset)
            .take(page_size as usize)
            .collect())
    }

    pub fn list_saved_searches(&self) -> Result<Vec<SavedSearch>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT id, name, query FROM saved_searches ORDER BY created_at, name")?;
        let rows = stmt.query_map([], |row| {
            Ok(SavedSearch {
                id: row.get(0)?,
                name: row.get(1)?,
                query: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn save_search(&self, name: &str, query: &str) -> Result<SavedSearch> {
        let id = uuid::Uuid::now_v7().to_string();
        self.conn()?.execute(
            "INSERT INTO saved_searches (id, name, query, created_at)
             VALUES (?1, ?2, ?3, unixepoch())",
            params![id, name, query],
        )?;
        Ok(SavedSearch {
            id,
            name: name.to_string(),
            query: query.to_string(),
        })
    }

    pub fn delete_saved_search(&self, id: &str) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM saved_searches WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Distinct IMAP keywords with how many local messages carry each one.
    pub fn list_keywords(&self) -> Result<Vec<KeywordCount>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT json_each.value, COUNT(*)
               FROM messages, json_each(messages.keywords_json)
              WHERE json_each.value IS NOT NULL
                AND json_each.value != ''
              GROUP BY json_each.value
              ORDER BY COUNT(*) DESC, json_each.value COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(KeywordCount {
                name: row.get(0)?,
                count: row.get::<_, i64>(1)? as u32,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// Frequently seen senders and recipients for compose completion.
    pub fn list_correspondents(&self, limit: u32) -> Result<Vec<Correspondent>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT from_json, to_json FROM messages")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut contacts: HashMap<String, (Option<String>, u32)> = HashMap::new();
        for row in rows {
            let (from_json, to_json) = row?;
            for json in [from_json, to_json] {
                for address in serde_json::from_str::<Vec<Address>>(&json).unwrap_or_default() {
                    if address.addr.trim().is_empty() {
                        continue;
                    }
                    let key = address.addr.to_ascii_lowercase();
                    let entry = contacts.entry(key).or_insert((address.name.clone(), 0));
                    if entry.0.is_none() && address.name.is_some() {
                        entry.0 = address.name;
                    }
                    entry.1 += 1;
                }
            }
        }
        let mut contacts: Vec<_> = contacts
            .into_iter()
            .map(|(addr, (name, message_count))| Correspondent {
                name,
                addr,
                message_count,
            })
            .collect();
        contacts.sort_by(|a, b| {
            b.message_count
                .cmp(&a.message_count)
                .then_with(|| a.addr.cmp(&b.addr))
        });
        contacts.truncate(limit as usize);
        Ok(contacts)
    }

    // --------------------------------------------------------------
    // Outbox
    // --------------------------------------------------------------

    pub fn save_draft(&self, id: &str, draft_json: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO drafts (id, draft_json, updated_at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT(id) DO UPDATE SET draft_json = excluded.draft_json,
                 updated_at = excluded.updated_at",
            params![id, draft_json],
        )?;
        Ok(())
    }

    pub fn load_draft(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT draft_json FROM drafts WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn delete_draft(&self, id: &str) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM drafts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn draft_remote(&self, id: &str) -> Result<Option<(String, String, u32)>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT account_id, remote_mailbox, remote_uid FROM drafts
                  WHERE id = ?1 AND account_id IS NOT NULL
                    AND remote_mailbox IS NOT NULL AND remote_uid IS NOT NULL",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? as u32,
                    ))
                },
            )
            .optional()?)
    }

    pub fn set_draft_remote(
        &self,
        id: &str,
        account_id: &str,
        mailbox: &str,
        uid: u32,
    ) -> Result<()> {
        self.conn()?.execute(
            "UPDATE drafts SET account_id = ?2, remote_mailbox = ?3, remote_uid = ?4
              WHERE id = ?1",
            params![id, account_id, mailbox, uid as i64],
        )?;
        Ok(())
    }

    pub fn outbox_add(&self, account_id: &str, op: &OutboxOp) -> Result<i64> {
        let conn = self.conn()?;
        let op_json = serde_json::to_string(op)
            .map_err(|e| Error::Backend(format!("outbox op json: {e}")))?;
        conn.execute(
            "INSERT INTO outbox (account_id, op_json, created_at)
             VALUES (?1, ?2, unixepoch())",
            params![account_id, op_json],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn outbox_list(&self, account_id: &str) -> Result<Vec<OutboxEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, op_json, created_at, attempts, last_error, failed_at
               FROM outbox WHERE account_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, account_id, op_json, created_at, attempts, last_error, failed_at) = row?;
            let op: OutboxOp = serde_json::from_str(&op_json)
                .map_err(|e| Error::Backend(format!("outbox op parse: {e}")))?;
            out.push(OutboxEntry {
                id,
                account_id,
                op,
                created_at,
                attempts: attempts as u32,
                last_error,
                failed_at,
            });
        }
        Ok(out)
    }

    pub fn outbox_count(&self, account_id: &str) -> Result<u32> {
        let count: i64 = self.conn()?.query_row(
            "SELECT COUNT(*) FROM outbox WHERE account_id = ?1 AND failed_at IS NULL",
            params![account_id],
            |row| row.get(0),
        )?;
        u32::try_from(count).map_err(|_| Error::Backend("outbox count exceeds u32".into()))
    }

    /// Terminal-failed rows; the Outbox entry points surface these so a fully
    /// poisoned outbox stays reachable (with per-row Retry).
    pub fn outbox_failed_count(&self, account_id: &str) -> Result<u32> {
        let count: i64 = self.conn()?.query_row(
            "SELECT COUNT(*) FROM outbox WHERE account_id = ?1 AND failed_at IS NOT NULL",
            params![account_id],
            |row| row.get(0),
        )?;
        u32::try_from(count).map_err(|_| Error::Backend("outbox count exceeds u32".into()))
    }

    pub fn outbox_remove(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM outbox WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn outbox_mark_failed(&self, id: i64, error: &str) -> Result<()> {
        let conn = self.conn()?;
        // Errors can embed credentials from protocol chatter; never persist them.
        let redacted = crate::redact::redact_secrets(error);
        conn.execute(
            "UPDATE outbox SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1",
            params![id, redacted],
        )?;
        Ok(())
    }

    /// Move an op to the terminal failed state (poison: never auto-retried).
    pub fn outbox_fail_permanent(&self, id: i64, error: &str) -> Result<()> {
        let conn = self.conn()?;
        let redacted = crate::redact::redact_secrets(error);
        conn.execute(
            "UPDATE outbox SET failed_at = unixepoch(), last_error = ?2 WHERE id = ?1",
            params![id, redacted],
        )?;
        Ok(())
    }

    /// Reopen a terminal-failed op for retry (explicit user action).
    pub fn outbox_reopen(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE outbox SET failed_at = NULL, attempts = 0, last_error = NULL WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

fn deduplicate_envelopes(physical: Vec<(String, Envelope)>) -> Vec<Envelope> {
    let mut logical: Vec<Envelope> = Vec::new();
    let mut groups = HashMap::<String, Vec<usize>>::new();

    for (account_id, envelope) in physical {
        let key = envelope.logical_id(&account_id);
        let matching_group = groups.get(&key).and_then(|indices| {
            indices.iter().copied().find(|index| {
                !logical[*index].sources.iter().any(|existing| {
                    envelope
                        .sources
                        .iter()
                        .any(|incoming| incoming.mailbox_id == existing.mailbox_id)
                })
            })
        });
        if let Some(index) = matching_group {
            merge_envelope_sources(&mut logical[index], envelope);
        } else {
            let index = logical.len();
            logical.push(envelope);
            groups.entry(key).or_default().push(index);
        }
    }

    logical
}

fn merge_envelope_sources(existing: &mut Envelope, incoming: Envelope) {
    let all_sources = existing
        .sources
        .iter()
        .chain(incoming.sources.iter())
        .cloned()
        .collect::<HashSet<_>>();
    existing.sources = all_sources.into_iter().collect();
    existing.sources.sort_by(|left, right| {
        left.mailbox_id
            .cmp(&right.mailbox_id)
            .then_with(|| left.server_uid.cmp(&right.server_uid))
    });

    let all_seen = existing.flags.contains(&Flag::Seen) && incoming.flags.contains(&Flag::Seen);
    for flag in incoming.flags {
        if flag != Flag::Seen && !existing.flags.contains(&flag) {
            existing.flags.push(flag);
        }
    }
    if all_seen {
        if !existing.flags.contains(&Flag::Seen) {
            existing.flags.push(Flag::Seen);
        }
    } else {
        existing.flags.retain(|flag| *flag != Flag::Seen);
    }

    for keyword in incoming.keywords {
        if !existing.keywords.contains(&keyword) {
            existing.keywords.push(keyword);
        }
    }
    existing.keywords.sort();
    existing.has_attachment |= incoming.has_attachment;
}

/// Flatten stored address JSON into searchable text.
fn address_text(json: &str) -> String {
    let addrs: Vec<crate::model::Address> = serde_json::from_str(json).unwrap_or_default();
    addrs
        .into_iter()
        .map(|a| match a.name {
            Some(name) => format!("{name} <{}>", a.addr),
            None => a.addr,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn received_at_from_date(date: Option<&str>) -> Option<i64> {
    date.and_then(mail_parser::DateTime::parse_rfc822)
        .map(|value| value.to_timestamp())
}

fn like_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
