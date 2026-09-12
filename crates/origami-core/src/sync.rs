//! Sync engine (ADR-0003): converges the local store with the server.
//!
//! Layout:
//! - [`plan_sync`] — pure decision core (property-tested)
//! - [`SyncEngine`] — store + blobs + backend, event broadcast to the UI
//!
//! M2 implements one-shot sync (`sync_account`) plus a watcher loop
//! (`spawn_account`) that uses io-imap's QRESYNC/IDLE watch stream when
//! the server offers it, falling back to a 60 s poll.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::backend::MailBackend;
use crate::blob::BlobStore;
use crate::config::AccountConfig;
use crate::imap::ImapBackend;
use crate::message::{parse as parse_message, parse_display, DisplayMessage, ParsedMessage};
use crate::model::{Envelope, EnvelopeSource, Flag, Mailbox, OutboxOp, SyncState};
use crate::smtp::OrigamiSmtp;
use crate::store::{PrefetchCandidate, Store};
use crate::{Error, Result, SmtpSender};
use base64::Engine;

/// Events emitted to subscribers (UI in M3, CLI `--watch` today).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncEvent {
    #[serde(rename_all = "camelCase")]
    AccountSyncStarted { account_id: String },
    #[serde(rename_all = "camelCase")]
    FolderSynced {
        account_id: String,
        folder: String,
        stats: FolderSyncStats,
    },
    #[serde(rename_all = "camelCase")]
    NewEnvelope {
        account_id: String,
        folder: String,
        envelope: Box<Envelope>,
    },
    #[serde(rename_all = "camelCase")]
    FlagsChanged {
        account_id: String,
        folder: String,
        server_uid: u32,
    },
    #[serde(rename_all = "camelCase")]
    AccountSynced { account_id: String },
    #[serde(rename_all = "camelCase")]
    OutboxChanged { account_id: String },
    #[serde(rename_all = "camelCase")]
    Error { account_id: String, message: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSyncStats {
    pub added: u32,
    pub changed: u32,
    pub removed: u32,
}

/// What to do with a folder, decided purely from stored vs. selected
/// state (ADR-0003 §1–2). Unit- and property-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    /// UIDVALIDITY flipped: wipe local rows, resync from scratch.
    FullResync,
    /// No usable local baseline: page the whole folder.
    InitialSync,
    /// Pull new UIDs after `after_uid`; pull flag changes with
    /// CHANGEDSINCE `changed_since` when the server offers CONDSTORE.
    Delta {
        after_uid: u32,
        changed_since: Option<u64>,
    },
    /// Nothing to do (empty folder, no baseline).
    Noop,
}

type BodyLockMap = Mutex<HashMap<(String, u32), Arc<tokio::sync::Mutex<()>>>>;

const PREFETCH_WINDOW: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const PREFETCH_MAX_MESSAGES: u32 = 500;
const PREFETCH_MAX_MESSAGE_SIZE: u32 = 8 * 1024 * 1024;

/// Decide the sync action for a folder.
///
/// - `stored`: local checkpoint (`None` = never synced)
/// - `uid_validity`: server's current UIDVALIDITY (`None` = unknown → 0)
/// - `exists`: server's current message count
/// - `condstore`: server advertised CONDSTORE/QRESYNC
pub fn plan_sync(
    stored: Option<SyncState>,
    uid_validity: Option<u32>,
    exists: u32,
    condstore: bool,
) -> SyncAction {
    let server_uv = uid_validity.unwrap_or(0);
    match stored {
        None => {
            if exists == 0 {
                SyncAction::Noop
            } else {
                SyncAction::InitialSync
            }
        }
        Some(state) if state.uid_validity != server_uv => SyncAction::FullResync,
        Some(state) => {
            if state.last_uid == 0 && exists > 0 {
                SyncAction::InitialSync
            } else if exists == 0 && state.last_uid == 0 {
                SyncAction::Noop
            } else {
                SyncAction::Delta {
                    after_uid: state.last_uid,
                    changed_since: if condstore && state.highest_modseq > 0 {
                        Some(state.highest_modseq)
                    } else {
                        None
                    },
                }
            }
        }
    }
}

