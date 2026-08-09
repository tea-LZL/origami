//! OAuth 2.0 PKCE flow coordinator (Tauri-specific).

use origami_core::oauth::{exchange_token_blocking, OAuthProvider, OAuthTokens, PkceChallenge};
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Full PKCE flow: bind a local-port redirect listener, open the
/// provider consent page in a Tauri webview, capture the code, exchange
/// for tokens. Call from an async Tauri command context.
pub async fn run_oauth_flow(
    app: &tauri::AppHandle,
    provider: OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens, String> {
    let pkce = PkceChallenge::generate();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("listener bind: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("addr: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let auth_url = provider.auth_url(client_id, &redirect_uri, &pkce.challenge);

    let window = WebviewWindowBuilder::new(
        app,
        "oauth-signin",
        WebviewUrl::External(url::Url::parse(&auth_url).map_err(|e| format!("bad auth URL: {e}"))?),
    )
    .title("Sign in")
    .inner_size(520.0, 680.0)
    .center()
    .build()
    .map_err(|e| format!("webview: {e}"))?;

    // Accept exactly one HTTP connection (the browser redirect).
    let code = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|e| format!("accept: {e}"))?;
        let mut buf = [0u8; 4096];
        let n = socket
            .read(&mut buf)
            .await
            .map_err(|e| format!("read: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse `?code=...` from the GET path.
        let first_line = request.lines().next().ok_or("empty request".to_string())?;
        if first_line.contains("error=") {
            return Err(format!(
                "provider error: {}",
                query_param(first_line, "error").unwrap_or_default()
            ));
        }
        let code = query_param(first_line, "code")
            .ok_or_else(|| format!("no code in redirect: {first_line}"))?;

        // Send a minimal "you may close this tab" page.
        let _ = socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                  <html><body><h1>Origami</h1><p>You may close this tab.</p></body></html>",
            )
            .await;

        Ok(code)
    })
    .await
    .map_err(|_| "oauth flow timed out".to_string())??;

    // Close the consent window.
    let _ = window.close();

    exchange_token_blocking(
        provider,
        client_id,
        client_secret,
        &code,
        &pkce.verifier,
        &redirect_uri,
    )
    .await
    .map_err(|e| format!("token exchange: {e}"))
}

/// Extract a query-parameter value from an HTTP request line like
/// `GET /?param=val&other=x HTTP/1.1`.
fn query_param(first_line: &str, key: &str) -> Option<String> {
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return urlencoding::decode(v).ok().map(|c| c.into_owned());
            }
        }
    }
    None
}
