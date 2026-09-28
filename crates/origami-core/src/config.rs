//! Account configuration: TOML on disk, secrets by indirection.
//!
//! Config lives at `$XDG_CONFIG_HOME/origami/config.toml`
//! (usually `~/.config/origami/config.toml`). Secrets are never stored
//! in plaintext in the file: they resolve either from a shell command
//! (`pass show mail/work`) or, for development only, an inline raw value.
//!
//! Example:
//!
//! ```toml
//! [accounts.work]
//! name = "Work"
//! email = "me@example.com"
//! default = true
//!
//! [accounts.work.imap]
//! host = "imap.example.com"
//! auth = "login"               # login | plain | xoauth2 | oauthbearer
//! username = "me@example.com"
//!
//! [accounts.work.imap.secret]
//! command = "pass show mail/work"
//!
//! [accounts.work.smtp]
//! host = "smtp.example.com"
//! port = 465
//! auth = "login"
//! username = "me@example.com"
//!
//! [accounts.work.smtp.secret]
//! command = "pass show mail/work"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{private_fs, Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountConfig>,
    #[serde(default)]
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub preview: NotificationPreview,
    #[serde(default)]
    pub folder_scope: NotificationFolderScope,
    #[serde(default)]
    pub quiet_hours: Option<QuietHours>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            preview: NotificationPreview::Full,
            folder_scope: NotificationFolderScope::All,
            quiet_hours: None,
        }
    }
}

impl NotificationConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(hours) = &self.quiet_hours {
            let start = parse_time(&hours.start)?;
            let end = parse_time(&hours.end)?;
            if start == end {
                return Err(Error::Config(
                    "quiet hours start and end must differ".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPreview {
    #[default]
    Full,
    SenderOnly,
    Hidden,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationFolderScope {
    #[default]
    All,
    Inbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start: String,
    pub end: String,
}

/// OAuth 2.0 client credentials registered with each provider.
///
/// ```toml
/// [oauth]
/// google_client_id = "YOUR-ID.apps.googleusercontent.com"
/// microsoft_client_id = "00000000-0000-0000-0000-000000000000"
/// ```
///
/// Create these in the Google Cloud Console (Desktop application type)
/// and Azure Portal, with `http://127.0.0.1` in the redirect URIs.
///
/// The client `_secret` fields are optional. PKCE (Proof Key for Code
/// Exchange) verifies the client via the code challenge, so the secret
/// is not required for desktop flows. Include it only when a provider's
/// token endpoint demands it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub microsoft_client_id: Option<String>,
    pub microsoft_client_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub default: bool,
    pub imap: Option<ImapConfig>,
    pub smtp: Option<SmtpConfig>,
    /// Plain-text signature appended to new compositions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMechanism {
    Login,
    Plain,
    Xoauth2,
    Oauthbearer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    /// Defaults to 993 (TLS) or 143 (plain/STARTTLS).
    pub port: Option<u16>,
    /// Use implicit TLS (imaps). When false, plain TCP + optional STARTTLS.
    #[serde(default = "default_true")]
    pub tls: bool,
    /// STARTTLS upgrade (only meaningful with `tls = false`).
    #[serde(default)]
    pub starttls: bool,
    #[serde(default = "default_auth")]
    pub auth: AuthMechanism,
    pub username: String,
    #[serde(default)]
    pub secret: Option<Secret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    /// Defaults to 465 (TLS) or 25/587 (plain/STARTTLS).
    pub port: Option<u16>,
    /// Use implicit TLS (smtps). When false, plain TCP + optional STARTTLS.
    #[serde(default = "default_true")]
    pub tls: bool,
    #[serde(default)]
    pub starttls: bool,
    #[serde(default = "default_auth")]
    pub auth: AuthMechanism,
    pub username: String,
    #[serde(default)]
    pub secret: Option<Secret>,
}

/// A secret (password or OAuth token), resolved on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Secret {
    /// Inline plaintext — development only, discouraged for real accounts.
    Raw { raw: String },
    /// Shell command whose stdout is the secret (e.g. `pass show mail/work`).
    Command { command: String },
    /// Keychain entry name; stored under `{entry}` in the OS keyring
    /// (GNOME Keyring / KWallet via Secret Service on Linux).
    Keyring { entry: String },
}

impl Secret {
    pub fn resolve(&self) -> Result<String> {
        match self {
            Secret::Raw { raw } => Ok(raw.clone()),
            Secret::Command { command } => {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()?;
                if !output.status.success() {
                    return Err(Error::Secret(format!(
                        "secret command failed ({output}): `{command}`",
                        output = output.status
                    )));
                }
                let secret = String::from_utf8(output.stdout)
                    .map_err(|e| Error::Secret(format!("secret is not UTF-8: {e}")))?;
                Ok(secret.trim_end_matches(['\r', '\n']).to_string())
            }
            Secret::Keyring { entry } => {
                let from_keyring = keyring::Entry::new("origami", entry)
                    .ok()
                    .and_then(|keyring| keyring.get_password().ok())
                    .filter(|secret| !secret.is_empty());
                if let Some(secret) = from_keyring {
                    let _ = write_secret_file(entry, &secret);
                    return Ok(secret);
                }
                read_secret_file(entry)?
                    .ok_or_else(|| Error::Secret(format!("cannot read keyring `{entry}`")))
            }
        }
    }
}

/// XDG data dir for Origami (`$XDG_DATA_HOME/origami` or `~/.local/share/origami`).
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("origami");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/origami")
}

fn secrets_dir() -> PathBuf {
    data_dir().join("secrets")
}

fn secret_file_path(entry: &str) -> PathBuf {
    let safe: String = entry
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '\0' => '_',
            _ => ch,
        })
        .collect();
    let safe = match safe.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => safe,
    };
    secrets_dir().join(safe)
}

