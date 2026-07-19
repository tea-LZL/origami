//! IMAP backend over pimalaya's git-pinned `io-imap` (see ADR-0002).
//!
//! `io-imap`'s std client is blocking and stateful (mailbox selection is
//! session state), so the backend serializes every operation through a
//! mutex and runs them on `tokio::task::spawn_blocking` workers. The sync
//! engine (M2) will replace this single-session pool with per-purpose
//! sessions (IDLE + fetch pool).

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use io_imap::client::ImapClientStd;
use io_imap::rfc3501::fetch::ImapMessageFetchOptions;
use io_imap::rfc3501::select::ImapMailboxSelectOptions;
use io_imap::rfc3501::store::ImapMessageStoreOptions;
use io_imap::types::fetch::{MacroOrMessageDataItemNames, MessageDataItem, MessageDataItemName};
use io_imap::types::flag::{Flag as ImapFlag, FlagFetch, StoreType};
use io_imap::types::mailbox::{ListMailbox, Mailbox as ImapMailbox};
use io_imap::types::sequence::SequenceSet;
use io_imap::types::status::{StatusDataItem, StatusDataItemName};
use pimalaya_stream::sasl::{Sasl, SaslLogin, SaslOauthbearer, SaslPlain, SaslXoauth2};
use secrecy::SecretString;
use url::Url;

use crate::backend::MailBackend;
use crate::config::{AuthMechanism, ImapConfig};
use crate::model::{Address, Envelope, Flag, Mailbox, MailboxRole};
use crate::{Error, Result};

/// IMAP implementation of [`MailBackend`]. Cheap to clone (shared session).
#[derive(Clone)]
pub struct ImapBackend {
    account_id: Arc<str>,
    client: Arc<Mutex<ImapClientStd>>,
}

impl ImapBackend {
    /// Connect (TCP/TLS/STARTTLS + greeting + SASL) off the async runtime.
    pub async fn connect(account_id: &str, config: &ImapConfig) -> Result<Self> {
        let config = config.clone();
        let client = tokio::task::spawn_blocking(move || connect_blocking(&config))
            .await
            .map_err(|e| Error::Backend(format!("task join error: {e}")))??;
        Ok(Self {
            account_id: account_id.into(),
            client: Arc::new(Mutex::new(client)),
        })
    }

    /// Run a blocking operation on the shared session.
    async fn run<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut ImapClientStd) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            let mut client = client
                .lock()
                .map_err(|_| Error::Backend("IMAP session lock poisoned".into()))?;
            f(&mut client)
        })
        .await
        .map_err(|e| Error::Backend(format!("task join error: {e}")))?
    }
}

#[async_trait]
impl MailBackend for ImapBackend {
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>> {
        let account_id = Arc::clone(&self.account_id);
        self.run(move |client| {
            let reference: ImapMailbox<'static> = parse_mailbox("")?;
            let pattern: ListMailbox<'static> = "*"
                .try_into()
                .map_err(|_| Error::Backend("invalid LIST pattern".into()))?;
            let listing = client.list(reference, pattern).map_err(backend_err)?;

            let mut mailboxes = Vec::with_capacity(listing.len());
            for (mbox, _delimiter, _attrs) in listing {
                let name = mailbox_name(&mbox);
                // STATUS is best-effort: some servers reject it on
                // \Noselect parents; a failure must not hide the mailbox.
                let (total, unread) = status_counts(client, &name).unwrap_or((0, 0));
                mailboxes.push(Mailbox {
                    id: uuid::Uuid::now_v7().to_string(),
                    account_id: account_id.to_string(),
                    role: role_from_name(&name),
                    name,
                    total,
                    unread,
                });
            }
            Ok(mailboxes)
        })
        .await
    }

