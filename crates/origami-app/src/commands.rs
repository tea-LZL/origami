//! Tauri commands: the UI ↔ core bridge. All commands read from the
//! local store (never blocking on IMAP); network actions go through
//! short-lived backend sessions while background sync keeps the store
//! fresh (see docs/PLAN.md §Architecture).

use origami_core::compose::{Draft, DraftAttachment};
use origami_core::config::{
    AccountConfig, ImapConfig, NotificationConfig, NotificationFolderScope, NotificationPreview,
    QuietHours, SmtpConfig,
};
use origami_core::message::{AttachmentMeta, MessageHeaders, MimePart, ParsedMessage};
use origami_core::model::{Envelope, Flag, Mailbox, MailboxRole, OutboxEntry, OutboxOp};
use origami_core::oauth::{OAuthProvider, OAuthTokens};
use origami_core::provider_hints;
use origami_core::smtp::OrigamiSmtp;
use origami_core::{MailBackend, SmtpSender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Manager, State};

use crate::display_lru::{display_key, parsed_with_cache};
use crate::oauth_flow;
use crate::state::{self, AppState};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    origami_core::redact::redact_secrets(&e.to_string())
}

fn classify_login_error(message: &str, app_password: bool) -> String {
    let lower = message.to_lowercase();
    let auth_failed = lower.contains("auth")
        || lower.contains("login")
        || lower.contains("invalid credentials")
        || lower.contains("authentication");
    if auth_failed && app_password {
        return "The server rejected this app password. For Gmail, enable 2-Step Verification and use a 16-character app password — your regular password will not work.".into();
    }
    if auth_failed {
        return "The server rejected this password.".into();
    }
    if lower.contains("timed out")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("connect")
    {
        return "Could not reach the mail server. Check the host, port, and your network.".into();
    }
    message.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    core_version: &'static str,
    shell_version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Origami",
        core_version: origami_core::version(),
        shell_version: env!("CARGO_PKG_VERSION"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    /// Config id (TOML key).
    pub id: String,
    /// App-owned database UUID.
    pub db_id: String,
    pub name: String,
    pub email: String,
    pub has_imap: bool,
    pub has_smtp: bool,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxDto {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub role: MailboxRole,
    pub total: u32,
    pub unread: u32,
    pub source_ids: Vec<String>,
}

fn collapse_folders_with_counts(
    folders: Vec<Mailbox>,
    sent_counts: &HashMap<String, (u32, u32)>,
) -> Vec<MailboxDto> {
    let mut logical = Vec::with_capacity(folders.len());
    for folder in folders {
        if folder.role != MailboxRole::Sent {
            logical.push(MailboxDto {
                id: folder.id.clone(),
                account_id: folder.account_id.clone(),
                name: folder.name,
                role: folder.role,
                total: folder.total,
                unread: folder.unread,
                source_ids: vec![folder.id],
            });
            continue;
        }

        if let Some(group) = logical.iter_mut().find(|group: &&mut MailboxDto| {
            group.account_id == folder.account_id && group.role == MailboxRole::Sent
        }) {
            if let Some((total, unread)) = sent_counts.get(&folder.account_id) {
                group.total = *total;
                group.unread = *unread;
            } else {
                group.total += folder.total;
                group.unread += folder.unread;
            }
            group.source_ids.push(folder.id.clone());
            if sent_folder_preference(&folder.name) < sent_folder_preference(&group.name) {
                group.id = folder.id;
                group.name = folder.name;
            }
        } else {
            let (total, unread) = sent_counts
                .get(&folder.account_id)
                .copied()
                .unwrap_or((folder.total, folder.unread));
            logical.push(MailboxDto {
                id: folder.id.clone(),
                account_id: folder.account_id,
                name: folder.name,
                role: MailboxRole::Sent,
                total,
                unread,
                source_ids: vec![folder.id],
            });
        }
    }

    for group in &mut logical {
        if group.role == MailboxRole::Sent {
            group.source_ids.sort();
            if let Some(index) = group.source_ids.iter().position(|id| id == &group.id) {
                group.source_ids.swap(0, index);
            }
        }
    }
    logical
}

fn collapse_folders_for_store(
    store: &origami_core::store::Store,
    folders: Vec<Mailbox>,
) -> CmdResult<Vec<MailboxDto>> {
    let mut source_ids_by_account: HashMap<String, Vec<String>> = HashMap::new();
    for folder in &folders {
        if folder.role == MailboxRole::Sent {
            source_ids_by_account
                .entry(folder.account_id.clone())
                .or_default()
                .push(folder.id.clone());
        }
    }

    let mut sent_counts = HashMap::new();
    for (account_id, source_ids) in source_ids_by_account {
        sent_counts.insert(
            account_id,
            store.logical_envelope_counts(&source_ids).map_err(err)?,
        );
    }
    Ok(collapse_folders_with_counts(folders, &sent_counts))
}

fn sent_folder_preference(name: &str) -> u8 {
    if name.eq_ignore_ascii_case("[gmail]/sent mail") {
        0
    } else if name.eq_ignore_ascii_case("sent") {
        1
    } else {
        2
    }
}

fn folder_sort_key(folder: &MailboxDto) -> (u8, String, String) {
    let role_order = match folder.role {
        MailboxRole::Inbox => 0,
        MailboxRole::Sent => 1,
        MailboxRole::Drafts => 2,
        MailboxRole::Archive => 3,
        MailboxRole::Junk => 4,
        MailboxRole::Trash => 5,
        MailboxRole::Other => 6,
    };
    (role_order, folder.name.clone(), folder.account_id.clone())
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<AccountDto>> {
    let config = state.read_config();
    let mut out = Vec::new();
    for (id, account) in &config.accounts {
        let db_id = state
            .store
            .upsert_account(id, &account.name, &account.email)
            .map_err(err)?;
        out.push(AccountDto {
            id: id.clone(),
            db_id,
            name: account.name.clone(),
            email: account.email.clone(),
            has_imap: account.imap.is_some(),
            has_smtp: account.smtp.is_some(),
            signature: account.signature.clone(),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn list_folders(
    state: State<'_, AppState>,
    account_db_id: Option<String>,
) -> CmdResult<Vec<MailboxDto>> {
    match account_db_id {
        Some(id) => {
            collapse_folders_for_store(&state.store, state.store.list_folders(&id).map_err(err)?)
        }
        None => {
            let config = state.read_config();
            let mut all = Vec::new();
            for (config_id, account) in &config.accounts {
                let db_id = state
                    .store
                    .upsert_account(config_id, &account.name, &account.email)
                    .map_err(err)?;
                all.extend(state.store.list_folders(&db_id).map_err(err)?);
            }
            let mut all = collapse_folders_for_store(&state.store, all)?;
            // INBOX first, then role order, then the rest by name.
            all.sort_by_key(folder_sort_key);
            Ok(all)
        }
    }
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    account_id: String,
    name: String,
) -> CmdResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("folder name is required".to_string());
    }
    let (_, account) = state.account(Some(&account_id)).map_err(err)?;
    let backend = state.backend(&account_id).await.map_err(err)?;
    backend.create_mailbox(name).await.map_err(err)?;
    let account_db_id = state
        .store
        .upsert_account(&account_id, &account.name, &account.email)
        .map_err(err)?;
    state
        .store
        .upsert_folder(&account_db_id, name, MailboxRole::Other)
        .map_err(err)?;
    Ok(())
}

/// Toggle a folder subscription: local choice persists immediately; the
/// IMAP SUBSCRIBE/UNSUBSCRIBE is best-effort (offline keeps the local state
/// and reports the error).
#[tauri::command]
pub async fn set_folder_subscribed(
    state: State<'_, AppState>,
    folder_id: String,
    subscribed: bool,
) -> CmdResult<()> {
    let (account_config_id, _account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    state
        .store
        .set_folder_subscribed(&folder_id, subscribed)
        .map_err(err)?;
    if let Ok(backend) = state.backend(&account_config_id).await {
        let result = if subscribed {
            backend.subscribe_mailbox(&mailbox).await
        } else {
            backend.unsubscribe_mailbox(&mailbox).await
        };
        result.map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn rename_folder(
    state: State<'_, AppState>,
    folder_id: String,
    name: String,
) -> CmdResult<()> {
    if state.store.folder_role(&folder_id).map_err(err)? != Some(MailboxRole::Other) {
        return Err("special folders cannot be renamed".to_string());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("folder name is required".to_string());
    }
    let (account_id, _, mailbox) = state.resolve_folder(&folder_id).map_err(err)?;
    let backend = state.backend(&account_id).await.map_err(err)?;
    backend.rename_mailbox(&mailbox, name).await.map_err(err)?;
    state.store.rename_folder(&folder_id, name).map_err(err)
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, folder_id: String) -> CmdResult<()> {
    if state.store.folder_role(&folder_id).map_err(err)? != Some(MailboxRole::Other) {
        return Err("special folders cannot be deleted".to_string());
    }
    let (account_id, _, mailbox) = state.resolve_folder(&folder_id).map_err(err)?;
    let backend = state.backend(&account_id).await.map_err(err)?;
    backend.delete_mailbox(&mailbox).await.map_err(err)?;
    state.store.delete_folder(&folder_id).map_err(err)
}

#[tauri::command]
pub fn list_envelopes(
    state: State<'_, AppState>,
    folder_id: String,
    page: u32,
    page_size: u32,
    unread_only: Option<bool>,
) -> CmdResult<Vec<Envelope>> {
    let source_ids = state.store.folder_source_ids(&folder_id).map_err(err)?;
    state
        .store
        .list_envelopes_in_folders(&source_ids, page, page_size, unread_only.unwrap_or(false))
        .map_err(err)
}

#[tauri::command]
pub fn list_unified_inbox(
    state: State<'_, AppState>,
    page: u32,
    page_size: u32,
    unread_only: Option<bool>,
) -> CmdResult<Vec<Envelope>> {
    state
        .store
        .list_unified_inbox(page, page_size, unread_only.unwrap_or(false))
        .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub envelope: Envelope,
    pub text: Option<String>,
    pub html: Option<String>,
    pub headers: MessageHeaders,
    pub attachments: Vec<AttachmentMeta>,
    pub parts: Vec<MimePart>,
    pub parse_warnings: Vec<String>,
}

fn message_dto(
    state: &AppState,
    folder_id: &str,
    server_uid: u32,
    parsed: ParsedMessage,
) -> CmdResult<MessageDto> {
    let envelope = state
        .store
        .get_envelope(folder_id, server_uid)
        .map_err(err)?
        .ok_or("envelope not found")?;
    Ok(MessageDto {
        envelope,
        text: parsed.text,
        html: parsed.html,
        headers: parsed.headers,
        attachments: parsed.attachments,
        parts: parsed.parts,
        parse_warnings: parsed.parse_warnings,
    })
}

/// Drop one physical source's entry from the display LRU (message gone or
/// moved locally).
fn invalidate_display_lru(state: &AppState, folder_id: &str, server_uid: u32) {
    state
        .display_lru
        .lock()
        .unwrap()
        .remove(&display_key(folder_id, server_uid));
}

/// Local cache only: never opens IMAP. None means the UI should show a
/// skeleton and call `get_message`.
#[tauri::command]
pub fn get_cached_message(
    state: State<'_, AppState>,
    folder_id: String,
    server_uid: u32,
) -> CmdResult<Option<MessageDto>> {
    let key = display_key(&folder_id, server_uid);
    let loaded = {
        let mut lru = state.display_lru.lock().unwrap();
        parsed_with_cache(&mut lru, &key, || {
            state
                .store
                .parsed_message_for_logical_message(&folder_id, server_uid)
        })
        .map_err(err)?
    };
    let Some(parsed) = loaded else {
        return Ok(None);
    };
    message_dto(&state, &folder_id, server_uid, parsed).map(Some)
}

/// Fetch normalized display data (or fall back to a full body) for display.
#[tauri::command]
pub async fn get_message(
    state: State<'_, AppState>,
    folder_id: String,
    server_uid: u32,
) -> CmdResult<MessageDto> {
    let key = display_key(&folder_id, server_uid);
    if let Some(parsed) = state.display_lru.lock().unwrap().get(&key) {
        return message_dto(&state, &folder_id, server_uid, parsed);
    }
    let (account_config_id, _account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    let parsed: ParsedMessage = match state
        .store
        .parsed_message_for_logical_message(&folder_id, server_uid)
        .map_err(err)?
    {
        Some(parsed) => parsed,
        None => {
            if let Some(hash) = state.store.blob_hash(&folder_id, server_uid).map_err(err)? {
                let raw = state.engine.blobs().get(&hash).map_err(err)?;
                state
                    .engine
                    .cache_parsed_message(&folder_id, server_uid, &hash, &raw)
                    .map_err(err)?
            } else {
                let backend = state.backend(&account_config_id).await.map_err(err)?;
                match origami_core::sync::with_network_timeout(
                    backend.fetch_display_message(&mailbox, server_uid),
                )
                .await
                {
                    Ok(display) => match state
                        .engine
                        .cache_display_message(&folder_id, server_uid, &display)
                    {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            let hash = state
                                .engine
                                .ensure_body(&backend, &folder_id, &mailbox, server_uid)
                                .await
                                .map_err(err)?;
                            let raw = state.engine.blobs().get(&hash).map_err(err)?;
                            state
                                .engine
                                .cache_parsed_message(&folder_id, server_uid, &hash, &raw)
                                .map_err(err)?
                        }
                    },
                    Err(_) => {
                        let hash = state
                            .engine
                            .ensure_body(&backend, &folder_id, &mailbox, server_uid)
                            .await
                            .map_err(err)?;
                        let raw = state.engine.blobs().get(&hash).map_err(err)?;
                        state
                            .engine
                            .cache_parsed_message(&folder_id, server_uid, &hash, &raw)
                            .map_err(err)?
                    }
                }
            }
        }
    };

    state
        .display_lru
        .lock()
        .unwrap()
        .insert(key, parsed.clone());
    message_dto(&state, &folder_id, server_uid, parsed)
}

struct LoadedAttachment {
    name: String,
    mime: String,
    bytes: Vec<u8>,
}

/// The message the reader is showing. That can live in the display LRU, or
/// only on another physical copy of the same mail, without a cache row for
/// this folder and UID.
fn opened_parsed_message(
    lru: &mut crate::display_lru::DisplayLru,
    store: &origami_core::store::Store,
    folder_id: &str,
    server_uid: u32,
) -> CmdResult<ParsedMessage> {
    let key = display_key(folder_id, server_uid);
    if let Some(parsed) = lru.get(&key) {
        return Ok(parsed);
    }
    store
        .parsed_message_for_logical_message(folder_id, server_uid)
        .map_err(err)?
        .ok_or_else(|| "message not opened yet".to_string())
}

async fn load_attachment(
    state: &AppState,
    folder_id: &str,
    server_uid: u32,
    index: usize,
) -> CmdResult<LoadedAttachment> {
    let (account_config_id, _account_db_id, mailbox) =
        state.resolve_folder(folder_id).map_err(err)?;
    let parsed = {
        let mut lru = state.display_lru.lock().unwrap();
        opened_parsed_message(&mut lru, &state.store, folder_id, server_uid)?
    };
    let attachment = parsed
        .attachments
        .iter()
        .find(|item| item.index == index)
        .ok_or("attachment not found")?;
    let part_path = attachment.part_path.clone();
    let attachment_index = attachment.index;
    let name = attachment
        .name
        .clone()
        .unwrap_or_else(|| format!("attachment-{attachment_index}"));
    let mime = attachment.mime.clone();
    let bytes = if let Some(hash) = state.store.blob_hash(folder_id, server_uid).map_err(err)? {
        let raw = state.engine.blobs().get(&hash).map_err(err)?;
        origami_core::message::attachment_bytes_at_path(&raw, &part_path)
            .or_else(|| origami_core::message::attachment_bytes(&raw, attachment_index))
            .ok_or("attachment not found")?
    } else {
        let backend = state.backend(&account_config_id).await.map_err(err)?;
        origami_core::sync::with_network_timeout(
            backend.fetch_attachment_section(&mailbox, server_uid, &part_path),
        )
        .await
        .map_err(err)?
    };
    Ok(LoadedAttachment { name, mime, bytes })
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "application/pdf" => Some("pdf"),
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "text/plain" => Some("txt"),
        "text/csv" => Some("csv"),
        "text/html" => Some("html"),
        "message/rfc822" => Some("eml"),
        "application/zip" => Some("zip"),
        "application/json" => Some("json"),
        _ => None,
    }
}

/// A single path segment. Directory pieces in the MIME filename are dropped
/// so an attachment named `../../etc/passwd` cannot leave the cache directory.
fn safe_attachment_file_name(raw_name: &str, mime: &str, index: usize) -> String {
    let base = Path::new(raw_name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|ch| !ch.is_control() && *ch != '/' && *ch != '\\')
        .take(120)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let mut name = if cleaned.is_empty() {
        format!("attachment-{index}")
    } else {
        cleaned.to_string()
    };
    let has_extension = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| !ext.is_empty());
    if !has_extension {
        if let Some(ext) = extension_for_mime(mime) {
            name = format!("{name}.{ext}");
        }
    }
    name
}

fn unique_child_dir(root: &Path) -> std::io::Result<PathBuf> {
    static NONCE: AtomicU64 = AtomicU64::new(1);
    for _ in 0..100 {
        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = root.join(format!("{nanos:x}-{nonce:x}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other(
        "could not create attachment directory",
    ))
}

fn write_attachment_file(
    root: &Path,
    raw_name: &str,
    mime: &str,
    index: usize,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let dir = unique_child_dir(&root)?;
    let name = safe_attachment_file_name(raw_name, mime, index);
    let path = dir.join(&name);
    if path.parent() != Some(dir.as_path()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "attachment name escaped cache directory",
        ));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(bytes)?;
    let written = path.canonicalize()?;
    if !written.starts_with(&root) {
        let _ = std::fs::remove_file(&path);
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "attachment path escaped cache directory",
        ));
    }
    Ok(written)
}

/// Decoded bytes of one attachment (base64 for IPC transport).
#[tauri::command]
pub async fn get_attachment(
    state: State<'_, AppState>,
    folder_id: String,
    server_uid: u32,
    index: usize,
) -> CmdResult<String> {
    use base64::Engine;
    let loaded = load_attachment(state.inner(), &folder_id, server_uid, index).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(loaded.bytes))
}

/// Write one attachment under the app cache and open it with the system handler.
#[tauri::command]
pub async fn open_attachment(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
    server_uid: u32,
    index: usize,
) -> CmdResult<()> {
    let loaded = load_attachment(state.inner(), &folder_id, server_uid, index).await?;
    let root = app.path().app_cache_dir().map_err(err)?.join("attachments");
    let path = write_attachment_file(&root, &loaded.name, &loaded.mime, index, &loaded.bytes)
        .map_err(err)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(err)
}

/// Replace a message's flags; immediate when online, queued otherwise.
#[tauri::command]
pub async fn store_flags(
    state: State<'_, AppState>,
    folder_id: String,
    server_uid: u32,
    flags: Vec<Flag>,
) -> CmdResult<()> {
    let (account_config_id, account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;

    if let Ok(backend) = state.backend(&account_config_id).await {
        if backend
            .store_flags(&mailbox, server_uid, &flags)
            .await
            .is_ok()
        {
            state
                .store
                .update_flags(&folder_id, server_uid, &flags)
                .map_err(err)?;
            return Ok(());
        }
    }

    state
        .engine
        .queue_store_flags(&account_db_id, &folder_id, &mailbox, server_uid, &flags)
        .map_err(err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagUpdateDto {
    server_uid: u32,
    flags: Vec<Flag>,
}

/// Apply a same-folder batch using one backend session. Retryable
/// failures are queued individually and reflected in SQLite immediately.
#[tauri::command]
pub async fn store_flags_batch(
    state: State<'_, AppState>,
    folder_id: String,
    updates: Vec<FlagUpdateDto>,
) -> CmdResult<()> {
    let (account_config_id, account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    let backend = state.backend(&account_config_id).await.ok();
    let mut local_updates: Vec<(u32, Vec<Flag>)> = Vec::new();

    for update in updates {
        let stored = if let Some(backend) = &backend {
            backend
                .store_flags(&mailbox, update.server_uid, &update.flags)
                .await
                .is_ok()
        } else {
            false
        };

        if stored {
            local_updates.push((update.server_uid, update.flags));
        } else {
            state
                .engine
                .queue_store_flags(
                    &account_db_id,
                    &folder_id,
                    &mailbox,
                    update.server_uid,
                    &update.flags,
                )
                .map_err(err)?;
        }
    }
    if !local_updates.is_empty() {
        state
            .store
            .update_flags_batch(&folder_id, &local_updates)
            .map_err(err)?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeywordUpdateDto {
    server_uid: u32,
    flags: Vec<Flag>,
    keywords: Vec<String>,
}

/// Replace message keywords while preserving the supplied system flags.
#[tauri::command]
pub async fn store_keywords_batch(
    state: State<'_, AppState>,
    folder_id: String,
    updates: Vec<KeywordUpdateDto>,
) -> CmdResult<()> {
    let (account_config_id, account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    let backend = state.backend(&account_config_id).await.ok();
    let mut local_flags: Vec<(u32, Vec<Flag>)> = Vec::new();
    let mut local_keywords: Vec<(u32, Vec<String>)> = Vec::new();

    for update in updates {
        let stored = if let Some(backend) = &backend {
            backend
                .store_flags_and_keywords(
                    &mailbox,
                    update.server_uid,
                    &update.flags,
                    Some(&update.keywords),
                )
                .await
                .is_ok()
        } else {
            false
        };
        if stored {
            local_flags.push((update.server_uid, update.flags));
            local_keywords.push((update.server_uid, update.keywords));
        } else {
            state
                .engine
                .queue_store_keywords(
                    &account_db_id,
                    &folder_id,
                    &mailbox,
                    update.server_uid,
                    &update.flags,
                    &update.keywords,
                )
                .map_err(err)?;
        }
    }
    if !local_flags.is_empty() {
        state
            .store
            .update_flags_batch(&folder_id, &local_flags)
            .map_err(err)?;
        state
            .store
            .update_keywords_batch(&folder_id, &local_keywords)
            .map_err(err)?;
    }
    Ok(())
}

/// Move a same-folder UID batch and remove the source rows optimistically.
/// Network failures are persisted in the outbox for replay.
#[tauri::command]
pub async fn move_messages(
    state: State<'_, AppState>,
    folder_id: String,
    destination_folder_id: String,
    server_uids: Vec<u32>,
) -> CmdResult<()> {
    let (account_config_id, account_db_id, source_mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    let (destination_account_id, _, destination_mailbox) =
        state.resolve_folder(&destination_folder_id).map_err(err)?;
    if destination_account_id != account_config_id {
        return Err("messages cannot be moved between accounts".to_string());
    }

    let moved = if let Ok(backend) = state.backend(&account_config_id).await {
        backend
            .move_messages(&source_mailbox, &destination_mailbox, &server_uids)
            .await
            .is_ok()
    } else {
        false
    };
    if !moved {
        state
            .engine
            .queue_move_messages(
                &account_db_id,
                &source_mailbox,
                &destination_mailbox,
                &server_uids,
            )
            .map_err(err)?;
    }
    state
        .store
        .delete_messages_by_uids(&folder_id, &server_uids)
        .map_err(err)?;
    for uid in server_uids {
        invalidate_display_lru(&state, &folder_id, uid);
    }
    Ok(())
}

/// Permanently delete and expunge a same-folder UID batch. Offline
/// requests are persisted before local rows are removed.
#[tauri::command]
pub async fn delete_messages(
    state: State<'_, AppState>,
    folder_id: String,
    server_uids: Vec<u32>,
) -> CmdResult<()> {
    let (account_config_id, account_db_id, mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    let deleted = if let Ok(backend) = state.backend(&account_config_id).await {
        backend
            .delete_messages(&mailbox, &server_uids)
            .await
            .is_ok()
    } else {
        false
    };
    if !deleted {
        state
            .engine
            .queue_delete_messages(&account_db_id, &mailbox, &server_uids)
            .map_err(err)?;
    }
    state
        .store
        .delete_messages_by_uids(&folder_id, &server_uids)
        .map_err(err)?;
    for uid in server_uids {
        invalidate_display_lru(&state, &folder_id, uid);
    }
    Ok(())
}

// ------------------------------------------------------------------
// Account management & onboarding
// ------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHintDto {
    pub description: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub auth: String,
    pub oauth_provider: Option<String>,
}

/// Resolve known defaults for an email address domain.
#[tauri::command]
pub fn provider_hints(email: String) -> ProviderHintDto {
    let domain = email.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
    let oauth = OAuthProvider::from_domain(&email);
    match provider_hints::for_domain(domain.to_lowercase().as_str()) {
        Some(p) => ProviderHintDto {
            description: Some(p.description.to_string()),
            imap_host: Some(p.imap.0.to_string()),
            imap_port: Some(p.imap.1),
            smtp_host: Some(p.smtp.0.to_string()),
            smtp_port: Some(p.smtp.1),
            auth: match oauth {
                Some(OAuthProvider::Microsoft) => "xoauth2".to_string(),
                _ => "login".to_string(),
            },
            oauth_provider: oauth.map(|p| {
                match p {
                    OAuthProvider::Google => "google",
                    OAuthProvider::Microsoft => "microsoft",
                }
                .to_string()
            }),
        },
        None => ProviderHintDto {
            description: None,
            imap_host: None,
            imap_port: None,
            smtp_host: None,
            smtp_port: None,
            auth: "login".to_string(),
            oauth_provider: oauth.map(|p| {
                match p {
                    OAuthProvider::Google => "google",
                    OAuthProvider::Microsoft => "microsoft",
                }
                .to_string()
            }),
        },
    }
}

/// Launch the OAuth PKCE flow and return tokens.
#[tauri::command]
pub async fn oauth_sign_in(
    app: tauri::AppHandle,
    provider: String,
    client_id: String,
    client_secret: Option<String>,
) -> CmdResult<OAuthTokens> {
    let p = match provider.as_str() {
        "google" => OAuthProvider::Google,
        "microsoft" => OAuthProvider::Microsoft,
        _ => return Err(format!("unknown oauth provider: {provider}")),
    };
    oauth_flow::run_oauth_flow(&app, p, &client_id, client_secret.as_deref())
        .await
        .map_err(err)
}

/// Look up the OAuth client credentials for a provider from the
/// `[oauth]` config section. The frontend calls this before starting a
/// sign-in so credentials never appear in UI code.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClientConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[tauri::command]
pub fn oauth_client_id(
    state: State<'_, AppState>,
    provider: String,
) -> CmdResult<OAuthClientConfig> {
    let oauth = &state.read_config().oauth;
    match provider.as_str() {
        "google" => Ok(OAuthClientConfig {
            client_id: oauth
                .google_client_id
                .clone()
                .ok_or_else(|| "no `oauth.google_client_id` in config".to_string())?,
            client_secret: oauth.google_client_secret.clone(),
        }),
        "microsoft" => Ok(OAuthClientConfig {
            client_id: oauth
                .microsoft_client_id
                .clone()
                .ok_or_else(|| "no `oauth.microsoft_client_id` in config".to_string())?,
            client_secret: oauth.microsoft_client_secret.clone(),
        }),
        _ => Err(format!("unknown oauth provider: {provider}")),
    }
}

/// Persist a new account: store secrets in the keyring, append to
/// config TOML, populate the store row immediately so the UI sees it,
/// and return an `AccountDto` for the caller to append to the list
/// (the in-memory `AppState.config` is stale until the next restart).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn add_account(
    state: State<'_, AppState>,
    account_id: String,
    name: String,
    email: String,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    auth: String,
    username: String,
    password: Option<String>,
    oauth_access_token: Option<String>,
    oauth_refresh_token: Option<String>,
) -> CmdResult<AccountDto> {
    let auth_mech = match auth.as_str() {
        "login" => origami_core::config::AuthMechanism::Login,
        "plain" => origami_core::config::AuthMechanism::Plain,
        "xoauth2" => origami_core::config::AuthMechanism::Xoauth2,
        "oauthbearer" => origami_core::config::AuthMechanism::Oauthbearer,
        _ => return Err(format!("unknown auth: {auth}")),
    };

    // Capture before moves for the DTO we'll return at the end.
    let (has_imap, has_smtp) = (imap_host.is_some(), smtp_host.is_some());
    let (dto_name, dto_email) = (name.clone(), email.clone());

    let imap = imap_host.map(|host| ImapConfig {
        host,
        port: imap_port,
        tls: true,
        starttls: false,
        auth: auth_mech,
        username: username.clone(),
        secret: None, // filled below
    });

    let smtp = smtp_host.map(|host| SmtpConfig {
        host,
        port: smtp_port,
        tls: true,
        starttls: false,
        auth: auth_mech,
        username: username.clone(),
        secret: None,
    });

    // Store the secret in the keyring and reference it in the config.
    let secret_value = if let Some(tok) = oauth_access_token {
        tok
    } else if let Some(pw) = password {
        pw
    } else {
        return Err("password or oauth token required".to_string());
    };

    let imap_probe = imap.as_ref().map(|config| ImapConfig {
        secret: Some(origami_core::config::Secret::Raw {
            raw: secret_value.clone(),
        }),
        ..config.clone()
    });
    if let Some(imap_probe) = imap_probe.as_ref() {
        if let Err(error) = origami_core::imap::ImapBackend::connect(&account_id, imap_probe).await
        {
            let app_password = matches!(
                auth_mech,
                origami_core::config::AuthMechanism::Login
                    | origami_core::config::AuthMechanism::Plain
            ) && email.to_lowercase().rsplit_once('@').is_some_and(
                |(_, domain)| matches!(domain, "gmail.com" | "googlemail.com" | "google.com"),
            );
            return Err(classify_login_error(&error.to_string(), app_password));
        }
    }

    origami_core::config::write_keyring_secret(&account_id, &secret_value).map_err(err)?;
    // Also store the refresh token for OAuth accounts.
    if let Some(refresh) = oauth_refresh_token {
        origami_core::config::write_keyring_secret(&format!("{account_id}-refresh"), &refresh)
            .map_err(err)?;
    }

    let imap_with_secret = imap.map(|mut i| {
        i.secret = Some(origami_core::config::Secret::Keyring {
            entry: account_id.clone(),
        });
        i
    });
    let smtp_with_secret = smtp.map(|mut s| {
        s.secret = Some(origami_core::config::Secret::Keyring {
            entry: account_id.clone(),
        });
        s
    });

    // Clone values for the DTO before they're moved into AccountConfig.
    let mut config = state.read_config();
    let is_default = config.accounts.is_empty();

    let account = AccountConfig {
        name,
        email,
        default: is_default,
        imap: imap_with_secret,
        smtp: smtp_with_secret,
        signature: None,
    };

    config.accounts.insert(account_id.clone(), account.clone());
    state.save_config(&config).map_err(err)?;

    // Insert the account row into the store so the UI's folder/envelope
    // queries work immediately (otherwise they wait for the sync task
    // to start, which races with the UI navigating to the new account).
    let db_id = state
        .store
        .upsert_account(
            &account_id,
            &config.accounts[&account_id].name,
            &config.accounts[&account_id].email,
        )
        .map_err(err)?;

    let signature = config
        .accounts
        .get(&account_id)
        .and_then(|account| account.signature.clone());
    let dto = AccountDto {
        id: account_id.clone(),
        db_id,
        name: dto_name,
        email: dto_email,
        has_imap,
        has_smtp,
        signature,
    };

    // Start the account's managed sync loop so initial import continues
    // in the background and subsequent new mail is watched.
    let account = config.accounts.get(&account_id).unwrap().clone();
    state.start_account_sync(account_id, account);

    Ok(dto)
}

/// Remove an account: stop its sync loop, delete its keyring secrets,
/// remove it from the config TOML, and return the remaining accounts
/// so the UI can refresh.
#[tauri::command]
pub async fn remove_account(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<Vec<AccountDto>> {
    // Stop the running sync loop first (clean shutdown).
    state.stop_account_sync(&account_id);

    // Everything this account cached in memory is now unreachable.
    state.display_lru.lock().unwrap().clear();
    // Global cancel is fine here: queued prefetch for other accounts is
    // cheap to re-request, and in-flight fetches discard stale writes.
    state.engine.prefetch_queue().cancel_all();

    // Wipe keyring secrets for this account.
    let _ = origami_core::config::delete_keyring_secret(&account_id);
    let _ = origami_core::config::delete_keyring_secret(&format!("{account_id}-refresh"));

    // Remove from the config TOML.
    let mut config = state.read_config();
    config.accounts.remove(&account_id);
    state.save_config(&config).map_err(err)?;

    // Clear per-account error and return the new list.
    state.clear_account_error(&account_id);

    let mut out = Vec::new();
    for (id, account) in &config.accounts {
        let db_id = state
            .store
            .upsert_account(id, &account.name, &account.email)
            .map_err(err)?;
        out.push(AccountDto {
            id: id.clone(),
            db_id,
            name: account.name.clone(),
            email: account.email.clone(),
            has_imap: account.imap.is_some(),
            has_smtp: account.smtp.is_some(),
            signature: account.signature.clone(),
        });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatusDto {
    state: &'static str,
    error: Option<String>,
    pending_operations: u32,
    failed_operations: u32,
}

#[tauri::command]
pub fn account_statuses(
    state: State<'_, AppState>,
) -> CmdResult<HashMap<String, AccountStatusDto>> {
    let config = state.read_config();
    let errors = state.account_errors_snapshot();
    let syncing = state.syncing_accounts_snapshot();
    let mut statuses = HashMap::new();
    for (id, account) in &config.accounts {
        let db_id = state
            .store
            .upsert_account(id, &account.name, &account.email)
            .map_err(err)?;
        let error = errors.get(id).cloned();
        let phase = if error.is_some() {
            "error"
        } else if syncing.contains(id) {
            "syncing"
        } else {
            "online"
        };
        statuses.insert(
            id.clone(),
            AccountStatusDto {
                state: phase,
                error,
                pending_operations: state.store.outbox_count(&db_id).map_err(err)?,
                failed_operations: state.store.outbox_failed_count(&db_id).map_err(err)?,
            },
        );
    }
    Ok(statuses)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxSummaryDto {
    id: i64,
    kind: &'static str,
    detail: String,
    created_at: i64,
    attempts: u32,
    last_error: Option<String>,
    failed_at: Option<i64>,
}

#[tauri::command]
pub fn list_outbox(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<Vec<OutboxSummaryDto>> {
    let (_, account) = state.account(Some(&account_id)).map_err(err)?;
    let account_db_id = state
        .store
        .upsert_account(&account_id, &account.name, &account.email)
        .map_err(err)?;
    let entries = state.store.outbox_list(&account_db_id).map_err(err)?;
    Ok(entries.into_iter().map(summarize_outbox_entry).collect())
}

fn summarize_outbox_entry(entry: OutboxEntry) -> OutboxSummaryDto {
    let (kind, detail) = match entry.op {
        OutboxOp::StoreFlags {
            mailbox,
            server_uid,
            ..
        } => (
            "Message update",
            format!("Update message {server_uid} in {mailbox}"),
        ),
        OutboxOp::MoveMessages {
            source_mailbox,
            destination_mailbox,
            server_uids,
        } => (
            "Move",
            format!(
                "Move {} {} from {source_mailbox} to {destination_mailbox}",
                server_uids.len(),
                if server_uids.len() == 1 {
                    "message"
                } else {
                    "messages"
                }
            ),
        ),
        OutboxOp::DeleteMessages {
            mailbox,
            server_uids,
        } => (
            "Delete",
            format!(
                "Delete {} {} from {mailbox}",
                server_uids.len(),
                if server_uids.len() == 1 {
                    "message"
                } else {
                    "messages"
                }
            ),
        ),
        OutboxOp::SendMessage { .. } => ("Send", "Send a queued message".into()),
        OutboxOp::AppendSent { .. } => ("Sent copy", "Save a copy in Sent".into()),
    };
    OutboxSummaryDto {
        id: entry.id,
        kind,
        detail,
        created_at: entry.created_at,
        attempts: entry.attempts,
        last_error: entry.last_error,
        failed_at: entry.failed_at,
    }
}

/// Reopen a terminal-failed outbox op for another attempt (user action).
#[tauri::command]
pub fn reopen_outbox_entry(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.store.outbox_reopen(id).map_err(err)
}

#[tauri::command]
pub async fn retry_outbox(state: State<'_, AppState>, account_id: String) -> CmdResult<()> {
    let (_, account) = state.account(Some(&account_id)).map_err(err)?;
    state
        .engine
        .sync_account(&account_id, &account)
        .await
        .map_err(err)?;
    state
        .engine
        .spawn_recent_prefetch(account_id.clone(), account, None);
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsDto {
    preview: String,
    folder_scope: String,
    quiet_hours: Option<QuietHoursDto>,
    grouped_per_account: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietHoursDto {
    start: String,
    end: String,
}

impl From<NotificationConfig> for NotificationSettingsDto {
    fn from(settings: NotificationConfig) -> Self {
        Self {
            preview: match settings.preview {
                NotificationPreview::Full => "full",
                NotificationPreview::SenderOnly => "sender_only",
                NotificationPreview::Hidden => "hidden",
            }
            .into(),
            folder_scope: match settings.folder_scope {
                NotificationFolderScope::All => "all",
                NotificationFolderScope::Inbox => "inbox",
            }
            .into(),
            quiet_hours: settings.quiet_hours.map(|hours| QuietHoursDto {
                start: hours.start,
                end: hours.end,
            }),
            grouped_per_account: settings.grouped_per_account,
        }
    }
}

#[tauri::command]
pub fn get_notification_settings(state: State<'_, AppState>) -> CmdResult<NotificationSettingsDto> {
    Ok(state.read_config().notifications.into())
}

#[tauri::command]
pub fn update_notification_settings(
    state: State<'_, AppState>,
    settings: NotificationSettingsDto,
) -> CmdResult<()> {
    let notifications = NotificationConfig {
        preview: match settings.preview.as_str() {
            "full" => NotificationPreview::Full,
            "sender_only" => NotificationPreview::SenderOnly,
            "hidden" => NotificationPreview::Hidden,
            value => return Err(format!("unknown notification preview: {value}")),
        },
        folder_scope: match settings.folder_scope.as_str() {
            "all" => NotificationFolderScope::All,
            "inbox" => NotificationFolderScope::Inbox,
            value => return Err(format!("unknown notification folder scope: {value}")),
        },
        quiet_hours: settings.quiet_hours.map(|hours| QuietHours {
            start: hours.start,
            end: hours.end,
        }),
        grouped_per_account: settings.grouped_per_account,
    };
    notifications.validate().map_err(err)?;
    let mut config = state.read_config();
    config.notifications = notifications;
    state.save_config(&config).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSettingsDto {
    name: String,
    email: String,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    username: Option<String>,
    auth: Option<String>,
    oauth_provider: Option<String>,
    signature: Option<String>,
}

#[tauri::command]
pub fn get_account_settings(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AccountSettingsDto> {
    let (_, account) = state.account(Some(&account_id)).map_err(err)?;
    let mechanism = account
        .imap
        .as_ref()
        .map(|imap| imap.auth)
        .or_else(|| account.smtp.as_ref().map(|smtp| smtp.auth));
    let auth = mechanism.map(|mechanism| {
        match mechanism {
            origami_core::config::AuthMechanism::Login => "login",
            origami_core::config::AuthMechanism::Plain => "plain",
            origami_core::config::AuthMechanism::Xoauth2 => "xoauth2",
            origami_core::config::AuthMechanism::Oauthbearer => "oauthbearer",
        }
        .to_string()
    });
    let oauth_provider = mechanism
        .filter(|mechanism| {
            matches!(
                mechanism,
                origami_core::config::AuthMechanism::Xoauth2
                    | origami_core::config::AuthMechanism::Oauthbearer
            )
        })
        .and_then(|_| OAuthProvider::from_domain(&account.email))
        .map(|provider| match provider {
            OAuthProvider::Google => "google".to_string(),
            OAuthProvider::Microsoft => "microsoft".to_string(),
        });
    Ok(AccountSettingsDto {
        name: account.name,
        email: account.email,
        imap_host: account.imap.as_ref().map(|imap| imap.host.clone()),
        imap_port: account.imap.as_ref().and_then(|imap| imap.port),
        smtp_host: account.smtp.as_ref().map(|smtp| smtp.host.clone()),
        smtp_port: account.smtp.as_ref().and_then(|smtp| smtp.port),
        username: account
            .imap
            .as_ref()
            .map(|imap| imap.username.clone())
            .or_else(|| account.smtp.as_ref().map(|smtp| smtp.username.clone())),
        auth,
        oauth_provider,
        signature: account.signature.clone(),
    })
}

/// Reconnect / update an existing account: stop the old sync loop,
/// replace the config entry, and start a fresh loop.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn update_account(
    state: State<'_, AppState>,
    account_id: String,
    name: Option<String>,
    email: Option<String>,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    auth: Option<String>,
    username: Option<String>,
    password: Option<String>,
    oauth_access_token: Option<String>,
    oauth_refresh_token: Option<String>,
    signature: Option<String>,
) -> CmdResult<()> {
    let mut config = state.read_config();
    let existing = config
        .accounts
        .get_mut(&account_id)
        .ok_or_else(|| format!("account `{account_id}` not found"))?;

    if let Some(n) = name {
        existing.name = n;
    }
    if let Some(e) = email {
        existing.email = e;
    }
    if let Some(sig) = signature {
        existing.signature = if sig.trim().is_empty() {
            None
        } else {
            Some(sig)
        };
    }
    if let Some(u) = username {
        if let Some(imap) = &mut existing.imap {
            imap.username = u.clone();
        }
        if let Some(smtp) = &mut existing.smtp {
            smtp.username = u;
        }
    }
    if let Some(a) = auth {
        let mech = match a.as_str() {
            "login" => origami_core::config::AuthMechanism::Login,
            "plain" => origami_core::config::AuthMechanism::Plain,
            "xoauth2" => origami_core::config::AuthMechanism::Xoauth2,
            "oauthbearer" => origami_core::config::AuthMechanism::Oauthbearer,
            _ => return Err(format!("unknown auth: {a}")),
        };
        if let Some(imap) = &mut existing.imap {
            imap.auth = mech;
        }
        if let Some(smtp) = &mut existing.smtp {
            smtp.auth = mech;
        }
    }
    if let Some(h) = imap_host {
        if let Some(imap) = &mut existing.imap {
            imap.host = h;
        }
    }
    if let Some(p) = imap_port {
        if let Some(imap) = &mut existing.imap {
            imap.port = Some(p);
        }
    }
    if let Some(h) = smtp_host {
        if let Some(smtp) = &mut existing.smtp {
            smtp.host = h;
        }
    }
    if let Some(p) = smtp_port {
        if let Some(smtp) = &mut existing.smtp {
            smtp.port = Some(p);
        }
    }

    match (oauth_access_token, oauth_refresh_token) {
        (Some(access), Some(refresh)) => {
            let is_oauth = existing.imap.as_ref().is_some_and(|imap| {
                matches!(
                    imap.auth,
                    origami_core::config::AuthMechanism::Xoauth2
                        | origami_core::config::AuthMechanism::Oauthbearer
                )
            });
            if !is_oauth {
                return Err("OAuth tokens can only update an OAuth account".into());
            }
            if access.trim().is_empty() || refresh.trim().is_empty() {
                return Err("OAuth reauthentication returned an empty token".into());
            }
            origami_core::config::write_keyring_secret(&format!("{account_id}-refresh"), &refresh)
                .map_err(err)?;
            origami_core::config::write_keyring_secret(&account_id, &access).map_err(err)?;
            if let Some(imap) = &mut existing.imap {
                imap.secret = Some(origami_core::config::Secret::Keyring {
                    entry: account_id.clone(),
                });
            }
            if let Some(smtp) = &mut existing.smtp {
                smtp.secret = Some(origami_core::config::Secret::Keyring {
                    entry: account_id.clone(),
                });
            }
        }
        (None, None) => {}
        _ => return Err("OAuth access and refresh tokens must be updated together".into()),
    }

    // Update the keyring secret if a new password is given.
    if let Some(pw) = password {
        origami_core::config::write_keyring_secret(&account_id, &pw).map_err(err)?;
    }

    state.save_config(&config).map_err(err)?;

    // Restart the sync loop with the new credentials.
    state.stop_account_sync(&account_id);
    state.clear_account_error(&account_id);
    let account = config.accounts.get(&account_id).unwrap().clone();
    state.start_account_sync(account_id, account);

    Ok(())
}

/// Trigger an immediate one-shot sync of an account (or all).
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>, account_id: Option<String>) -> CmdResult<()> {
    if let Some(id) = account_id {
        let (_, account) = state.account(Some(&id)).map_err(err)?;
        state
            .engine
            .sync_account(&id, &account)
            .await
            .map_err(err)?;
        state
            .engine
            .spawn_recent_prefetch(id.clone(), account, None);
    } else {
        let config = state.read_config();
        for (id, account) in &config.accounts {
            state.engine.sync_account(id, account).await.map_err(err)?;
            state
                .engine
                .spawn_recent_prefetch(id.clone(), account.clone(), None);
        }
    }
    Ok(())
}

/// One queued display prefetch request from the UI.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefetchRequestDto {
    pub folder_id: String,
    pub server_uid: u32,
    pub priority: String,
}

/// Enqueue display-MIME prefetch for predicted/viewport rows. Never touches
/// flags; cached sources are skipped. Fire-and-forget for the UI.
#[tauri::command]
pub async fn prefetch_display(
    state: State<'_, AppState>,
    requests: Vec<PrefetchRequestDto>,
) -> CmdResult<()> {
    state.ensure_prefetch_workers();
    let account_errors = state.account_errors_snapshot();
    for request in requests {
        let priority = match request.priority.as_str() {
            "open" => origami_core::prefetch_queue::PrefetchPriority::Open,
            "predictive" => origami_core::prefetch_queue::PrefetchPriority::Predictive,
            "viewport" => origami_core::prefetch_queue::PrefetchPriority::Viewport,
            other => return Err(format!("unknown prefetch priority: {other}")),
        };
        if state
            .store
            .parsed_message_for_logical_message(&request.folder_id, request.server_uid)
            .map_err(err)?
            .is_some()
        {
            continue;
        }
        // Accounts in an error state skip prefetch until they recover.
        if let Ok((account_config_id, _, _)) = state.resolve_folder(&request.folder_id) {
            if account_errors.contains_key(&account_config_id) {
                continue;
            }
        }
        let key = display_key(&request.folder_id, request.server_uid);
        state
            .engine
            .prefetch_queue()
            .enqueue(origami_core::prefetch_queue::PrefetchRequest::new(
                key, priority,
            ));
    }
    Ok(())
}

/// Warm display cache for the folder the user is looking at, then Inbox.
///
/// Must stay `async`: `spawn_recent_prefetch` calls `tokio::spawn`, and Tauri runs
/// synchronous commands on the main thread, where no Tokio runtime context exists.
#[tauri::command]
pub async fn prefetch_selected_folder(
    state: State<'_, AppState>,
    folder_id: String,
) -> CmdResult<()> {
    let (account_id, _, _) = state.resolve_folder(&folder_id).map_err(err)?;
    let (_, account) = state.account(Some(&account_id)).map_err(err)?;
    state
        .engine
        .spawn_recent_prefetch(account_id, account, Some(folder_id));
    Ok(())
}

#[tauri::command]
pub fn search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> CmdResult<Vec<Envelope>> {
    state.store.search(&query, limit.unwrap_or(50)).map_err(err)
}

/// Cross-folder conversation members for one thread id (account-scoped).
#[tauri::command]
pub fn thread_envelopes(
    state: State<'_, AppState>,
    folder_id: String,
    thread_id: String,
) -> CmdResult<Vec<Envelope>> {
    let (_account_config_id, account_db_id, _mailbox) =
        state.resolve_folder(&folder_id).map_err(err)?;
    state
        .store
        .thread_envelopes(&account_db_id, &thread_id)
        .map_err(err)
}

#[tauri::command]
pub fn search_count(state: State<'_, AppState>, query: String) -> CmdResult<u32> {
    state.store.search_count(&query).map_err(err)
}

#[tauri::command]
pub fn search_page(
    state: State<'_, AppState>,
    query: String,
    page: u32,
    page_size: u32,
) -> CmdResult<Vec<Envelope>> {
    state
        .store
        .search_page(&query, page, page_size)
        .map_err(err)
}

#[tauri::command]
pub fn list_saved_searches(
    state: State<'_, AppState>,
) -> CmdResult<Vec<origami_core::model::SavedSearch>> {
    state.store.list_saved_searches().map_err(err)
}

#[tauri::command]
pub fn save_search(
    state: State<'_, AppState>,
    name: String,
    query: String,
) -> CmdResult<origami_core::model::SavedSearch> {
    if name.trim().is_empty() || query.trim().is_empty() {
        return Err("saved search name and query are required".to_string());
    }
    state
        .store
        .save_search(name.trim(), query.trim())
        .map_err(err)
}

#[tauri::command]
pub fn delete_saved_search(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.store.delete_saved_search(&id).map_err(err)
}

#[tauri::command]
pub fn list_keywords(
    state: State<'_, AppState>,
) -> CmdResult<Vec<origami_core::model::KeywordCount>> {
    state.store.list_keywords().map_err(err)
}

#[tauri::command]
pub fn list_correspondents(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> CmdResult<Vec<origami_core::model::Correspondent>> {
    state
        .store
        .list_correspondents(limit.unwrap_or(200))
        .map_err(err)
}

#[tauri::command]
pub fn save_composer_draft(
    state: State<'_, AppState>,
    id: String,
    draft: serde_json::Value,
) -> CmdResult<()> {
    state
        .store
        .save_draft(&id, &serde_json::to_string(&draft).map_err(err)?)
        .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedComposerDraftDto {
    pub id: String,
    pub draft: serde_json::Value,
}

/// One unreadable row must not hide every other saved draft; skip it.
fn parse_saved_composer_drafts(rows: Vec<(String, String)>) -> Vec<SavedComposerDraftDto> {
    rows.into_iter()
        .filter_map(|(id, json)| {
            serde_json::from_str(&json)
                .ok()
                .map(|draft| SavedComposerDraftDto { id, draft })
        })
        .collect()
}

#[tauri::command]
pub fn list_composer_drafts(state: State<'_, AppState>) -> CmdResult<Vec<SavedComposerDraftDto>> {
    Ok(parse_saved_composer_drafts(state.store.list_drafts().map_err(err)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedComposerDraft {
    account_id: Option<String>,
    draft: PersistedDraftFields,
    #[serde(default)]
    attachments: Vec<PersistedAttachment>,
    #[serde(default)]
    threading: PersistedThreading,
}

#[derive(Deserialize)]
struct PersistedDraftFields {
    to: String,
    cc: String,
    bcc: String,
    subject: String,
    html: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedThreading {
    in_reply_to: Option<String>,
    references: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedAttachment {
    name: String,
    mime: String,
    data_base64: String,
}

fn draft_recipients(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(str::to_string)
        .collect()
}

#[tauri::command]
pub async fn sync_composer_draft(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let Some(json) = state.store.load_draft(&id).map_err(err)? else {
        return Ok(());
    };
    let saved: PersistedComposerDraft = serde_json::from_str(&json).map_err(err)?;
    if saved.draft.to.trim().is_empty()
        && saved.draft.cc.trim().is_empty()
        && saved.draft.bcc.trim().is_empty()
        && saved.draft.subject.trim().is_empty()
        && saved.draft.html.replace("<p></p>", "").trim().is_empty()
        && saved.attachments.is_empty()
    {
        return Ok(());
    }
    let (account_id, account) = state.account(saved.account_id.as_deref()).map_err(err)?;
    let raw = origami_core::compose::build_draft_message(&Draft {
        from_name: Some(account.name.clone()),
        from_addr: account.email.clone(),
        to: draft_recipients(&saved.draft.to),
        cc: draft_recipients(&saved.draft.cc),
        bcc: draft_recipients(&saved.draft.bcc),
        subject: saved.draft.subject,
        html: saved.draft.html,
        text: None,
        in_reply_to: saved.threading.in_reply_to,
        references: saved.threading.references,
        attachments: saved
            .attachments
            .into_iter()
            .map(|attachment| DraftAttachment {
                name: attachment.name,
                mime: attachment.mime,
                data_base64: attachment.data_base64,
            })
            .collect(),
    })
    .map_err(err)?;
    let backend = state.backend(&account_id).await.map_err(err)?;
    let mailboxes = backend.list_mailboxes().await.map_err(err)?;
    let drafts = mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Drafts)
        .ok_or("Drafts mailbox not found")?;
    let new_uid = backend
        .append_message(&drafts.name, &raw, &[Flag::Draft])
        .await
        .map_err(err)?
        .ok_or("Drafts server did not return APPENDUID")?;

    if let Some((old_account, old_mailbox, old_uid)) =
        state.store.draft_remote(&id).map_err(err)?
    {
        if let Ok(old_backend) = state.backend(&old_account).await {
            let _ = old_backend.delete_messages(&old_mailbox, &[old_uid]).await;
        }
    }
    state
        .store
        .set_draft_remote(&id, &account_id, &drafts.name, new_uid)
        .map_err(err)
}

#[tauri::command]
pub async fn delete_composer_draft(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if let Some((account_id, mailbox, uid)) = state.store.draft_remote(&id).map_err(err)? {
        if let Ok(backend) = state.backend(&account_id).await {
            let _ = backend.delete_messages(&mailbox, &[uid]).await;
        }
    }
    state.store.delete_draft(&id).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResultDto {
    queued: bool,
}

/// Build and send a draft; appends a copy to the Sent folder. Retryable
/// SMTP failures are persisted in the account outbox.
#[tauri::command]
pub async fn send_message(state: State<'_, AppState>, draft: Draft) -> CmdResult<SendResultDto> {
    let raw = origami_core::compose::build_message(&draft).map_err(err)?;

    let config = state.read_config();
    // Find the account owning the From address (fallback: default).
    let (account_id, account) = config
        .accounts
        .iter()
        .find(|(_, a)| a.email.eq_ignore_ascii_case(&draft.from_addr))
        .map(|(k, v)| (k.clone(), v.clone()))
        .or_else(|| {
            state::account_from_config(&config, None)
                .ok()
                .map(|(k, v)| (k.to_string(), v.clone()))
        })
        .ok_or("no account configured for sending")?;

    let smtp = account
        .smtp
        .as_ref()
        .ok_or_else(|| format!("account `{account_id}` has no SMTP configured"))?;
    let account_db_id = state
        .store
        .upsert_account(&account_id, &account.name, &account.email)
        .map_err(err)?;
    let sent = match OrigamiSmtp::connect(smtp).await {
        Ok(sender) => sender.send_message(&raw).await.is_ok(),
        Err(_) => false,
    };
    if !sent {
        state
            .engine
            .queue_send_message(&account_db_id, &raw)
            .map_err(err)?;
        return Ok(SendResultDto { queued: true });
    }

    // Delivery and Sent-copy retry are independent so a failed append
    // can never cause the SMTP message to be sent twice.
    let mut appended = false;
    if account.imap.is_some() {
        if let Ok(backend) = state.backend(&account_id).await {
            if let Ok(mailboxes) = backend.list_mailboxes().await {
                if let Some(sent) = mailboxes
                    .iter()
                    .find(|m| m.role == MailboxRole::Sent)
                    .or_else(|| mailboxes.iter().find(|m| m.name == "Sent"))
                {
                    appended = backend
                        .append_message(&sent.name, &raw, &[Flag::Seen])
                        .await
                        .is_ok();
                }
            }
        }
    }
    if !appended {
        state
            .engine
            .queue_append_sent(&account_db_id, &raw)
            .map_err(err)?;
    }
    Ok(SendResultDto { queued: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_app_password_failures_explain_the_required_setup() {
        assert!(
            classify_login_error("LOGIN failed: AUTHENTICATIONFAILED", true)
                .contains("16-character app password")
        );
        assert_eq!(
            classify_login_error("authentication failed", false),
            "The server rejected this password."
        );
        assert!(classify_login_error("connection timed out", false).contains("mail server"));
    }

    #[test]
    fn outbox_summary_does_not_expose_message_payload() {
        let payload = "U3ViamVjdDogc2VjcmV0";
        let summary = summarize_outbox_entry(OutboxEntry {
            id: 7,
            account_id: "account".into(),
            op: OutboxOp::SendMessage {
                raw_base64: payload.into(),
            },
            created_at: 1,
            attempts: 0,
            last_error: None,
            failed_at: None,
        });

        assert_eq!(summary.kind, "Send");
        assert!(!summary.detail.contains(payload));
    }

    #[test]
    fn attachment_file_stays_inside_cache_dir() {
        let root = tempfile::tempdir().unwrap();
        let path = write_attachment_file(
            root.path(),
            "../../etc/passwd",
            "application/pdf",
            0,
            b"hello",
        )
        .unwrap();
        let root_canon = root.path().canonicalize().unwrap();
        assert!(path.starts_with(&root_canon));
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        assert_eq!(path.file_name().unwrap(), "passwd.pdf");
    }

    #[test]
    fn attachment_file_replaces_dot_name() {
        let root = tempfile::tempdir().unwrap();
        let path =
            write_attachment_file(root.path(), "..", "application/octet-stream", 3, b"x").unwrap();
        assert_eq!(path.file_name().unwrap(), "attachment-3");
    }

    #[test]
    fn opened_message_uses_the_copy_the_reader_already_showed() {
        use origami_core::message::AttachmentMeta;
        use origami_core::model::{Address, Envelope, Flag, MailboxRole};
        use origami_core::store::Store;

        let store = Store::open_in_memory().unwrap();
        let account = store
            .upsert_account("test", "Test", "t@example.org")
            .unwrap();
        let inbox = store
            .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
            .unwrap();
        let archive = store
            .upsert_folder(&account, "Archive", MailboxRole::Archive)
            .unwrap();
        let mut shown = Envelope {
            id: "shown".into(),
            mailbox_id: String::new(),
            subject: "report".into(),
            from: vec![Address {
                name: None,
                addr: "alice@example.org".into(),
            }],
            to: vec![],
            date: None,
            received_at: None,
            flags: vec![Flag::Seen],
            keywords: vec![],
            has_attachment: true,
            size: 100,
            server_uid: Some(31),
            message_id: Some("same@example.org".into()),
            thread_id: Some("same@example.org".into()),
            sources: vec![],
        };
        let mut cached_copy = shown.clone();
        cached_copy.id = "copy".into();
        cached_copy.server_uid = Some(32);
        store.upsert_envelope(&inbox, &shown).unwrap();
        store.upsert_envelope(&archive, &cached_copy).unwrap();
        shown.server_uid = Some(31);
        store
            .set_parsed_message(
                &archive,
                32,
                &ParsedMessage {
                    attachments: vec![AttachmentMeta {
                        index: 0,
                        part_path: "2".into(),
                        name: Some("report.pdf".into()),
                        mime: "application/pdf".into(),
                        size: 5,
                        inline: false,
                        cid: None,
                    }],
                    ..ParsedMessage::default()
                },
            )
            .unwrap();

        assert!(store.parsed_message(&inbox, 31).unwrap().is_none());
        let mut lru = crate::display_lru::DisplayLru::new(1024, 8);
        let parsed = opened_parsed_message(&mut lru, &store, &inbox, 31).unwrap();
        assert_eq!(parsed.attachments[0].name.as_deref(), Some("report.pdf"));

        lru.insert(
            crate::display_lru::display_key(&inbox, 31),
            ParsedMessage {
                text: Some("from the open view".into()),
                ..ParsedMessage::default()
            },
        );
        let from_view = opened_parsed_message(&mut lru, &store, &inbox, 31).unwrap();
        assert_eq!(from_view.text.as_deref(), Some("from the open view"));
    }

    #[test]
    fn saved_drafts_skip_unparsable_rows() {
        let rows = vec![
            ("good".to_string(), "{\"a\":1}".to_string()),
            ("bad".to_string(), "{oops".to_string()),
        ];

        let parsed = parse_saved_composer_drafts(rows);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "good");
    }

    #[test]
    fn attachment_file_keeps_existing_extension() {
        let root = tempfile::tempdir().unwrap();
        let path = write_attachment_file(root.path(), "notes.txt", "text/plain", 1, b"hi").unwrap();
        assert_eq!(path.file_name().unwrap(), "notes.txt");
        assert_eq!(std::fs::read(&path).unwrap(), b"hi");
    }

    #[test]
    fn collapses_sent_mailboxes_per_account_and_sums_counts() {
        let folders = vec![
            Mailbox {
                id: "plain-sent".into(),
                account_id: "account-a".into(),
                name: "Sent".into(),
                role: MailboxRole::Sent,
                subscribed: true,
                total: 4,
                unread: 1,
            },
            Mailbox {
                id: "gmail-sent".into(),
                account_id: "account-a".into(),
                name: "[Gmail]/Sent Mail".into(),
                role: MailboxRole::Sent,
                subscribed: true,
                total: 68,
                unread: 2,
            },
            Mailbox {
                id: "inbox".into(),
                account_id: "account-a".into(),
                name: "INBOX".into(),
                role: MailboxRole::Inbox,
                subscribed: true,
                total: 10,
                unread: 3,
            },
            Mailbox {
                id: "other-account-sent".into(),
                account_id: "account-b".into(),
                name: "Sent".into(),
                role: MailboxRole::Sent,
                subscribed: true,
                total: 7,
                unread: 4,
            },
        ];

        let logical = collapse_folders_with_counts(folders, &HashMap::new());
        let sent = logical
            .iter()
            .find(|folder| folder.account_id == "account-a" && folder.role == MailboxRole::Sent)
            .unwrap();

        assert_eq!(logical.len(), 3);
        assert_eq!(sent.id, "gmail-sent");
        assert_eq!(sent.total, 72);
        assert_eq!(sent.unread, 3);
        assert_eq!(sent.source_ids, vec!["gmail-sent", "plain-sent"]);
        assert_eq!(
            logical
                .iter()
                .filter(|folder| folder.account_id == "account-b")
                .count(),
            1
        );
    }
}