fn write_secret_file(entry: &str, value: &str) -> Result<()> {
    let dir = secrets_dir();
    private_fs::create_private_dir(&dir).map_err(|e| {
        Error::Secret(format!(
            "cannot create secrets directory {}: {e}",
            dir.display()
        ))
    })?;
    let path = secret_file_path(entry);
    private_fs::write_private(&path, value.as_bytes())
        .map_err(|e| Error::Secret(format!("cannot write {}: {e}", path.display())))
}

fn read_secret_file(entry: &str) -> Result<Option<String>> {
    let path = secret_file_path(entry);
    match std::fs::read_to_string(&path) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Secret(format!(
            "cannot read {}: {e}",
            path.display()
        ))),
    }
}

fn delete_secret_file(entry: &str) -> Result<()> {
    let path = secret_file_path(entry);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Secret(format!(
            "cannot delete {}: {e}",
            path.display()
        ))),
    }
}

/// Store a secret in the OS keyring under `origami/<entry>`, and mirror it
/// to a 0600 file so it survives a locked or session-only keyring.
pub fn write_keyring_secret(entry: &str, value: &str) -> Result<()> {
    write_secret_file(entry, value)?;
    match keyring::Entry::new("origami", entry).and_then(|keyring| keyring.set_password(value)) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::warn!("keyring write failed for `{entry}`, using private file: {e}");
            Ok(())
        }
    }
}

/// Remove a keyring secret and its private-file copy.
pub fn delete_keyring_secret(entry: &str) -> Result<()> {
    match keyring::Entry::new("origami", entry) {
        Ok(e) => {
            // keyring v3 does not expose delete; overwrite with empty.
            let _ = e.set_password("");
        }
        Err(e) => {
            if !e.to_string().contains("No such interface") {
                tracing::warn!("keyring delete failed for `{entry}`: {e}");
            }
        }
    }
    delete_secret_file(entry)
}

