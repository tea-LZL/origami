//! Tauri state: one SyncEngine + Store + reloadable Config shared by
//! all commands. Per-account sync handles and cancellation tokens live
//! here so the user can stop, remove, or reconnect individual accounts
//! without a restart.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use origami_core::config::{AccountConfig, AuthMechanism, Config, OAuthConfig, Secret};
use origami_core::imap::ImapBackend;
use origami_core::oauth::{refresh_token_blocking, OAuthProvider};
use origami_core::store::Store;
use origami_core::sync::SyncEngine;
use origami_core::{Error, Result};
use tokio_util::sync::CancellationToken;

/// Delay between account sync loops at startup so three TLS+AUTH+full-folder
/// passes do not collide with the first paint. The first account starts at once.
const SYNC_START_STAGGER: Duration = Duration::from_secs(2);

fn stagger_delay(index: usize) -> Duration {
    SYNC_START_STAGGER.saturating_mul(index as u32)
}

pub struct AppState {
    pub engine: Arc<SyncEngine>,
    pub store: Store,
    pub display_lru: Mutex<crate::display_lru::DisplayLru>,
    config: Arc<RwLock<Config>>,
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub(crate) account_errors: Arc<Mutex<HashMap<String, String>>>,
    pub(crate) syncing_accounts: Arc<Mutex<HashSet<String>>>,
    pub(crate) display_fetch_gates: DisplayFetchGates,
    prefetch_started: std::sync::Once,
}