/// The synchronization engine: one store, one blob store, one event bus.
pub struct SyncEngine {
    store: Store,
    blobs: BlobStore,
    events: broadcast::Sender<SyncEvent>,
    account_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    body_locks: BodyLockMap,
    prefetch_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl SyncEngine {
    pub fn new(store: Store, blobs: BlobStore) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            store,
            blobs,
            events,
            account_locks: Mutex::new(HashMap::new()),
            body_locks: Mutex::new(HashMap::new()),
            prefetch_locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.events.subscribe()
    }

    fn emit(&self, event: SyncEvent) {
        // No subscribers is normal (CLI one-shots); never an error.
        let _ = self.events.send(event);
    }

    fn emit_outbox_changed(&self, account_db_id: &str) {
        if let Ok(Some(account_id)) = self.store.account_config_id(account_db_id) {
            self.emit(SyncEvent::OutboxChanged { account_id });
        }
    }

    /// One-shot full convergence of an account: folders, envelopes,
    /// flag deltas, then outbox replay.
    pub async fn sync_account(
        &self,
        account_config_id: &str,
        config: &AccountConfig,
    ) -> Result<Vec<(String, FolderSyncStats)>> {
        let account_lock = self
            .account_locks
            .lock()
            .unwrap()
            .entry(account_config_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = account_lock.lock().await;
        self.emit(SyncEvent::AccountSyncStarted {
            account_id: account_config_id.to_string(),
        });
        let result = self.sync_account_inner(account_config_id, config).await;
        if let Err(error) = &result {
            self.emit(SyncEvent::Error {
                account_id: account_config_id.to_string(),
                message: error.to_string(),
            });
        }
        result
    }

    /// Schedule a non-blocking warm-cache pass for messages received during
    /// the last seven days. At most one pass runs per account at a time.
    pub fn spawn_recent_prefetch(
        self: &Arc<Self>,
        account_config_id: String,
        config: AccountConfig,
        prefer_folder_id: Option<String>,
    ) {
        // Fire-and-forget optimization, reachable from threads that have no
        // Tokio runtime (Tauri runs synchronous commands on the main thread).
        // `tokio::spawn` panics there, and that panic crosses an FFI boundary
        // and aborts the process, so skip the pass rather than take the app down.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::debug!("recent message prefetch skipped: no Tokio runtime on this thread");
            return;
        };
        let lock = self
            .prefetch_locks
            .lock()
            .unwrap()
            .entry(account_config_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let engine = Arc::clone(self);
        runtime.spawn(async move {
            let _guard = lock.lock().await;
            match engine
                .prefetch_recent(&account_config_id, &config, prefer_folder_id.as_deref())
                .await
            {
                Ok(cached) if cached > 0 => {
                    tracing::debug!(account = %account_config_id, cached, "warmed recent message cache");
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::debug!(account = %account_config_id, %error, "recent message prefetch skipped");
                }
            }
        });
    }

    /// Fetch and cache display-only MIME data without downloading attachment
    /// bytes or remote resources. Failed candidates remain uncached and are
    /// retried on the next successful synchronization pass.
    pub async fn prefetch_recent(
        &self,
        account_config_id: &str,
        config: &AccountConfig,
        prefer_folder_id: Option<&str>,
    ) -> Result<u32> {
        let imap = config
            .imap
            .as_ref()
            .ok_or_else(|| Error::Config(format!("account `{account_config_id}` has no IMAP")))?;
        let backend = ImapBackend::connect(account_config_id, imap).await?;
        let account_db_id =
            self.store
                .upsert_account(account_config_id, &config.name, &config.email)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| Error::Backend(format!("system clock before Unix epoch: {error}")))?;
        let since = now.as_secs().saturating_sub(PREFETCH_WINDOW.as_secs()) as i64;
        let candidates = group_prefetch_candidates(
            &account_db_id,
            self.store.recent_uncached_messages(
                &account_db_id,
                since,
                PREFETCH_MAX_MESSAGES,
                prefer_folder_id,
            )?,
        );

        let mut cached = 0u32;
        for candidate in candidates {
            if candidate.envelope.size > PREFETCH_MAX_MESSAGE_SIZE {
                continue;
            }
            let Some(source) = candidate.envelope.sources.first() else {
                continue;
            };
            let Some(server_uid) = candidate.envelope.server_uid else {
                continue;
            };
            let display = match backend
                .fetch_display_message(&candidate.mailbox, server_uid)
                .await
            {
                Ok(display) => display,
                Err(error) => {
                    tracing::debug!(
                        account = %account_config_id,
                        mailbox = %candidate.mailbox,
                        server_uid,
                        %error,
                        "display prefetch failed"
                    );
                    continue;
                }
            };
            let fallback = format!("display:{}:{}", source.mailbox_id, source.server_uid);
            if self
                .cache_display_message_sources(&candidate.envelope.sources, &display, &fallback)
                .is_ok()
            {
                cached += 1;
            }
        }
        Ok(cached)
    }

