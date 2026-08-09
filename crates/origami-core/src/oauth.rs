//! OAuth 2.0 PKCE flow for IMAP/SMTP (XOAUTH2, OAUTHBEARER).
//!
//! The token exchange lives in core (protocol-level); the webview flow
//! and local redirect listener live in `origami-app` (Tauri-specific).

use std::collections::HashMap;

use crate::{Error, Result};

/// The code verifier and its S256 challenge, generated per flow.
pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

impl PkceChallenge {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 64];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        let verifier = base64_url_no_pad(&bytes);
        let challenge = sha256_url_no_pad(verifier.as_bytes());
        Self {
            verifier,
            challenge,
        }
    }
}

/// Known OAuth 2.0 providers; the UI uses this table to build the
/// right `auth_url` / `token_url` and scope set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    pub fn from_domain(email: &str) -> Option<Self> {
        let domain = email.rsplit_once('@')?.1.to_ascii_lowercase();
        match domain.as_str() {
            "gmail.com" | "googlemail.com" | "google.com" => Some(OAuthProvider::Google),
            "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "microsoft.com"
            | "office365.com" => Some(OAuthProvider::Microsoft),
            _ => None,
        }
    }

    pub fn auth_url(&self, client_id: &str, redirect_uri: &str, challenge: &str) -> String {
        match self {
            OAuthProvider::Google => format!(
                "https://accounts.google.com/o/oauth2/v2/auth\
                 ?client_id={client_id}\
                 &redirect_uri={redirect_uri}\
                 &response_type=code\
                 &scope=https://mail.google.com/\
                 &code_challenge={challenge}\
                 &code_challenge_method=S256\
                 &access_type=offline\
                 &prompt=consent"
            ),
            OAuthProvider::Microsoft => format!(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize\
                 ?client_id={client_id}\
                 &redirect_uri={redirect_uri}\
                 &response_type=code\
                 &scope=offline_access\
                    %20https://outlook.office.com/IMAP.AccessAsUser.All\
                    %20https://outlook.office.com/SMTP.Send\
                 &code_challenge={challenge}\
                 &code_challenge_method=S256"
            ),
        }
    }

    pub fn token_url(&self) -> &str {
        match self {
            OAuthProvider::Google => "https://oauth2.googleapis.com/token",
            OAuthProvider::Microsoft => {
                "https://login.microsoftonline.com/common/oauth2/v2.0/token"
            }
        }
    }
}

/// Token endpoint response.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: String,
}

/// Exchange an `authorization_code` + `code_verifier` for tokens via
/// the provider's token endpoint. Synchronous (call from a blocking
/// worker) — reqwest is used via `tokio::Runtime::block_on` if needed,
/// or the caller brings a runtime. For simplicity this uses a
/// `ureq`-style blocking call, but reqwest is async-first. Let the
/// app layer handle async with `reqwest::blocking`.
///
/// (For the Tauri app this will be called from a blocking context.)
pub async fn exchange_token_blocking(
    provider: OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens> {
    let mut params = HashMap::new();
    params.insert("client_id", client_id);
    if let Some(secret) = client_secret {
        params.insert("client_secret", secret);
    }
    params.insert("code", code);
    params.insert("code_verifier", verifier);
    params.insert("redirect_uri", redirect_uri);
    params.insert("grant_type", "authorization_code");

    let client = reqwest::Client::new();
    let resp = client
        .post(provider.token_url())
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::Backend(format!("token request: {e}")))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Backend(format!("read response: {e}")))?;
    let tokens: OAuthTokens = serde_json::from_str(&body).map_err(|_| {
        Error::Backend(format!(
            "token exchange failed (HTTP {status}): {body}",
            body = body.chars().take(500).collect::<String>()
        ))
    })?;
    Ok(tokens)
}

/// Refresh an expired access token using its `refresh_token`.
pub async fn refresh_token_blocking(
    provider: OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<OAuthTokens> {
    let mut params = HashMap::new();
    params.insert("client_id", client_id);
    if let Some(secret) = client_secret {
        params.insert("client_secret", secret);
    }
    params.insert("refresh_token", refresh_token);
    params.insert("grant_type", "refresh_token");

    let client = reqwest::Client::new();
    let resp = client
        .post(provider.token_url())
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::Backend(format!("refresh request: {e}")))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Backend(format!("read response: {e}")))?;
    let tokens: OAuthTokens = serde_json::from_str(&body).map_err(|_| {
        Error::Backend(format!(
            "token refresh failed (HTTP {status}): {body}",
            body = body.chars().take(500).collect::<String>()
        ))
    })?;
    Ok(tokens)
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn sha256_url_no_pad(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    base64_url_no_pad(&digest)
}