/// One async gate per display key. An open click and a hover prefetch for the
/// same physical message share the gate, so only one IMAP fetch runs; the
/// loser waits, then finds the freshly cached body.
#[derive(Clone, Default)]
pub struct DisplayFetchGates {
    gates: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl DisplayFetchGates {
    pub fn acquire(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.gates
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Drop the map entry once no fetch holds or waits on it.
    pub fn release(&self, key: &str, gate: &Arc<tokio::sync::Mutex<()>>) {
        if Arc::strong_count(gate) <= 2 {
            self.gates.lock().unwrap().remove(key);
        }
    }
}

impl AppState {
    pub fn new() -> Result<Self> {
        let data_dir = data_dir();
        let store = Store::open(&data_dir.join("db.sqlite3"))?;
        let blobs = origami_core::blob::BlobStore::open(&data_dir.join("blobs"))?;
        let engine = Arc::new(SyncEngine::new(store.clone(), blobs));
        let config = origami_core::config::load()?;
        Ok(Self {
            engine,
            store,
            display_lru: Mutex::new(crate::display_lru::DisplayLru::new(32 * 1024 * 1024, 300)),
            config: Arc::new(RwLock::new(config)),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            account_errors: Arc::new(Mutex::new(HashMap::new())),
            syncing_accounts: Arc::new(Mutex::new(HashSet::new())),
            display_fetch_gates: DisplayFetchGates::default(),
            prefetch_started: std::sync::Once::new(),
        })
    }

    /// Read a snapshot of the current config.
    pub fn read_config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Atomically replace the config in memory after a write to disk.
    fn write_config(&self, config: Config) {
        *self.config.write().unwrap() = config;
    }

    /// Persist a config back to disk AND update the in-memory copy.
    pub fn save_config(&self, config: &Config) -> Result<()> {
        origami_core::config::save(config)?;
        self.write_config(config.clone());
        Ok(())
    }

    /// Start background sync loops for all IMAP accounts, staggered so the
    /// first screen paints while the later accounts are still connecting.
    pub fn spawn_sync_loops(&self) {
        let config = self.read_config();
        let mut index = 0;
        for (id, account) in &config.accounts {
            if account.imap.is_some() {
                self.start_account_sync_after(id.clone(), account.clone(), stagger_delay(index));
                index += 1;
            }
        }
    }

    /// Start a sync loop for one account, tracking its cancellation token.
    pub fn start_account_sync(&self, account_id: String, account: AccountConfig) {
        self.start_account_sync_after(account_id, account, Duration::ZERO);
    }

    fn start_account_sync_after(
        &self,
        account_id: String,
        account: AccountConfig,
        delay: Duration,
    ) {
        let token = CancellationToken::new();
        let engine = self.engine.clone();
        let token_clone = token.clone();
        let aid = account_id.clone();
        let sync_account = account.clone();
        let is_oauth = account.imap.as_ref().is_some_and(|imap| {
            matches!(
                imap.auth,
                AuthMechanism::Xoauth2 | AuthMechanism::Oauthbearer
            )
        });
        let oauth = is_oauth.then(|| self.read_config().oauth);
        let initial_oauth = oauth.clone();
        let initial_account_id = account_id.clone();
        let initial_account = account.clone();
        let initial_cancel = token.clone();
        let account_errors = self.account_errors.clone();
        let syncing_accounts = self.syncing_accounts.clone();
        self.syncing_accounts
            .lock()
            .unwrap()
            .insert(account_id.clone());

        tauri::async_runtime::spawn(async move {
            if !delay.is_zero() {
                tokio::select! {
                    _ = initial_cancel.cancelled() => return,
                    _ = tokio::time::sleep(delay) => {}
                }
            }
            if let Some(oauth) = initial_oauth {
                if let Err(error) = refresh_oauth_token(
                    &initial_account_id,
                    &initial_account,
                    &oauth,
                    &initial_cancel,
                )
                .await
                {
                    tracing::warn!(account = %initial_account_id, "OAuth refresh failed: {error}");
                    account_errors.lock().unwrap().insert(
                        initial_account_id.clone(),
                        format!("OAuth token expired: {error}"),
                    );
                    syncing_accounts.lock().unwrap().remove(&initial_account_id);
                    return;
                }
                account_errors.lock().unwrap().remove(&initial_account_id);
            }
            engine
                .run_account_loop(aid.clone(), sync_account, token_clone)
                .await;
            // The "syncing" marker is event-driven (AccountSyncStarted /
            // Error / AccountSynced) plus stop paths; removing it here would
            // race a restarted loop's fresh marker.
        });

        if let Some(oauth) = oauth {
            let refresh_account_id = account_id.clone();
            let refresh_account = account.clone();
            let refresh_cancel = token.clone();
            let account_errors = self.account_errors.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::select! {
                        _ = refresh_cancel.cancelled() => return,
                        _ = tokio::time::sleep(std::time::Duration::from_secs(45 * 60)) => {}
                    }
                    if let Err(error) = refresh_oauth_token(
                        &refresh_account_id,
                        &refresh_account,
                        &oauth,
                        &refresh_cancel,
                    )
                    .await
                    {
                        tracing::warn!(account = %refresh_account_id, "OAuth refresh failed: {error}");
                        account_errors.lock().unwrap().insert(
                            refresh_account_id.clone(),
                            format!("OAuth token expired: {error}"),
                        );
                    } else {
                        account_errors.lock().unwrap().remove(&refresh_account_id);
                    }
                }
            });
        }

