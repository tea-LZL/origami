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
    prefetch: crate::prefetch_queue::PrefetchQueue,
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
            prefetch: crate::prefetch_queue::PrefetchQueue::new(3),
        }
    }

    /// Priority queue driving UI-triggered display prefetch.
    pub fn prefetch_queue(&self) -> &crate::prefetch_queue::PrefetchQueue {
        &self.prefetch
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
            let Ok(_guard) = acquire_bounded(lock, LOCK_WAIT_BOUND).await else {
                tracing::debug!(account = %account_config_id, "recent prefetch skipped: lock busy");
                return;
            };
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
        let backend = with_network_timeout(ImapBackend::connect(account_config_id, imap)).await?;
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
            let display = match with_network_timeout(
                backend.fetch_display_message(&candidate.mailbox, server_uid),
            )
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
        // Keep the display cache bounded after warming a batch.
        self.store.evict_display_cache(2000, 30)?;
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
        let backend = with_network_timeout(ImapBackend::connect(account_config_id, imap)).await?;
        let account_db_id =
            self.store
                .upsert_account(account_config_id, &config.name, &config.email)?;

        let mailboxes = with_network_timeout(backend.list_mailboxes()).await?;
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
        let selected = with_network_timeout(backend.select_folder(&mailbox.name, condstore)).await?;
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
                let new_uids = with_network_timeout(backend.search_uids_after(&mailbox.name, after_uid)).await?;
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
                let changes = with_network_timeout(backend.fetch_flags(&mailbox.name, changed_since)).await?;
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
            let envelopes = with_network_timeout(backend.list_envelopes(mailbox, page, PAGE_SIZE)).await?;
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
        let _guard = acquire_bounded(body_lock, LOCK_WAIT_BOUND).await?;

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
        let raw = with_network_timeout(backend.fetch_message(mailbox, server_uid)).await?;
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
        self.store.update_flags_with_outbox(
            account_db_id,
            folder_db_id,
            mailbox,
            server_uid,
            flags,
            None,
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
        self.store.update_flags_with_outbox(
            account_db_id,
            folder_db_id,
            mailbox,
            server_uid,
            flags,
            Some(keywords),
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
            if entry.failed_at.is_some() {
                continue;
            }
            let result = match &entry.op {
                OutboxOp::StoreFlags {
                    mailbox,
                    server_uid,
                    flags,
                    keywords,
                } => {
                    with_network_timeout(backend.store_flags_and_keywords(
                        mailbox,
                        *server_uid,
                        flags,
                        keywords.as_deref(),
                    ))
                    .await
                }
                OutboxOp::MoveMessages {
                    source_mailbox,
                    destination_mailbox,
                    server_uids,
                } => {
                    with_network_timeout(backend.move_messages(
                        source_mailbox,
                        destination_mailbox,
                        server_uids,
                    ))
                    .await
                }
                OutboxOp::DeleteMessages {
                    mailbox,
                    server_uids,
                } => {
                    with_network_timeout(backend.delete_messages(mailbox, server_uids)).await
                }
                OutboxOp::SendMessage { raw_base64 } => {
                    async {
                        let raw = base64::engine::general_purpose::STANDARD
                            .decode(raw_base64)
                            .map_err(|error| Error::Backend(format!("queued message: {error}")))?;
                        let smtp = config.smtp.as_ref().ok_or_else(|| {
                            Error::Config("account has no SMTP configuration".into())
                        })?;
                        let sender =
                            with_network_timeout(OrigamiSmtp::connect(smtp)).await?;
                        with_network_timeout(sender.send_message(&raw)).await?;
                        let mut appended = false;
                        if let Ok(mailboxes) = backend.list_mailboxes().await {
                            if let Some(sent) = mailboxes
                                .iter()
                                .find(|mailbox| mailbox.role == crate::model::MailboxRole::Sent)
                            {
                                appended = with_network_timeout(
                                    backend.append_message(&sent.name, &raw, &[Flag::Seen]),
                                )
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
                        let mailboxes = with_network_timeout(backend.list_mailboxes()).await?;
                        let sent = mailboxes
                            .iter()
                            .find(|mailbox| mailbox.role == crate::model::MailboxRole::Sent)
                            .ok_or_else(|| Error::Backend("Sent mailbox not found".into()))?;
                        with_network_timeout(backend.append_message(&sent.name, &raw, &[Flag::Seen]))
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
                    let secrets = config_secret_values(config);
                    let message = crate::redact::redact_with(
                        &error.to_string(),
                        &secrets.iter().map(String::as_str).collect::<Vec<_>>(),
                    );
                    if outbox_failure_is_terminal(&error, entry.attempts + 1) {
                        let _ = self.store.outbox_fail_permanent(entry.id, &message);
                    } else {
                        let _ = self.store.outbox_mark_failed(entry.id, &message);
                    }
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
        const WATCH_TIMEOUT: Duration = Duration::from_secs(25 * 60);
        let mut limiter = ReconnectLimiter::new();

        loop {
            if cancel.is_cancelled() {
                return;
            }

            match self.sync_account(&account_config_id, &config).await {
                Err(error) => {
                    // Permanent errors (auth/config) go to the 60s slow mode;
                    // the loop stays alive so a fixed config self-heals.
                    let wait = retry_wait_for(&error, &mut limiter);
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(wait) => {},
                    }
                    continue;
                }
                Ok(_) => limiter.record_success(),
            }
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
                    let wait = limiter.record_failure();
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(wait) => {},
                    }
                }
            }
        }
    }
}

/// Bound for individual IMAP/SMTP operations.
pub const NETWORK_OP_TIMEOUT: Duration = Duration::from_secs(60);
/// Bound for waiting on a per-message lock; a stuck fetch must not wedge
/// later work forever.
pub const LOCK_WAIT_BOUND: Duration = Duration::from_secs(30);

/// Acquire an async lock with a bound; on timeout return a transient error.
pub(crate) async fn acquire_bounded(
    lock: Arc<tokio::sync::Mutex<()>>,
    bound: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>> {
    match tokio::time::timeout(bound, lock.lock_owned()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(Error::Backend(
            "timed out waiting for message lock".into(),
        )),
    }
}

/// Bound a network operation: on timeout return a transient error so the
/// retry policy takes over instead of hanging the sync loop.
pub async fn with_network_timeout<T>(
    op: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    with_network_timeout_in(NETWORK_OP_TIMEOUT, op).await
}

pub(crate) async fn with_network_timeout_in<T>(
    bound: Duration,
    op: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    match tokio::time::timeout(bound, op).await {
        Ok(result) => result,
        Err(_) => Err(Error::Backend("network operation timed out".into())),
    }
}

/// Should this failure be retried? IO and network-flavored backend errors
/// are transient; auth/config/data errors are permanent (fail fast).
pub fn is_transient(error: &Error) -> bool {
    match error {
        Error::Io(_) => true,
        Error::Backend(message) => {
            let lower = message.to_lowercase();
            ["timeout", "timed out", "connection", "reset", "bye", "watch error"]
                .iter()
                .any(|token| lower.contains(token))
                && !lower.contains("auth")
                && !lower.contains("invalid credentials")
        }
        _ => false,
    }
}

/// Exponential backoff with ±20% jitter derived from `jitter_seed` (a cheap
/// time-based value — not cryptographic); the result is clamped at `cap`.
pub fn backoff_delay(attempt: u32, base: Duration, cap: Duration, jitter_seed: u64) -> Duration {
    let expected = base
        .saturating_mul(2u32.saturating_pow(attempt.min(16)))
        .min(cap);
    let span = (expected.as_millis() / 5) as u64;
    if span == 0 {
        return expected;
    }
    let offset = (jitter_seed % (2 * span + 1)) as i64 - span as i64;
    Duration::from_millis((expected.as_millis() as i64 + offset).max(0) as u64).min(cap)
}

/// Caps reconnect churn: backoff while the streak is short, fixed 60s slow
/// mode from the 10th consecutive failure, reset on success.
pub struct ReconnectLimiter {
    consecutive: u32,
}

const RECONNECT_SLOW_MODE_AFTER: u32 = 10;
const RECONNECT_SLOW_MODE_WAIT: Duration = Duration::from_secs(60);

impl Default for ReconnectLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconnectLimiter {
    pub fn new() -> Self {
        Self { consecutive: 0 }
    }

    pub fn record_failure(&mut self) -> Duration {
        self.consecutive = self.consecutive.saturating_add(1);
        if self.consecutive >= RECONNECT_SLOW_MODE_AFTER {
            RECONNECT_SLOW_MODE_WAIT
        } else {
            backoff_delay(
                self.consecutive - 1,
                Duration::from_secs(2),
                Duration::from_secs(300),
                jitter_seed(self.consecutive),
            )
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive = 0;
    }

    pub fn consecutive(&self) -> u32 {
        self.consecutive
    }
}

fn jitter_seed(salt: u32) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(u64::from(salt))
}

/// Poison rule for outbox replay failures: permanent errors go terminal
/// immediately; transient errors retry until `MAX_OUTBOX_ATTEMPTS` then stop
/// (no infinite retry).
fn outbox_failure_is_terminal(error: &Error, attempts_after_this: u32) -> bool {
    !is_transient(error) || attempts_after_this >= MAX_OUTBOX_ATTEMPTS
}

const MAX_OUTBOX_ATTEMPTS: u32 = 5;

/// Wait before retrying a failed sync. Transient errors use the limiter's
/// growing backoff; permanent errors (auth/config) drop straight to the 60s
/// slow mode — the account loop never dies and never hot-retries.
fn retry_wait_for(error: &Error, limiter: &mut ReconnectLimiter) -> Duration {
    let wait = limiter.record_failure();
    if is_transient(error) {
        wait
    } else {
        RECONNECT_SLOW_MODE_WAIT
    }
}

/// Resolved plaintext secrets for this account (best-effort; unresolvable
/// secrets are skipped). Used to scrub error strings before persistence.
fn config_secret_values(config: &AccountConfig) -> Vec<String> {
    let mut values = Vec::new();
    for secret in [
        config.imap.as_ref().and_then(|imap| imap.secret.as_ref()),
        config.smtp.as_ref().and_then(|smtp| smtp.secret.as_ref()),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(value) = secret.resolve() {
            if !value.is_empty() {
                values.push(value);
            }
        }
    }
    values
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_transient() {
        let timed_out = Error::Backend("network operation timed out".into());
        assert!(is_transient(&timed_out));
    }

    #[tokio::test]
    async fn body_lock_wait_bounded() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let held = lock.clone().lock_owned().await;

        let result = acquire_bounded(lock.clone(), Duration::from_millis(50)).await;
        assert!(result.is_err(), "second acquisition must time out, not hang");
        assert!(is_transient(&result.unwrap_err()));

        drop(held);
        let acquired = acquire_bounded(lock, Duration::from_millis(50)).await;
        assert!(acquired.is_ok(), "free lock must be acquirable");
    }

    #[tokio::test]
    async fn with_network_timeout_bounds_ops() {
        let pending = std::future::pending::<Result<()>>();
        let result = with_network_timeout_in(Duration::from_millis(10), pending).await;
        assert!(result.is_err());
        assert!(is_transient(&result.unwrap_err()));
    }

    #[test]
    fn is_transient_classifies_errors() {
        // Transient: IO failures and network-flavored backend errors.
        assert!(is_transient(&Error::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out"
        ))));
        assert!(is_transient(&Error::Backend("connection reset".into())));
        assert!(is_transient(&Error::Backend("IMAP timeout waiting for response".into())));
        assert!(is_transient(&Error::Backend("watch error".into())));
        assert!(is_transient(&Error::Backend("server sent BYE".into())));

        // Permanent: auth/config/data errors must fail fast.
        assert!(!is_transient(&Error::Backend("auth failed".into())));
        assert!(!is_transient(&Error::Config("no IMAP".into())));
        assert!(!is_transient(&Error::AccountNotFound("x".into())));
        assert!(!is_transient(&Error::Store(rusqlite::Error::ExecuteReturnedResults)));
    }

    #[test]
    fn backoff_delay_grows_caps_and_jitters() {
        let base = Duration::from_secs(2);
        let cap = Duration::from_secs(300);

        // Deterministic jitter extremes stay within ±20%.
        for attempt in 0..10 {
            for seed in [0u64, u64::MAX / 2, u64::MAX] {
                let delay = backoff_delay(attempt, base, cap, seed);
                let expected = base
                    .saturating_mul(2u32.saturating_pow(attempt.min(16)))
                    .min(cap);
                let span = expected.as_millis() / 5;
                assert!(
                    delay.as_millis() + span >= expected.as_millis()
                        && delay.as_millis() <= expected.as_millis() + span,
                    "attempt {attempt} seed {seed}: {delay:?} outside ±20% of {expected:?}"
                );
            }
        }
        // Growth reaches the cap.
        assert!(backoff_delay(10, base, cap, 0) <= cap);
        assert!(backoff_delay(30, base, cap, 0) <= cap);
    }

    #[test]
    fn outbox_poison_rule() {
        let permanent = Error::Config("bad auth".into());
        let transient = Error::Backend("connection reset".into());
        // Permanent errors go terminal on the first failure.
        assert!(outbox_failure_is_terminal(&permanent, 1));
        // Transient errors retry until the poison bound.
        assert!(!outbox_failure_is_terminal(&transient, 1));
        assert!(!outbox_failure_is_terminal(&transient, 4));
        assert!(outbox_failure_is_terminal(&transient, 5));
    }

    #[test]
    fn retry_wait_policy() {
        let mut limiter = ReconnectLimiter::new();
        // Permanent errors drop to the 60s slow mode immediately — the loop
        // must never die and never hot-retry.
        let permanent = Error::Config("bad auth".into());
        assert_eq!(
            retry_wait_for(&permanent, &mut limiter),
            RECONNECT_SLOW_MODE_WAIT
        );
        // Transient errors use the limiter's growing backoff.
        let transient = Error::Backend("connection reset".into());
        let wait = retry_wait_for(&transient, &mut limiter);
        assert!(wait <= Duration::from_secs(300));
        assert_ne!(wait, Duration::ZERO);
    }

    #[test]
    fn reconnect_limiter_caps_streak() {
        let mut limiter = ReconnectLimiter::new();
        for _ in 0..9 {
            let wait = limiter.record_failure();
            assert!(wait <= Duration::from_secs(300), "under streak cap: {wait:?}");
        }
        // 10th consecutive failure drops into the 60s slow mode.
        let slow = limiter.record_failure();
        assert_eq!(slow, Duration::from_secs(60));
        assert_eq!(limiter.record_failure(), Duration::from_secs(60));

        // Success resets the streak.
        limiter.record_success();
        assert_eq!(limiter.consecutive(), 0);
    }
}