    async fn sync_account_inner(
        &self,
        account_config_id: &str,
        config: &AccountConfig,
    ) -> Result<Vec<(String, FolderSyncStats)>> {
        let imap = config
            .imap
            .as_ref()
            .ok_or_else(|| Error::Config(format!("account `{account_config_id}` has no IMAP")))?;
        let backend = ImapBackend::connect(account_config_id, imap).await?;
        let account_db_id =
            self.store
                .upsert_account(account_config_id, &config.name, &config.email)?;

        let mailboxes = backend.list_mailboxes().await?;
        let remote_names: Vec<String> = mailboxes
            .iter()
            .map(|mailbox| mailbox.name.clone())
            .collect();
        // LIST succeeded, so the result is authoritative. Reconcile before
        // syncing individual folders to avoid retaining deleted mailboxes.
        self.store
            .remove_folders_not_in(&account_db_id, &remote_names)?;

        let mut results = Vec::new();
        let mut folder_errors = Vec::new();
        for mailbox in mailboxes {
            let folder_db_id =
                self.store
                    .upsert_folder(&account_db_id, &mailbox.name, mailbox.role)?;
            match self
                .sync_folder(&backend, account_config_id, &folder_db_id, &mailbox)
                .await
            {
                Ok(stats) => results.push((mailbox.name.clone(), stats)),
                Err(error) => {
                    folder_errors.push(format!("{}: {error}", mailbox.name));
                }
            }
        }

        self.replay_outbox(&backend, &account_db_id, config).await;
        if !folder_errors.is_empty() {
            return Err(Error::Backend(format!(
                "{} folder sync failed: {}",
                folder_errors.len(),
                folder_errors.join("; ")
            )));
        }
        self.emit(SyncEvent::AccountSynced {
            account_id: account_config_id.to_string(),
        });
        Ok(results)
    }

