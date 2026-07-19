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

use crate::{Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub default: bool,
    pub imap: Option<ImapConfig>,
    pub smtp: Option<SmtpConfig>,
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
        }
    }
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
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("cannot read {}: {e}", path.display())))?;
    toml::from_str(&raw).map_err(|e| Error::Config(format!("invalid config: {e}")))
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