        self.cancel_tokens.lock().unwrap().insert(account_id, token);
    }

    /// Stop the sync loop for an account gracefully.
    pub fn stop_account_sync(&self, account_id: &str) {
        if let Some(token) = self.cancel_tokens.lock().unwrap().remove(account_id) {
            token.cancel();
        }
        self.syncing_accounts.lock().unwrap().remove(account_id);
    }

    /// Cancel every account worker before the process exits.
    pub fn stop_all_sync(&self) {
        let tokens = std::mem::take(&mut *self.cancel_tokens.lock().unwrap());
        for token in tokens.into_values() {
            token.cancel();
        }
        self.syncing_accounts.lock().unwrap().clear();
    }

    /// Clear the error for an account.
    pub fn clear_account_error(&self, account_id: &str) {
        self.account_errors.lock().unwrap().remove(account_id);
    }

    /// Snapshot of all per-account errors.
    pub fn account_errors_snapshot(&self) -> HashMap<String, String> {
        self.account_errors.lock().unwrap().clone()
    }

    pub fn syncing_accounts_snapshot(&self) -> HashSet<String> {
        self.syncing_accounts.lock().unwrap().clone()
    }

    // -----------------------------------------------------------------
    // Backend helpers
    // -----------------------------------------------------------------

    /// Find an account by config id, or the default.
    pub fn account(&self, id: Option<&str>) -> Result<(String, AccountConfig)> {
        let config = self.read_config();
        let (k, v) = account_from_config(&config, id)?;
        Ok((k.to_string(), v.clone()))
    }

    /// Connect an IMAP backend for an account config id.
    pub async fn backend(&self, account_config_id: &str) -> Result<ImapBackend> {
        let config = self.read_config();
        let (_, account) = config
            .accounts
            .get_key_value(account_config_id)
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Error::AccountNotFound(account_config_id.to_string()))?;
        let imap = account
            .imap
            .as_ref()
            .ok_or_else(|| Error::Config(format!("account `{account_config_id}` has no IMAP")))?;
        origami_core::sync::with_network_timeout(ImapBackend::connect(account_config_id, imap))
            .await
    }

    /// Resolve a folder UUID to (account_config_id, account_db_id, mailbox name).
    pub fn resolve_folder(&self, folder_id: &str) -> Result<(String, String, String)> {
        let (account_db_id, name) = self
            .store
            .folder_account_and_name(folder_id)?
            .ok_or_else(|| Error::Backend(format!("unknown folder {folder_id}")))?;
        let account_config_id = self
            .store
            .account_config_id(&account_db_id)?
            .ok_or_else(|| Error::Backend(format!("unknown account {account_db_id}")))?;
        Ok((account_config_id, account_db_id, name))
    }

    /// Start the display-prefetch workers once. Must be called from inside
    /// the Tokio runtime (async command context); `Once` keeps it idempotent.
    pub fn ensure_prefetch_workers(&self) {
        let engine = self.engine.clone();
        let store = self.store.clone();
        let config = self.config.clone();
        let account_errors = self.account_errors.clone();
        let gates = self.display_fetch_gates.clone();
        self.prefetch_started.call_once(|| {
            let fetch_engine = engine.clone();
            engine.prefetch_queue().run(move |key: String| {
                prefetch_display_key(
                    fetch_engine.clone(),
                    store.clone(),
                    config.clone(),
                    account_errors.clone(),
                    gates.clone(),
                    key,
                )
            });
        });
    }
}

/// Fetch display MIME for one physical source and cache it. Never touches
/// flags. Writes after a `cancel_all` are dropped (generation protocol).
/// Accounts in an error state and benign misses are skipped without
/// counting toward the queue's failure streak.
async fn prefetch_display_key(
    engine: Arc<SyncEngine>,
    store: Store,
    config: Arc<RwLock<Config>>,
    account_errors: Arc<Mutex<HashMap<String, String>>>,
    gates: DisplayFetchGates,
    key: String,
) -> std::result::Result<(), String> {
    let Some((folder_id, uid)) = key.rsplit_once(':') else {
        // Malformed keys are a local mistake, not a fetch failure.
        return Ok(());
    };
    let Ok(server_uid) = uid.parse::<u32>() else {
        return Ok(());
    };
    let folder_id = folder_id.to_string();
    if store
        .parsed_message_for_logical_message(&folder_id, server_uid)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(());
    }
    let Some((account_db_id, mailbox)) = store
        .folder_account_and_name(&folder_id)
        .map_err(|error| error.to_string())?
    else {
        // Folder vanished since the UI enqueued the request.
        return Ok(());
    };
    let Some(account_config_id) = store
        .account_config_id(&account_db_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    if account_errors
        .lock()
        .unwrap()
        .contains_key(&account_config_id)
    {
        // Account in error state: prefetch waits for recovery.
        return Ok(());
    }
    let account = {
        let config = config.read().unwrap();
        config.accounts.get(&account_config_id).cloned()
    };
    let Some(account) = account else {
        // Account removed since the UI enqueued the request.
        return Ok(());
    };
    let Some(imap) = account.imap else {
        return Ok(());
    };

    // Share one fetch with an open click for the same physical message.
    let gate = gates.acquire(&key);
    let guard = gate.lock().await;
    let result = async {
        if store
            .parsed_message_for_logical_message(&folder_id, server_uid)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(());
        }
        let generation = engine.prefetch_queue().generation();
        let backend = origami_core::sync::with_network_timeout(ImapBackend::connect(
            &account_config_id,
            &imap,
        ))
        .await
        .map_err(|error| error.to_string())?;
        let display = origami_core::sync::with_network_timeout(
            backend.fetch_display_message(&mailbox, server_uid),
        )
        .await
        .map_err(|error| error.to_string())?;
        if engine.prefetch_queue().generation() != generation {
            return Ok(());
        }
        let envelope = store
            .get_envelope(&folder_id, server_uid)
            .map_err(|error| error.to_string())?;
        let Some(envelope) = envelope else {
            // Message deleted since the UI enqueued the request.
            return Ok(());
        };
        let fallback = format!("display:{folder_id}:{server_uid}");
        engine
            .cache_display_message_sources(&envelope.sources, &display, &fallback)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    .await;
    drop(guard);
    gates.release(&key, &gate);
    result
}