    /// Converge one folder.
    pub async fn sync_folder(
        &self,
        backend: &ImapBackend,
        account_id: &str,
        folder_db_id: &str,
        mailbox: &Mailbox,
    ) -> Result<FolderSyncStats> {
        let condstore = backend.supports_condstore();
        let selected = backend.select_folder(&mailbox.name, condstore).await?;
        let stored = self.store.sync_state(folder_db_id)?;
        let action = plan_sync(stored, selected.uid_validity, selected.exists, condstore);

        let mut stats = FolderSyncStats::default();
        match action {
            SyncAction::Noop => {}

            SyncAction::FullResync => {
                stats.removed = self.store.clear_folder_messages(folder_db_id)? as u32;
                stats.added = self
                    .initial_sync(backend, folder_db_id, &mailbox.name)
                    .await?;
            }

            SyncAction::InitialSync => {
                stats.added = self
                    .initial_sync(backend, folder_db_id, &mailbox.name)
                    .await?;
            }

            SyncAction::Delta {
                after_uid,
                changed_since,
            } => {
                // 1. New arrivals.
                let new_uids = backend.search_uids_after(&mailbox.name, after_uid).await?;
                if !new_uids.is_empty() {
                    let envelopes = backend
                        .fetch_envelopes_by_uids(&mailbox.name, &new_uids)
                        .await?;
                    for envelope in envelopes {
                        let (_, inserted) = self.store.upsert_envelope(folder_db_id, &envelope)?;
                        if inserted {
                            self.emit(SyncEvent::NewEnvelope {
                                account_id: account_id.to_string(),
                                folder: folder_db_id.to_string(),
                                envelope: Box::new(envelope),
                            });
                            stats.added += 1;
                        }
                    }
                }

                // 2. Flag changes (CONDSTORE CHANGEDSINCE, else full sweep).
                let changes = backend.fetch_flags(&mailbox.name, changed_since).await?;
                for (uid, flags, keywords) in changes {
                    let flags_changed = self.store.update_flags(folder_db_id, uid, &flags)?;
                    let keywords_changed =
                        self.store.update_keywords(folder_db_id, uid, &keywords)?;
                    if flags_changed || keywords_changed {
                        self.emit(SyncEvent::FlagsChanged {
                            account_id: account_id.to_string(),
                            folder: folder_db_id.to_string(),
                            server_uid: uid,
                        });
                        stats.changed += 1;
                    }
                }

                // UIDs can disappear below the high-water mark after an
                // expunge or a move, so new-mail searches alone cannot detect
                // them. A UID-only SEARCH is much cheaper than refetching
                // envelopes and keeps the local folder convergent.
                let remote_uids: HashSet<u32> = backend
                    .search_all_uids(&mailbox.name)
                    .await?
                    .into_iter()
                    .collect();
                for uid in self.store.message_uids(folder_db_id)? {
                    if !remote_uids.contains(&uid)
                        && self.store.delete_message_by_uid(folder_db_id, uid)?
                    {
                        stats.removed += 1;
                    }
                }
            }
        }

        // Checkpoint (ADR-0003 §2): keep UIDVALIDITY/modseq from SELECT,
        // last_uid from what we actually stored (robust against holes).
        let last_uid = self.store.max_server_uid(folder_db_id)?;
        self.store.set_sync_state(
            folder_db_id,
            &SyncState {
                uid_validity: selected.uid_validity.unwrap_or(0),
                highest_modseq: selected.highest_modseq.unwrap_or(0),
                last_uid,
            },
        )?;

        self.emit(SyncEvent::FolderSynced {
            account_id: account_id.to_string(),
            folder: folder_db_id.to_string(),
            stats,
        });
        Ok(stats)
    }

    /// Page the whole folder into the store, newest → oldest.
    async fn initial_sync(
        &self,
        backend: &ImapBackend,
        folder_db_id: &str,
        mailbox: &str,
    ) -> Result<u32> {
        const PAGE_SIZE: u32 = 200;
        let mut added = 0u32;
        let mut page = 1u32;
        loop {
            let envelopes = backend.list_envelopes(mailbox, page, PAGE_SIZE).await?;
            if envelopes.is_empty() {
                break;
            }
            let count = envelopes.len() as u32;
            self.store.upsert_envelopes(folder_db_id, &envelopes)?;
            added += count;
            if count < PAGE_SIZE {
                break;
            }
            page += 1;
        }
        Ok(added)
    }

    /// Fetch, blob, and index a message body (idempotent). Also assigns
    /// the thread id from the message's References ancestry.
    pub async fn ensure_body(
        &self,
        backend: &ImapBackend,
        folder_db_id: &str,
        mailbox: &str,
        server_uid: u32,
    ) -> Result<String> {
        let body_lock = self
            .body_locks
            .lock()
            .unwrap()
            .entry((folder_db_id.to_string(), server_uid))
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = body_lock.lock().await;

        if let Some(hash) = self.store.blob_hash(folder_db_id, server_uid)? {
            if self
                .store
                .parsed_message(folder_db_id, server_uid)?
                .is_none()
            {
                let raw = self.blobs.get(&hash)?;
                self.cache_parsed_message(folder_db_id, server_uid, &hash, &raw)?;
            }
            return Ok(hash);
        }
        let raw = backend.fetch_message(mailbox, server_uid).await?;
        let hash = self.blobs.put(&raw)?;
        self.cache_parsed_message(folder_db_id, server_uid, &hash, &raw)?;
        Ok(hash)
    }