    async fn list_envelopes(
        &self,
        mailbox: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Envelope>> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            // SELECT gives us `exists` for the sequence window; re-selecting
            // per call keeps the session state explicit and stateless.
            let selected = client
                .select(
                    parse_mailbox(&mailbox)?,
                    ImapMailboxSelectOptions::default(),
                )
                .map_err(backend_err)?;
            let exists = selected.exists.unwrap_or(0);

            let Some(window) = sequence_window(exists, page, page_size) else {
                return Ok(Vec::new());
            };
            let sequence_set: SequenceSet = window
                .as_str()
                .try_into()
                .map_err(|_| Error::Backend(format!("invalid sequence set `{window}`")))?;

            let item_names = MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::Uid,
                MessageDataItemName::Flags,
                MessageDataItemName::Envelope,
                MessageDataItemName::Rfc822Size,
            ]);
            let rows = client
                .fetch(sequence_set, item_names, ImapMessageFetchOptions::default())
                .map_err(backend_err)?;

            let mut envelopes: Vec<Envelope> = rows
                .into_iter()
                .map(|(seq, items)| envelope_from(seq.get(), items.into_iter().collect::<Vec<_>>()))
                .collect();
            envelopes.reverse(); // sequence window is ascending; UI wants newest first
            Ok(envelopes)
        })
        .await
    }

    async fn fetch_message(&self, mailbox: &str, server_uid: u32) -> Result<Vec<u8>> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            client
                .select(
                    parse_mailbox(&mailbox)?,
                    ImapMailboxSelectOptions::default(),
                )
                .map_err(backend_err)?;

            let uid = NonZeroU32::new(server_uid)
                .ok_or_else(|| Error::Backend("UID 0 is invalid".into()))?;
            let sequence_set = SequenceSet::try_from(vec![uid])
                .map_err(|_| Error::Backend("invalid UID set".into()))?;
            // BODY.PEEK[] — never marks the message \Seen.
            let item_names = MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::BodyExt {
                    section: None,
                    partial: None,
                    peek: true,
                },
            ]);
            let rows = client
                .fetch(
                    sequence_set,
                    item_names,
                    ImapMessageFetchOptions {
                        uid: true,
                        modifiers: vec![],
                    },
                )
                .map_err(backend_err)?;

            for (_seq, items) in rows {
                for item in items {
                    if let MessageDataItem::BodyExt { data, .. } = item {
                        if let Some(bytes) = data.0 {
                            return Ok(bytes.as_ref().to_vec());
                        }
                    }
                }
            }
            Err(Error::Backend(format!(
                "no body in FETCH response for UID {server_uid}"
            )))
        })
        .await
    }

    async fn store_flags(&self, mailbox: &str, server_uid: u32, flags: &[Flag]) -> Result<()> {
        let mailbox = mailbox.to_string();
        let flags = flags.to_vec();
        self.run(move |client| {
            client
                .select(
                    parse_mailbox(&mailbox)?,
                    ImapMailboxSelectOptions::default(),
                )
                .map_err(backend_err)?;
            let uid = NonZeroU32::new(server_uid)
                .ok_or_else(|| Error::Backend("UID 0 is invalid".into()))?;
            let sequence_set = SequenceSet::try_from(vec![uid])
                .map_err(|_| Error::Backend("invalid UID set".into()))?;
            let imap_flags = flags.iter().map(imap_flag_from).collect();
            client
                .store(
                    sequence_set,
                    StoreType::Replace,
                    imap_flags,
                    ImapMessageStoreOptions { uid: true },
                )
                .map_err(backend_err)?;
            Ok(())
        })
        .await
    }
}

// ------------------------------------------------------------------
// Blocking helpers
// ------------------------------------------------------------------

fn connect_blocking(config: &ImapConfig) -> Result<ImapClientStd> {
    let scheme = if config.tls { "imaps" } else { "imap" };
    let port = config.port.unwrap_or(if config.tls { 993 } else { 143 });
    let url = Url::parse(&format!("{scheme}://{}:{port}", config.host))
        .map_err(|e| Error::Config(format!("invalid IMAP server URL: {e}")))?;
    let tls = pimalaya_stream::tls::Tls::default();
    let sasl = build_sasl(config, &url)?;

    let (client, _capabilities) =
        ImapClientStd::connect(&url, &tls, config.starttls, sasl, None).map_err(backend_err)?;
    Ok(client)
}

fn build_sasl(config: &ImapConfig, url: &Url) -> Result<Option<Sasl>> {
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
            port: url.port().unwrap_or(993),
            token: secret,
        }
        .into(),
    };
    Ok(Some(sasl))
}

fn parse_mailbox(name: &str) -> Result<ImapMailbox<'static>> {
    String::from(name)
        .try_into()
        .map_err(|_| Error::Backend(format!("invalid IMAP mailbox `{name}`")))
}

fn mailbox_name(mbox: &ImapMailbox<'_>) -> String {
    match mbox {
        ImapMailbox::Inbox => "INBOX".to_string(),
        // TODO(M2): decode modified UTF-7 for non-ASCII mailbox names.
        ImapMailbox::Other(other) => String::from_utf8_lossy(other.inner().as_ref()).into_owned(),
    }
}

fn status_counts(client: &mut ImapClientStd, name: &str) -> Result<(u32, u32)> {
    let items = client
        .status(
            parse_mailbox(name)?,
            vec![StatusDataItemName::Messages, StatusDataItemName::Unseen],
        )
        .map_err(backend_err)?;
    let mut total = 0;
    let mut unread = 0;
    for item in items {
        match item {
            StatusDataItem::Messages(n) => total = n,
            StatusDataItem::Unseen(n) => unread = n,
            _ => {}
        }
    }
    Ok((total, unread))
}

