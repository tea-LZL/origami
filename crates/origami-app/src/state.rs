//! Tauri state: one SyncEngine + Store + reloadable Config shared by
//! all commands. Per-account sync handles and cancellation tokens live
//! here so the user can stop, remove, or reconnect individual accounts
//! without a restart.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use origami_core::config::{AccountConfig, AuthMechanism, Config, OAuthConfig, Secret};
use origami_core::imap::ImapBackend;
use origami_core::oauth::{refresh_token_blocking, OAuthProvider};
use origami_core::store::Store;
use origami_core::sync::SyncEngine;
use origami_core::{Error, Result};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub engine: Arc<SyncEngine>,
    pub store: Store,
    config: Arc<RwLock<Config>>,
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub(crate) account_errors: Arc<Mutex<HashMap<String, String>>>,
    pub(crate) syncing_accounts: Arc<Mutex<HashSet<String>>>,
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
            config: Arc::new(RwLock::new(config)),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            account_errors: Arc::new(Mutex::new(HashMap::new())),
            syncing_accounts: Arc::new(Mutex::new(HashSet::new())),
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

    /// Start background sync loops for all IMAP accounts.
    pub fn spawn_sync_loops(&self) {
        let config = self.read_config();
        for (id, account) in &config.accounts {
            if account.imap.is_some() {
                self.start_account_sync(id.clone(), account.clone());
            }
        }
    }

    /// Start a sync loop for one account, tracking its cancellation token.
    pub fn start_account_sync(&self, account_id: String, account: AccountConfig) {
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
        self.syncing_accounts
            .lock()
            .unwrap()
            .insert(account_id.clone());

        tauri::async_runtime::spawn(async move {
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
                }
            }
            engine
                .run_account_loop(aid, sync_account, token_clone)
                .await;
        });

        if let Some(oauth) = oauth {
            let refresh_account_id = account_id.clone();
            let refresh_account = account.clone();
            let refresh_cancel = token.clone();
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
        ImapBackend::connect(account_config_id, imap).await
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
        return Ok(());
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
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("origami");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/origami")
}