    /// Parse and cache a blob that was fetched by an older version or by a
    /// read path that already has the raw body locally.
    pub fn cache_parsed_message(
        &self,
        folder_db_id: &str,
        server_uid: u32,
        hash: &str,
        raw: &[u8],
    ) -> Result<ParsedMessage> {
        let parsed = parse_message(raw)
            .ok_or_else(|| Error::Backend("message MIME parsing failed".into()))?;
        // Threading: root of the References chain, else own Message-ID.
        let thread_id = parsed
            .headers
            .references
            .first()
            .cloned()
            .or_else(|| parsed.headers.message_id.clone())
            .unwrap_or_else(|| hash.to_string());
        self.store
            .set_body_and_cache(folder_db_id, server_uid, hash, &parsed, &thread_id)?;
        Ok(parsed)
    }

    /// Cache normalized display data fetched without a complete RFC 822 body.
    pub fn cache_display_message(
        &self,
        folder_db_id: &str,
        server_uid: u32,
        display: &DisplayMessage,
    ) -> Result<ParsedMessage> {
        let source = EnvelopeSource {
            mailbox_id: folder_db_id.to_string(),
            server_uid,
        };
        let fallback = format!("display:{folder_db_id}:{server_uid}");
        self.cache_display_message_sources(&[source], display, &fallback)
    }

    /// Cache one normalized display representation for every physical source
    /// retained by a logical message. This prevents opening a provider label
    /// copy from causing a second network fetch.
    pub fn cache_display_message_sources(
        &self,
        sources: &[EnvelopeSource],
        display: &DisplayMessage,
        fallback_thread_id: &str,
    ) -> Result<ParsedMessage> {
        if sources.is_empty() {
            return Err(Error::Backend(
                "display message has no physical source".into(),
            ));
        }
        let parsed = parse_display(display)
            .ok_or_else(|| Error::Backend("display MIME parsing failed".into()))?;
        let thread_id = parsed
            .headers
            .references
            .first()
            .cloned()
            .or_else(|| parsed.headers.message_id.clone())
            .unwrap_or_else(|| fallback_thread_id.to_string());
        let mut cached_any = false;
        for source in sources {
            if self
                .store
                .set_parsed_message_and_index(
                    &source.mailbox_id,
                    source.server_uid,
                    &parsed,
                    &thread_id,
                )
                .is_ok()
            {
                cached_any = true;
            }
        }
        if !cached_any {
            return Err(Error::Backend("display cache source disappeared".into()));
        }
        Ok(parsed)
    }