async fn refresh_oauth_token(
    account_id: &str,
    account: &AccountConfig,
    oauth: &OAuthConfig,
    cancel: &CancellationToken,
) -> Result<()> {
    let Some(provider) = OAuthProvider::from_domain(&account.email) else {
        return Ok(());
    };
    let refresh = Secret::Keyring {
        entry: format!("{account_id}-refresh"),
    }
    .resolve()?;
    if refresh.is_empty() {
        return Err(Error::Secret(format!(
            "missing OAuth refresh token for `{account_id}`"
        )));
    }
    let (client_id, client_secret) = match provider {
        OAuthProvider::Google => (
            oauth.google_client_id.as_deref(),
            oauth.google_client_secret.as_deref(),
        ),
        OAuthProvider::Microsoft => (
            oauth.microsoft_client_id.as_deref(),
            oauth.microsoft_client_secret.as_deref(),
        ),
    };
    let Some(client_id) = client_id else {
        return Ok(());
    };
    let tokens = refresh_token_blocking(provider, client_id, client_secret, &refresh).await?;
    if cancel.is_cancelled() {
        return Ok(());
    }
    origami_core::config::write_keyring_secret(account_id, &tokens.access_token)?;
    if let Some(new_refresh) = tokens.refresh_token {
        origami_core::config::write_keyring_secret(&format!("{account_id}-refresh"), &new_refresh)?;
    }
    Ok(())
}

/// Look up an account from a config snapshot (cheap, no locking needed
/// for callers that already hold a read lock).
pub fn account_from_config<'c>(
    config: &'c Config,
    id: Option<&str>,
) -> Result<(&'c str, &'c AccountConfig)> {
    match id {
        Some(id) => config
            .accounts
            .get_key_value(id)
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Error::AccountNotFound(id.to_string())),
        None => config
            .accounts
            .iter()
            .find(|(_, a)| a.default)
            .or_else(|| config.accounts.iter().next())
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Error::Config("no accounts configured".into())),
    }
}