/// Save the current config to disk at the standard path, with
/// parent-directory creation.
pub fn save(config: &Config) -> Result<()> {
    config.notifications.validate()?;
    let path = config_path();
    if let Some(parent) = path.parent() {
        private_fs::create_private_dir(parent).map_err(|e| {
            Error::Config(format!(
                "cannot secure config directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    let raw = toml::to_string_pretty(config).map_err(|e| Error::Config(format!("toml: {e}")))?;
    private_fs::write_private(&path, raw.as_bytes())
        .map_err(|e| Error::Config(format!("cannot write {}: {e}", path.display())))
}

fn default_true() -> bool {
    true
}

fn default_auth() -> AuthMechanism {
    AuthMechanism::Login
}

/// Path to the configuration file.
pub fn config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("origami/config.toml");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/origami/config.toml")
}

/// Load the configuration file; missing file means "no accounts".
pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    if let Some(parent) = path.parent() {
        private_fs::create_private_dir(parent).map_err(|e| {
            Error::Config(format!(
                "cannot secure config directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    private_fs::secure_existing_file(&path)
        .map_err(|e| Error::Config(format!("cannot secure {}: {e}", path.display())))?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("cannot read {}: {e}", path.display())))?;
    let config: Config =
        toml::from_str(&raw).map_err(|e| Error::Config(format!("invalid config: {e}")))?;
    config.notifications.validate()?;
    Ok(config)
}

pub fn parse_time(value: &str) -> Result<u16> {
    let (hour, minute) = value
        .split_once(':')
        .ok_or_else(|| Error::Config(format!("invalid time `{value}`; expected HH:MM")))?;
    if hour.len() != 2 || minute.len() != 2 {
        return Err(Error::Config(format!(
            "invalid time `{value}`; expected HH:MM"
        )));
    }
    let hour: u16 = hour
        .parse()
        .map_err(|_| Error::Config(format!("invalid hour in `{value}`")))?;
    let minute: u16 = minute
        .parse()
        .map_err(|_| Error::Config(format!("invalid minute in `{value}`")))?;
    if hour > 23 || minute > 59 {
        return Err(Error::Config(format!("time `{value}` is out of range")));
    }
    Ok(hour * 60 + minute)
}

impl Config {
    /// Resolve an account by id, falling back to the default account.
    pub fn account(&self, id: Option<&str>) -> Result<(&str, &AccountConfig)> {
        match id {
            Some(id) => self
                .accounts
                .get_key_value(id)
                .map(|(k, v)| (k.as_str(), v))
                .ok_or_else(|| Error::AccountNotFound(id.to_string())),
            None => self
                .accounts
                .iter()
                .find(|(_, a)| a.default)
                .or_else(|| self.accounts.iter().next())
                .map(|(k, v)| (k.as_str(), v))
                .ok_or_else(|| Error::Config("no accounts configured".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_uses_compatible_notification_defaults() {
        let config: Config = toml::from_str("[oauth]\n").unwrap();
        assert_eq!(config.notifications, NotificationConfig::default());
    }

    #[test]
    fn notification_settings_roundtrip() {
        let config = Config {
            notifications: NotificationConfig {
                preview: NotificationPreview::Hidden,
                folder_scope: NotificationFolderScope::Inbox,
                quiet_hours: Some(QuietHours {
                    start: "22:30".into(),
                    end: "07:15".into(),
                }),
            },
            ..Config::default()
        };
        let raw = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&raw).unwrap();
        assert_eq!(decoded.notifications, config.notifications);
        assert!(decoded.notifications.validate().is_ok());
    }

    #[test]
    fn quiet_hours_reject_invalid_ranges() {
        for (start, end) in [
            ("9:00", "17:00"),
            ("24:00", "07:00"),
            ("09:60", "17:00"),
            ("09:00", "09:00"),
        ] {
            let settings = NotificationConfig {
                quiet_hours: Some(QuietHours {
                    start: start.into(),
                    end: end.into(),
                }),
                ..NotificationConfig::default()
            };
            assert!(settings.validate().is_err(), "accepted {start}-{end}");
        }
    }

    fn with_temp_data_home<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let old = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", temp.path());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(temp.path())));
        match old {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    fn secret_file(data_home: &std::path::Path, entry: &str) -> std::path::PathBuf {
        data_home.join("origami/secrets").join(entry)
    }

    #[test]
    fn keyring_secret_is_also_written_to_private_file() {
        with_temp_data_home(|data_home| {
            let entry = format!("origami-test-write-{}", uuid::Uuid::now_v7());
            write_keyring_secret(&entry, "s3cret").unwrap();
            let path = secret_file(data_home, &entry);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "s3cret");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            delete_keyring_secret(&entry).unwrap();
        });
    }

    #[test]
    fn keyring_secret_falls_back_to_private_file_when_keyring_is_empty() {
        with_temp_data_home(|data_home| {
            let entry = format!("origami-test-fallback-{}", uuid::Uuid::now_v7());
            write_secret_file(&entry, "from-file").unwrap();
            if let Ok(keyring) = keyring::Entry::new("origami", &entry) {
                let _ = keyring.set_password("");
            }
            let got = Secret::Keyring {
                entry: entry.clone(),
            }
            .resolve()
            .unwrap();
            assert_eq!(got, "from-file");
            assert_eq!(
                std::fs::read_to_string(secret_file(data_home, &entry)).unwrap(),
                "from-file"
            );
            delete_keyring_secret(&entry).unwrap();
        });
    }

    #[test]
    fn delete_keyring_secret_removes_private_file() {
        with_temp_data_home(|data_home| {
            let entry = format!("origami-test-delete-{}", uuid::Uuid::now_v7());
            write_keyring_secret(&entry, "gone").unwrap();
            let path = secret_file(data_home, &entry);
            assert!(path.exists());
            delete_keyring_secret(&entry).unwrap();
            assert!(!path.exists());
            assert!(Secret::Keyring { entry }.resolve().is_err());
        });
    }
}
