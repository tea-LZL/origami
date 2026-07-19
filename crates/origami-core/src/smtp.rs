//! SMTP sender over pimalaya's `io-smtp` (see ADR-0002).
//!
//! `send_message` derives the RFC 5321 envelope from the message headers
//! (From: → MAIL FROM; To:/Cc:/Bcc: → RCPT TO), the same approach as
//! himalaya's `src/smtp/backend.rs`.

use std::borrow::Cow;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use io_smtp::client::SmtpClientStd;
use io_smtp::rfc5321::{
    SmtpDomain, SmtpEhloDomain, SmtpForwardPath, SmtpLocalPart, SmtpMailbox, SmtpReversePath,
};
use mail_parser::MessageParser;
use pimalaya_stream::sasl::{Sasl, SaslLogin, SaslOauthbearer, SaslPlain, SaslXoauth2};
use secrecy::SecretString;
use url::Url;

use crate::backend::SmtpSender;
use crate::config::{AuthMechanism, SmtpConfig};
use crate::{Error, Result};

/// SMTP implementation of [`SmtpSender`]. Cheap to clone (shared session).
#[derive(Clone)]
pub struct OrigamiSmtp {
    client: Arc<Mutex<SmtpClientStd>>,
}

impl OrigamiSmtp {
    /// Connect (TCP/TLS/STARTTLS + greeting + EHLO + SASL).
    pub async fn connect(config: &SmtpConfig) -> Result<Self> {
        let config = config.clone();
        let client = tokio::task::spawn_blocking(move || connect_blocking(&config))
            .await
            .map_err(|e| Error::Backend(format!("task join error: {e}")))??;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }
}

#[async_trait]
impl SmtpSender for OrigamiSmtp {
    async fn send_message(&self, raw: &[u8]) -> Result<()> {
        let (reverse, forwards) = derive_envelope(raw)?;
        let message = raw.to_vec();
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            let mut client = client
                .lock()
                .map_err(|_| Error::Backend("SMTP session lock poisoned".into()))?;
            client
                .send(reverse, forwards, message)
                .map_err(|e| Error::Backend(e.to_string()))
        })
        .await
        .map_err(|e| Error::Backend(format!("task join error: {e}")))?
    }
}

fn connect_blocking(config: &SmtpConfig) -> Result<SmtpClientStd> {
    let scheme = if config.tls { "smtps" } else { "smtp" };
    let default_port = if config.tls {
        465
    } else if config.starttls {
        587
    } else {
        25
    };
    let port = config.port.unwrap_or(default_port);
    let url = Url::parse(&format!("{scheme}://{}:{port}", config.host))
        .map_err(|e| Error::Config(format!("invalid SMTP server URL: {e}")))?;
    let tls = pimalaya_stream::tls::Tls::default();
    let sasl = build_sasl(config, &url)?;
    // EHLO identity: address-literal, same as himalaya (no FQDN guessing).
    let domain: SmtpEhloDomain<'static> = Ipv4Addr::LOCALHOST.into();

    SmtpClientStd::connect(&url, &tls, config.starttls, domain, sasl)
        .map_err(|e| Error::Backend(e.to_string()))
}

fn build_sasl(config: &SmtpConfig, url: &Url) -> Result<Option<Sasl>> {
    let username = config.username.clone();
    let secret = config
        .secret
        .as_ref()
        .ok_or_else(|| Error::Config(format!("no secret configured for {}", config.host)))?
        .resolve()?;
    let secret = SecretString::from(secret);

    let sasl = match config.auth {
        AuthMechanism::Login => SaslLogin {
            username,
            password: secret,
        }
        .into(),
        AuthMechanism::Plain => SaslPlain {
            authzid: None,
            authcid: username,
            passwd: secret,
        }
        .into(),
        AuthMechanism::Xoauth2 => SaslXoauth2 {
            username,
            token: secret,
        }
        .into(),
        AuthMechanism::Oauthbearer => SaslOauthbearer {
            username,
            host: url.host_str().unwrap_or_default().to_string(),
            port: url.port().unwrap_or(465),
            token: secret,
        }
        .into(),
    };
    Ok(Some(sasl))
}

/// Derive the RFC 5321 envelope from RFC 5322 headers.
fn derive_envelope(
    raw: &[u8],
) -> Result<(SmtpReversePath<'static>, Vec<SmtpForwardPath<'static>>)> {
    let parsed = MessageParser::default()
        .parse_headers(raw)
        .ok_or_else(|| Error::Backend("cannot parse message headers".into()))?;

    let from = collect_addresses(parsed.from())
        .into_iter()
        .next()
        .ok_or_else(|| Error::Backend("no `From:` header in message".into()))?;
    let reverse = SmtpReversePath::SmtpMailbox(parse_mailbox(&from)?);

    let mut forwards = Vec::new();
    for group in [parsed.to(), parsed.cc(), parsed.bcc()] {
        for address in collect_addresses(group) {
            forwards.push(SmtpForwardPath(parse_mailbox(&address)?));
        }
    }
    if forwards.is_empty() {
        return Err(Error::Backend("no recipients in message".into()));
    }
    Ok((reverse, forwards))
}

fn collect_addresses(address: Option<&mail_parser::Address<'_>>) -> Vec<String> {
    let Some(address) = address else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match address {
        mail_parser::Address::List(list) => {
            for addr in list.iter() {
                if let Some(a) = &addr.address {
                    out.push(a.to_string());
                }
            }
        }
        mail_parser::Address::Group(groups) => {
            for group in groups.iter() {
                for addr in group.addresses.iter() {
                    if let Some(a) = &addr.address {
                        out.push(a.to_string());
                    }
                }
            }
        }
    }
    out
}

fn parse_mailbox(address: &str) -> Result<SmtpMailbox<'static>> {
    let (local, domain) = address
        .rsplit_once('@')
        .ok_or_else(|| Error::Backend(format!("invalid address `{address}`")))?;
    Ok(SmtpMailbox {
        local_part: SmtpLocalPart(Cow::Owned(local.to_string())),
        domain: SmtpEhloDomain::SmtpDomain(SmtpDomain(Cow::Owned(domain.to_string()))),
    })
}