/// Sequence-number window for `(page, page_size)` against `exists`;
/// page 1 is the most recent window. `None` when the window is empty.
fn sequence_window(exists: u32, page: u32, page_size: u32) -> Option<String> {
    if exists == 0 || page_size == 0 {
        return None;
    }
    let skip = page.max(1).saturating_sub(1).saturating_mul(page_size);
    if skip >= exists {
        return None;
    }
    let end = exists - skip;
    let start = end.saturating_sub(page_size - 1).max(1);
    Some(format!("{start}:{end}"))
}

// ------------------------------------------------------------------
// Wire → domain mapping
// ------------------------------------------------------------------

fn envelope_from(seq: u32, items: Vec<MessageDataItem<'static>>) -> Envelope {
    let mut server_uid = None;
    let mut flags = Vec::new();
    let mut subject = String::new();
    let mut from = Vec::new();
    let mut to = Vec::new();
    let mut date = None;
    let mut size = 0;

    for item in items {
        match item {
            MessageDataItem::Uid(uid) => server_uid = Some(uid.get()),
            MessageDataItem::Flags(fetched) => {
                flags = fetched.into_iter().filter_map(flag_from_fetch).collect();
            }
            MessageDataItem::Envelope(env) => {
                if let Some(s) = env.subject.into_option() {
                    subject = decode_mime_words(s.as_ref());
                }
                if let Some(d) = env.date.into_option() {
                    date = Some(String::from_utf8_lossy(d.as_ref()).trim().to_string());
                }
                from = env.from.iter().map(address_from).collect();
                to = env.to.iter().map(address_from).collect();
            }
            MessageDataItem::Rfc822Size(n) => size = n,
            _ => {}
        }
    }

    Envelope {
        id: uuid::Uuid::now_v7().to_string(),
        mailbox_id: String::new(), // filled by the sync engine (M2)
        subject,
        from,
        to,
        date,
        flags,
        has_attachment: false, // BODYSTRUCTURE arrives with the sync engine (M2)
        size,
        server_uid: server_uid.or(Some(seq)),
    }
}

fn address_from(addr: &io_imap::types::envelope::Address<'_>) -> Address {
    let name = addr.name.0.as_ref().map(|n| decode_mime_words(n.as_ref()));
    let mailbox = addr
        .mailbox
        .0
        .as_ref()
        .map(|m| String::from_utf8_lossy(m.as_ref()).into_owned())
        .unwrap_or_default();
    let host = addr
        .host
        .0
        .as_ref()
        .map(|h| String::from_utf8_lossy(h.as_ref()).into_owned())
        .unwrap_or_default();
    Address {
        name: name.filter(|n| !n.is_empty()),
        addr: format!("{mailbox}@{host}"),
    }
}

fn flag_from_fetch(fetch: FlagFetch<'_>) -> Option<Flag> {
    let FlagFetch::Flag(flag) = fetch else {
        return None; // \Recent is session state, not a stored flag
    };
    match flag {
        ImapFlag::Seen => Some(Flag::Seen),
        ImapFlag::Answered => Some(Flag::Answered),
        ImapFlag::Flagged => Some(Flag::Flagged),
        ImapFlag::Deleted => Some(Flag::Deleted),
        ImapFlag::Draft => Some(Flag::Draft),
        _ => None, // keywords map to tags in M5
    }
}

fn imap_flag_from(flag: &Flag) -> ImapFlag<'static> {
    match flag {
        Flag::Seen => ImapFlag::Seen,
        Flag::Answered => ImapFlag::Answered,
        Flag::Flagged => ImapFlag::Flagged,
        Flag::Deleted => ImapFlag::Deleted,
        Flag::Draft => ImapFlag::Draft,
    }
}

fn role_from_name(name: &str) -> MailboxRole {
    match name.to_ascii_lowercase().as_str() {
        "inbox" => MailboxRole::Inbox,
        "sent" | "sent items" | "sent mail" | "[gmail]/sent mail" => MailboxRole::Sent,
        "drafts" | "[gmail]/drafts" => MailboxRole::Drafts,
        "trash" | "bin" | "deleted" | "deleted items" | "[gmail]/trash" => MailboxRole::Trash,
        "archive" | "all mail" | "[gmail]/all mail" => MailboxRole::Archive,
        "junk" | "spam" | "junk e-mail" | "[gmail]/spam" => MailboxRole::Junk,
        _ => MailboxRole::Other,
    }
}

/// Decode RFC 2047 encoded words (=?UTF-8?B?...?=), recovering from
/// malformed input rather than failing (himalaya does the same).
fn decode_mime_words(bytes: &[u8]) -> String {
    rfc2047_decoder::Decoder::new()
        .too_long_encoded_word_strategy(rfc2047_decoder::RecoverStrategy::Decode)
        .decode(bytes)
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
}

fn backend_err(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}