/// XDG data dir for Origami (~/.local/share/origami).
pub fn data_dir() -> PathBuf {
    origami_core::config::data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use origami_core::model::MailboxRole;

    #[test]
    fn startup_sync_stagger_offsets_each_account() {
        assert_eq!(stagger_delay(0), std::time::Duration::ZERO);
        assert_eq!(stagger_delay(1), SYNC_START_STAGGER);
        assert_eq!(stagger_delay(2), SYNC_START_STAGGER * 2);
    }

    #[test]
    fn display_fetch_gates_share_one_gate_and_release_it() {
        let gates = DisplayFetchGates::default();
        let first = gates.acquire("folder:7");
        let second = gates.acquire("folder:7");
        assert!(Arc::ptr_eq(&first, &second));

        drop(second);
        gates.release("folder:7", &first);
        assert!(gates.gates.lock().unwrap().get("folder:7").is_none());
    }

    #[tokio::test]
    async fn benign_prefetch_misses_do_not_disable_queue() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let blobs = origami_core::blob::BlobStore::open(&dir.path().join("blobs")).unwrap();
        let engine = Arc::new(SyncEngine::new(store.clone(), blobs));
        let config = Arc::new(RwLock::new(Config::default()));

        let account = store
            .upsert_account("test", "Test", "t@example.org")
            .unwrap();
        let folder = store
            .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
            .unwrap();

        // Malformed key, unknown folder, and envelope-gone are benign skips.
        for key in [
            "nope".to_string(),
            "missing-folder:7".to_string(),
            format!("{folder}:99"),
        ] {
            let result = prefetch_display_key(
                engine.clone(),
                store.clone(),
                config.clone(),
                Arc::new(Mutex::new(HashMap::new())),
                DisplayFetchGates::default(),
                key.clone(),
            )
            .await;
            assert!(result.is_ok(), "benign miss must not be an error: {key}");
            // Mirror the worker's streak accounting for an Ok outcome.
            engine.prefetch_queue().note_success();
        }

        assert!(
            engine
                .prefetch_queue()
                .enqueue(origami_core::prefetch_queue::PrefetchRequest::new(
                    "k",
                    origami_core::prefetch_queue::PrefetchPriority::Viewport,
                )),
            "benign misses must not trip the failure kill switch"
        );
    }

    #[tokio::test]
    async fn prefetch_noop_when_account_error() {
        use origami_core::config::{ImapConfig, Secret};
        use origami_core::model::{Address, Envelope, Flag};

        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let blobs = origami_core::blob::BlobStore::open(&dir.path().join("blobs")).unwrap();
        let engine = Arc::new(SyncEngine::new(store.clone(), blobs));

        // Account points at a dead port so a real fetch attempt errors.
        let mut config = Config::default();
        config.accounts.insert(
            "test".to_string(),
            AccountConfig {
                name: "Test".into(),
                email: "t@example.org".into(),
                default: true,
                imap: Some(ImapConfig {
                    host: "127.0.0.1".into(),
                    port: Some(1),
                    tls: false,
                    starttls: false,
                    auth: AuthMechanism::Login,
                    username: "u".into(),
                    secret: Some(Secret::Raw { raw: "p".into() }),
                }),
                smtp: None,
                signature: None,
            },
        );
        let config = Arc::new(RwLock::new(config));

        let account = store
            .upsert_account("test", "Test", "t@example.org")
            .unwrap();
        let folder = store
            .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
            .unwrap();
        let envelope = Envelope {
            id: "msg-7".to_string(),
            mailbox_id: folder.clone(),
            subject: "warm".into(),
            from: vec![Address {
                name: None,
                addr: "a@example.org".into(),
            }],
            to: vec![],
            date: None,
            received_at: None,
            flags: vec![Flag::Seen],
            keywords: vec![],
            has_attachment: false,
            size: 10,
            server_uid: Some(7),
            message_id: Some("<warm@example.org>".into()),
            thread_id: Some("<warm@example.org>".into()),
            sources: Vec::new(),
        };
        store.upsert_envelope(&folder, &envelope).unwrap();

        let account_errors: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        account_errors
            .lock()
            .unwrap()
            .insert("test".to_string(), "IMAP down".to_string());

        for _ in 0..6 {
            let result = prefetch_display_key(
                engine.clone(),
                store.clone(),
                config.clone(),
                account_errors.clone(),
                DisplayFetchGates::default(),
                format!("{folder}:7"),
            )
            .await;
            assert!(result.is_ok(), "error-state account must be a benign skip");
        }
        assert!(
            engine
                .prefetch_queue()
                .enqueue(origami_core::prefetch_queue::PrefetchRequest::new(
                    "k",
                    origami_core::prefetch_queue::PrefetchPriority::Viewport,
                )),
            "account-error skips must not trip the failure kill switch"
        );
    }
}