    /// Queue a flag change: optimistic local update + outbox entry.
    pub fn queue_store_flags(
        &self,
        account_db_id: &str,
        folder_db_id: &str,
        mailbox: &str,
        server_uid: u32,
        flags: &[Flag],
    ) -> Result<()> {
        self.store.update_flags(folder_db_id, server_uid, flags)?;
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::StoreFlags {
                mailbox: mailbox.to_string(),
                server_uid,
                flags: flags.to_vec(),
                keywords: None,
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    pub fn queue_store_keywords(
        &self,
        account_db_id: &str,
        folder_db_id: &str,
        mailbox: &str,
        server_uid: u32,
        flags: &[Flag],
        keywords: &[String],
    ) -> Result<()> {
        self.store.update_flags(folder_db_id, server_uid, flags)?;
        self.store
            .update_keywords(folder_db_id, server_uid, keywords)?;
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::StoreFlags {
                mailbox: mailbox.to_string(),
                server_uid,
                flags: flags.to_vec(),
                keywords: Some(keywords.to_vec()),
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    pub fn queue_move_messages(
        &self,
        account_db_id: &str,
        source_mailbox: &str,
        destination_mailbox: &str,
        server_uids: &[u32],
    ) -> Result<()> {
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::MoveMessages {
                source_mailbox: source_mailbox.to_string(),
                destination_mailbox: destination_mailbox.to_string(),
                server_uids: server_uids.to_vec(),
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    pub fn queue_delete_messages(
        &self,
        account_db_id: &str,
        mailbox: &str,
        server_uids: &[u32],
    ) -> Result<()> {
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::DeleteMessages {
                mailbox: mailbox.to_string(),
                server_uids: server_uids.to_vec(),
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    pub fn queue_send_message(&self, account_db_id: &str, raw: &[u8]) -> Result<()> {
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::SendMessage {
                raw_base64: base64::engine::general_purpose::STANDARD.encode(raw),
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    pub fn queue_append_sent(&self, account_db_id: &str, raw: &[u8]) -> Result<()> {
        self.store.outbox_add(
            account_db_id,
            &OutboxOp::AppendSent {
                raw_base64: base64::engine::general_purpose::STANDARD.encode(raw),
            },
        )?;
        self.emit_outbox_changed(account_db_id);
        Ok(())
    }

    /// Replay queued offline ops, oldest first; failures stay queued.
    pub async fn replay_outbox(
        &self,
        backend: &ImapBackend,
        account_db_id: &str,
        config: &AccountConfig,
    ) {
        for entry in self.store.outbox_list(account_db_id).unwrap_or_default() {
            let result = match &entry.op {
                OutboxOp::StoreFlags {
                    mailbox,
                    server_uid,
                    flags,
                    keywords,
                } => {
                    backend
                        .store_flags_and_keywords(mailbox, *server_uid, flags, keywords.as_deref())
                        .await
                }
                OutboxOp::MoveMessages {
                    source_mailbox,
                    destination_mailbox,
                    server_uids,
                } => {
                    backend
                        .move_messages(source_mailbox, destination_mailbox, server_uids)
                        .await
                }
                OutboxOp::DeleteMessages {
                    mailbox,
                    server_uids,
                } => backend.delete_messages(mailbox, server_uids).await,
                OutboxOp::SendMessage { raw_base64 } => {
                    async {
                        let raw = base64::engine::general_purpose::STANDARD
                            .decode(raw_base64)
                            .map_err(|error| Error::Backend(format!("queued message: {error}")))?;
                        let smtp = config.smtp.as_ref().ok_or_else(|| {
                            Error::Config("account has no SMTP configuration".into())
                        })?;
                        let sender = OrigamiSmtp::connect(smtp).await?;
                        sender.send_message(&raw).await?;
                        let mut appended = false;
                        if let Ok(mailboxes) = backend.list_mailboxes().await {
                            if let Some(sent) = mailboxes
                                .iter()
                                .find(|mailbox| mailbox.role == crate::model::MailboxRole::Sent)
                            {
                                appended = backend
                                    .append_message(&sent.name, &raw, &[Flag::Seen])
                                    .await
                                    .is_ok();
                            }
                        }
                        if !appended {
                            self.queue_append_sent(account_db_id, &raw)?;
                        }
                        Ok(())
                    }
                    .await
                }
                OutboxOp::AppendSent { raw_base64 } => {
                    async {
                        let raw = base64::engine::general_purpose::STANDARD
                            .decode(raw_base64)
                            .map_err(|error| {
                                Error::Backend(format!("queued Sent copy: {error}"))
                            })?;
                        let mailboxes = backend.list_mailboxes().await?;
                        let sent = mailboxes
                            .iter()
                            .find(|mailbox| mailbox.role == crate::model::MailboxRole::Sent)
                            .ok_or_else(|| Error::Backend("Sent mailbox not found".into()))?;
                        backend
                            .append_message(&sent.name, &raw, &[Flag::Seen])
                            .await?;
                        Ok(())
                    }
                    .await
                }
            };
            match result {
                Ok(()) => {
                    let _ = self.store.outbox_remove(entry.id);
                }
                Err(error) => {
                    let _ = self.store.outbox_mark_failed(entry.id, &error.to_string());
                }
            }
        }
        self.emit_outbox_changed(account_db_id);
    }

    /// Long-running per-account loop: sync, then watch INBOX (IDLE via
    /// QRESYNC watch stream) or poll; any wake triggers a resync.
    /// Checks `cancel` at every yield point so the caller can stop the
    /// loop cleanly.
    pub async fn run_account_loop(
        self: Arc<Self>,
        account_config_id: String,
        config: AccountConfig,
        cancel: CancellationToken,
    ) {
        const POLL_INTERVAL: Duration = Duration::from_secs(60);
        const WATCH_TIMEOUT: Duration = Duration::from_secs(25 * 60);
        let mut backoff = Duration::from_secs(1);

        loop {
            if cancel.is_cancelled() {
                return;
            }

            if self
                .sync_account(&account_config_id, &config)
                .await
                .is_err()
            {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(backoff) => {},
                }
                backoff = (backoff * 2).min(Duration::from_secs(300));
                continue;
            }
            backoff = Duration::from_secs(1);
            self.spawn_recent_prefetch(account_config_id.clone(), config.clone(), None);

            let Some(imap) = config.imap.clone() else {
                return;
            };
            let cancel2 = cancel.clone();
            let woke = tokio::task::spawn_blocking(move || {
                let stream = ImapBackend::watch_mailbox_blocking(&imap, "INBOX")?;
                let deadline = std::time::Instant::now() + WATCH_TIMEOUT;
                loop {
                    if cancel2.is_cancelled() {
                        let _ = stream.close();
                        return Err(Error::Backend("cancelled".into()));
                    }
                    let remaining = deadline - std::time::Instant::now();
                    if remaining.is_zero() {
                        break;
                    }
                    match stream.recv_timeout(Duration::from_secs(5)) {
                        Ok(Ok(_)) => return Ok(()),
                        Ok(Err(..)) => return Err(Error::Backend("watch error".into())),
                        Err(..) => {} // timeout
                    }
                }
                Ok(())
            })
            .await;

            match woke {
                Ok(Ok(())) => {}
                Ok(Err(..)) | Err(..) => {
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(POLL_INTERVAL) => {},
                    }
                }
            }
        }
    }
}

fn group_prefetch_candidates(
    account_id: &str,
    candidates: Vec<PrefetchCandidate>,
) -> Vec<PrefetchCandidate> {
    let mut grouped = Vec::new();
    for candidate in candidates {
        let key = candidate.envelope.logical_id(account_id);
        let same_logical_message = grouped
            .iter_mut()
            .find(|existing: &&mut PrefetchCandidate| {
                existing.envelope.logical_id(account_id) == key
                    && candidate.envelope.sources.iter().all(|incoming| {
                        existing
                            .envelope
                            .sources
                            .iter()
                            .all(|current| current.mailbox_id != incoming.mailbox_id)
                    })
            });
        if let Some(existing) = same_logical_message {
            for source in candidate.envelope.sources {
                if !existing.envelope.sources.contains(&source) {
                    existing.envelope.sources.push(source);
                }
            }
        } else {
            grouped.push(candidate);
        }
    }
    grouped
}

/// Extract indexable plain text from a raw RFC 822 message: prefer the
/// first text/plain part; fall back to tag-stripped text/html.
pub fn extract_body_text(raw: &[u8]) -> String {
    let Some(parsed) = mail_parser::MessageParser::default().parse(raw) else {
        return String::new();
    };
    if let Some(text) = parsed.body_text(0) {
        return text.into_owned();
    }
    if let Some(html) = parsed.body_html(0) {
        return strip_html_tags(&html);
    }
    String::new()
}

/// Naive tag stripper for FTS input (display sanitization is a UI concern).
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Parse References + In-Reply-To out of a raw message for threading.
pub fn parse_references(raw: &[u8]) -> (Option<String>, Vec<String>) {
    let Some(parsed) = mail_parser::MessageParser::default().parse(raw) else {
        return (None, Vec::new());
    };
    let message_id = parsed.message_id().map(|s| s.to_string());
    let mut references: Vec<String> = parsed
        .references()
        .as_text_list()
        .map(|list| list.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    if let Some(irt) = parsed.in_reply_to().as_text() {
        let irt = irt.to_string();
        if !references.contains(&irt) {
            references.push(irt);
        }
    }
    (message_id, references)
}
