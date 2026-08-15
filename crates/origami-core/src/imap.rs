//! IMAP backend over pimalaya's git-pinned `io-imap` (see ADR-0002).
//!
//! `io-imap`'s std client is blocking and stateful (mailbox selection is
//! session state), so the backend serializes every operation through a
//! mutex and runs them on `tokio::task::spawn_blocking` workers. The sync
//! engine (M2) will replace this single-session pool with per-purpose
//! sessions (IDLE + fetch pool).

use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use io_imap::client::{ImapClientStd, ImapMailboxWatchStream};
use io_imap::rfc3501::append::ImapMessageAppendOptions;
use io_imap::rfc3501::fetch::ImapMessageFetchOptions;
use io_imap::rfc3501::search::ImapMessageSearchOptions;
use io_imap::rfc3501::select::ImapMailboxSelectOptions;
use io_imap::rfc3501::store::ImapMessageStoreOptions;
use io_imap::rfc6851::r#move::ImapMessageMoveOptions;
use io_imap::types::body::{BodyStructure, SpecificFields};
use io_imap::types::command::{FetchModifier, SelectParameter};
use io_imap::types::core::{AString, Atom, Vec1};
use io_imap::types::fetch::{
    MacroOrMessageDataItemNames, MessageDataItem, MessageDataItemName, Part, Section,
};
use io_imap::types::flag::{Flag as ImapFlag, FlagFetch, FlagNameAttribute, StoreType};
use io_imap::types::mailbox::{ListMailbox, Mailbox as ImapMailbox};
use io_imap::types::response::Capability;
use io_imap::types::search::SearchKey;
use io_imap::types::sequence::SequenceSet;
use io_imap::types::status::{StatusDataItem, StatusDataItemName};
use pimalaya_stream::sasl::{Sasl, SaslLogin, SaslOauthbearer, SaslPlain, SaslXoauth2};
use secrecy::SecretString;
use url::Url;

use crate::backend::MailBackend;
use crate::config::{AuthMechanism, ImapConfig};
use crate::message::{AttachmentMeta, DisplayMessage, DisplaySection, MimePart};
use crate::model::{Address, Envelope, Flag, Mailbox, MailboxRole};
use crate::{Error, Result};

/// IMAP implementation of [`MailBackend`]. Cheap to clone (shared session).
#[derive(Clone)]
pub struct ImapBackend {
    account_id: Arc<str>,
    client: Arc<Mutex<ImapClientStd>>,
    capabilities: Arc<Vec<Capability<'static>>>,
}

/// Mailbox state reported by SELECT, used by the sync planner (ADR-0003).
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectInfo {
    pub exists: u32,
    pub uid_validity: Option<u32>,
    pub highest_modseq: Option<u64>,
}

impl ImapBackend {
    /// Connect (TCP/TLS/STARTTLS + greeting + SASL) off the async runtime.
    pub async fn connect(account_id: &str, config: &ImapConfig) -> Result<Self> {
        let config = config.clone();
        let (client, capabilities) = tokio::task::spawn_blocking(move || connect_blocking(&config))
            .await
            .map_err(|e| Error::Backend(format!("task join error: {e}")))??;
        Ok(Self {
            account_id: account_id.into(),
            client: Arc::new(Mutex::new(client)),
            capabilities: Arc::new(capabilities),
        })
    }

    /// Whether the server advertised CONDSTORE/QRESYNC at connect time.
    pub fn supports_condstore(&self) -> bool {
        self.capabilities
            .iter()
            .any(|c| matches!(c, Capability::CondStore | Capability::QResync))
    }

    /// SELECT a mailbox, requesting CONDSTORE data when supported.
    pub async fn select_folder(&self, mailbox: &str, condstore: bool) -> Result<SelectInfo> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            let parameters = if condstore {
                vec![SelectParameter::CondStore]
            } else {
                vec![]
            };
            let data = client
                .select(
                    parse_mailbox(&mailbox)?,
                    ImapMailboxSelectOptions { parameters },
                )
                .map_err(backend_err)?;
            Ok(SelectInfo {
                exists: data.exists.unwrap_or(0),
                uid_validity: data.uid_validity.map(|v| v.get()),
                highest_modseq: data.highest_mod_seq,
            })
        })
        .await
    }

    /// UIDs strictly greater than `after_uid` (new arrivals).
    pub async fn search_uids_after(&self, mailbox: &str, after_uid: u32) -> Result<Vec<u32>> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let range = format!("{}:*", after_uid.saturating_add(1));
            let set: SequenceSet = range
                .as_str()
                .try_into()
                .map_err(|_| Error::Backend(format!("invalid sequence set `{range}`")))?;
            let criteria: Vec1<SearchKey<'static>> = vec![SearchKey::Uid(set)]
                .try_into()
                .map_err(|_| Error::Backend("empty search criteria".into()))?;
            let uids = client
                .search(criteria, ImapMessageSearchOptions { uid: true })
                .map_err(backend_err)?;
            // RFC 3501 gotcha: a UID range `n:*` ALWAYS matches the
            // mailbox's last message even when n exceeds every assigned
            // UID — so "new mail" must be filtered client-side.
            Ok(uids
                .iter()
                .map(|u| u.get())
                .filter(|u| *u > after_uid)
                .collect())
        })
        .await
    }

    /// All current UIDs in a mailbox, without fetching message metadata.
    /// Used to reconcile expunged or moved-away messages whose UIDs are not
    /// necessarily above the local high-water mark.
    pub async fn search_all_uids(&self, mailbox: &str) -> Result<Vec<u32>> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let criteria: Vec1<SearchKey<'static>> = vec![SearchKey::All]
                .try_into()
                .map_err(|_| Error::Backend("empty search criteria".into()))?;
            let uids = client
                .search(criteria, ImapMessageSearchOptions { uid: true })
                .map_err(backend_err)?;
            Ok(uids.iter().map(|uid| uid.get()).collect())
        })
        .await
    }

    /// Envelopes for explicit UIDs (delta new-mail fetch).
    pub async fn fetch_envelopes_by_uids(
        &self,
        mailbox: &str,
        uids: &[u32],
    ) -> Result<Vec<Envelope>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let mailbox = mailbox.to_string();
        let uids: Vec<NonZeroU32> = uids.iter().filter_map(|u| NonZeroU32::new(*u)).collect();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let set = SequenceSet::try_from(uids)
                .map_err(|_| Error::Backend("invalid UID set".into()))?;
            let rows = client
                .fetch(
                    set,
                    envelope_item_names(),
                    ImapMessageFetchOptions {
                        uid: true,
                        modifiers: vec![],
                    },
                )
                .map_err(backend_err)?;
            Ok(rows
                .into_iter()
                .map(|(seq, items)| envelope_from(seq.get(), items.into_iter().collect::<Vec<_>>()))
                .collect())
        })
        .await
    }

    /// (uid, flags, keywords) for the whole folder, or only messages changed since
    /// `changed_since` (CONDSTORE CHANGEDSINCE, ADR-0003 §2).
    pub async fn fetch_flags(
        &self,
        mailbox: &str,
        changed_since: Option<u64>,
    ) -> Result<Vec<(u32, Vec<Flag>, Vec<String>)>> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let set: SequenceSet = "1:*"
                .try_into()
                .map_err(|_| Error::Backend("invalid sequence set".into()))?;
            let item_names = MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::Uid,
                MessageDataItemName::Flags,
            ]);
            let modifiers = changed_since
                .and_then(NonZeroU64::new)
                .map(FetchModifier::ChangedSince)
                .into_iter()
                .collect();
            let rows = client
                .fetch(
                    set,
                    item_names,
                    ImapMessageFetchOptions {
                        uid: true,
                        modifiers,
                    },
                )
                .map_err(backend_err)?;
            let mut out = Vec::new();
            for (uid, items) in rows {
                let mut flags = Vec::new();
                let mut keywords = Vec::new();
                for item in items {
                    if let MessageDataItem::Flags(fetched) = item {
                        (flags, keywords) = flags_and_keywords(fetched);
                    }
                }
                out.push((uid.get(), flags, keywords));
            }
            Ok(out)
        })
        .await
    }

    /// Open a fresh dedicated session and consume it into a QRESYNC/IDLE
    /// watch on `mailbox`. Fully blocking — call from a blocking worker
    /// (`spawn_blocking`); the stream yields envelope/flag/expunge
    /// events until closed.
    pub fn watch_mailbox_blocking(
        config: &ImapConfig,
        mailbox: &str,
    ) -> Result<ImapMailboxWatchStream> {
        let (client, capabilities) = connect_blocking(config)?;
        client
            .watch_mailbox(parse_mailbox(mailbox)?, &capabilities)
            .map_err(backend_err)
    }

    /// APPEND a raw RFC 822 message to a mailbox; returns the assigned
    /// UID when the server supports UIDPLUS.
    pub async fn append_message(
        &self,
        mailbox: &str,
        raw: &[u8],
        flags: &[Flag],
    ) -> Result<Option<u32>> {
        let mailbox = mailbox.to_string();
        let raw = raw.to_vec();
        let flags = flags.to_vec();
        self.run(move |client| {
            let imap_flags = flags.iter().map(imap_flag_from).collect();
            let output = client
                .append(
                    parse_mailbox(&mailbox)?,
                    &raw,
                    ImapMessageAppendOptions {
                        flags: imap_flags,
                        date: None,
                        non_sync: false,
                    },
                )
                .map_err(backend_err)?;
            // ImapMessageAppendOutput = (exists, Option<(uid_validity, uid)>)
            Ok(output.1.map(|(_validity, uid)| uid))
        })
        .await
    }

    /// Create a mailbox (used by tests and future folder management UI).
    pub async fn create_mailbox(&self, mailbox: &str) -> Result<()> {
        let mailbox = mailbox.to_string();
        self.run(move |client| client.create(parse_mailbox(&mailbox)?).map_err(backend_err))
            .await
    }

    /// Delete a mailbox.
    pub async fn delete_mailbox(&self, mailbox: &str) -> Result<()> {
        let mailbox = mailbox.to_string();
        self.run(move |client| client.delete(parse_mailbox(&mailbox)?).map_err(backend_err))
            .await
    }

    pub async fn rename_mailbox(&self, mailbox: &str, new_name: &str) -> Result<()> {
        let mailbox = mailbox.to_string();
        let new_name = new_name.to_string();
        self.run(move |client| {
            client
                .rename(parse_mailbox(&mailbox)?, parse_mailbox(&new_name)?)
                .map_err(backend_err)
        })
        .await
    }

    /// Delete a message: set \Deleted + EXPUNGE.
    pub async fn delete_message(&self, mailbox: &str, server_uid: u32) -> Result<()> {
        self.delete_messages(mailbox, &[server_uid]).await
    }

    /// Move a UID batch in one command.
    pub async fn move_messages(
        &self,
        source_mailbox: &str,
        destination_mailbox: &str,
        server_uids: &[u32],
    ) -> Result<()> {
        if server_uids.is_empty() {
            return Ok(());
        }
        let source = source_mailbox.to_string();
        let destination = destination_mailbox.to_string();
        let uids: Vec<NonZeroU32> = server_uids
            .iter()
            .filter_map(|uid| NonZeroU32::new(*uid))
            .collect();
        self.run(move |client| {
            select_blocking(client, &source)?;
            let set = SequenceSet::try_from(uids)
                .map_err(|_| Error::Backend("invalid UID set".into()))?;
            client
                .r#move(
                    set,
                    parse_mailbox(&destination)?,
                    ImapMessageMoveOptions { uid: true },
                )
                .map_err(backend_err)?;
            Ok(())
        })
        .await
    }

    /// Mark a UID batch deleted and expunge once.
    pub async fn delete_messages(&self, mailbox: &str, server_uids: &[u32]) -> Result<()> {
        if server_uids.is_empty() {
            return Ok(());
        }
        let mailbox = mailbox.to_string();
        let uids: Vec<NonZeroU32> = server_uids
            .iter()
            .filter_map(|uid| NonZeroU32::new(*uid))
            .collect();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let set = SequenceSet::try_from(uids)
                .map_err(|_| Error::Backend("invalid UID set".into()))?;
            client
                .store(
                    set,
                    StoreType::Add,
                    vec![ImapFlag::Deleted],
                    ImapMessageStoreOptions { uid: true },
                )
                .map_err(backend_err)?;
            client.expunge().map_err(backend_err)?;
            Ok(())
        })
        .await
    }

    /// Replace system flags while either preserving current keywords or
    /// replacing them with an explicit set.
    pub async fn store_flags_and_keywords(
        &self,
        mailbox: &str,
        server_uid: u32,
        flags: &[Flag],
        keywords: Option<&[String]>,
    ) -> Result<()> {
        let mailbox = mailbox.to_string();
        let flags = flags.to_vec();
        let keywords = keywords.map(<[String]>::to_vec);
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let uid = NonZeroU32::new(server_uid)
                .ok_or_else(|| Error::Backend("UID 0 is invalid".into()))?;
            let make_set = || {
                SequenceSet::try_from(vec![uid])
                    .map_err(|_| Error::Backend("invalid UID set".into()))
            };
            let mut imap_flags: Vec<ImapFlag<'static>> = flags.iter().map(imap_flag_from).collect();

            if let Some(keywords) = keywords {
                for keyword in keywords {
                    let atom = Atom::try_from(keyword)
                        .map_err(|error| Error::Backend(format!("invalid keyword: {error}")))?;
                    imap_flags.push(ImapFlag::Keyword(atom));
                }
            } else {
                let items = MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                    MessageDataItemName::Flags,
                ]);
                let rows = client
                    .fetch(
                        make_set()?,
                        items,
                        ImapMessageFetchOptions {
                            uid: true,
                            modifiers: vec![],
                        },
                    )
                    .map_err(backend_err)?;
                for (_, items) in rows {
                    for item in items {
                        if let MessageDataItem::Flags(fetched) = item {
                            for flag in fetched {
                                match flag {
                                    FlagFetch::Flag(flag @ ImapFlag::Keyword(_))
                                    | FlagFetch::Flag(flag @ ImapFlag::Extension(_)) => {
                                        imap_flags.push(flag)
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }

            client
                .store(
                    make_set()?,
                    StoreType::Replace,
                    imap_flags,
                    ImapMessageStoreOptions { uid: true },
                )
                .map_err(backend_err)?;
            Ok(())
        })
        .await
    }

    /// Fetch top-level headers and displayable MIME leaves without fetching
    /// attachment bodies. All requests use BODY.PEEK so opening a message
    /// never changes its read state.
    pub async fn fetch_display_message(
        &self,
        mailbox: &str,
        server_uid: u32,
    ) -> Result<DisplayMessage> {
        let mailbox = mailbox.to_string();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let base_items = MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::BodyStructure,
                MessageDataItemName::BodyExt {
                    section: Some(Section::Header(None)),
                    partial: None,
                    peek: true,
                },
            ]);
            let rows = client
                .fetch(uid_sequence(server_uid)?, base_items, peek_fetch_options())
                .map_err(backend_err)?;
            let mut body_structure = None;
            let mut headers = Vec::new();
            for (_, items) in rows {
                for item in items {
                    match item {
                        MessageDataItem::BodyStructure(value) => body_structure = Some(value),
                        MessageDataItem::BodyExt { data, .. } => {
                            headers = data
                                .0
                                .map(|bytes| bytes.as_ref().to_vec())
                                .unwrap_or_default();
                        }
                        _ => {}
                    }
                }
            }
            let body_structure = body_structure
                .ok_or_else(|| Error::Backend("no BODYSTRUCTURE in FETCH response".into()))?;
            let structure = display_structure(&body_structure);
            if structure.display_paths.is_empty() {
                return Ok(DisplayMessage {
                    headers,
                    parts: structure.parts,
                    attachments: structure.attachments,
                    sections: Vec::new(),
                });
            }

            let mut requests = Vec::with_capacity(structure.display_paths.len() * 2);
            for path in &structure.display_paths {
                let part = imap_part(path)?;
                requests.push(MessageDataItemName::BodyExt {
                    section: Some(Section::Mime(part.clone())),
                    partial: None,
                    peek: true,
                });
                requests.push(MessageDataItemName::BodyExt {
                    section: Some(Section::Part(part)),
                    partial: None,
                    peek: true,
                });
            }
            let rows = client
                .fetch(
                    uid_sequence(server_uid)?,
                    requests.into(),
                    peek_fetch_options(),
                )
                .map_err(backend_err)?;
            let mut fetched = HashMap::new();
            for (_, items) in rows {
                for item in items {
                    if let MessageDataItem::BodyExt {
                        section: Some(section),
                        data,
                        ..
                    } = item
                    {
                        if let Some(key) = fetched_section_key(&section) {
                            fetched.insert(
                                key,
                                data.0
                                    .map(|bytes| bytes.as_ref().to_vec())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            let sections = structure
                .display_paths
                .into_iter()
                .map(|path| DisplaySection {
                    mime_headers: fetched.remove(&(true, path.clone())).unwrap_or_default(),
                    body: fetched.remove(&(false, path.clone())).unwrap_or_default(),
                    path,
                })
                .collect();
            Ok(DisplayMessage {
                headers,
                parts: structure.parts,
                attachments: structure.attachments,
                sections,
            })
        })
        .await
    }

    /// Fetch and decode one attachment MIME section on demand.
    pub async fn fetch_attachment_section(
        &self,
        mailbox: &str,
        server_uid: u32,
        path: &str,
    ) -> Result<Vec<u8>> {
        let mailbox = mailbox.to_string();
        let path = path.to_string();
        self.run(move |client| {
            select_blocking(client, &mailbox)?;
            let part = imap_part(&path)?;
            let rows = client
                .fetch(
                    uid_sequence(server_uid)?,
                    vec![
                        MessageDataItemName::BodyExt {
                            section: Some(Section::Mime(part.clone())),
                            partial: None,
                            peek: true,
                        },
                        MessageDataItemName::BodyExt {
                            section: Some(Section::Part(part)),
                            partial: None,
                            peek: true,
                        },
                    ]
                    .into(),
                    peek_fetch_options(),
                )
                .map_err(backend_err)?;
            let mut mime_headers = None;
            let mut body = None;
            for (_, items) in rows {
                for item in items {
                    if let MessageDataItem::BodyExt {
                        section: Some(section),
                        data,
                        ..
                    } = item
                    {
                        let Some((is_mime, section_path)) = fetched_section_key(&section) else {
                            continue;
                        };
                        if section_path != path {
                            continue;
                        }
                        let bytes = data
                            .0
                            .map(|value| value.as_ref().to_vec())
                            .unwrap_or_default();
                        if is_mime {
                            mime_headers = Some(bytes);
                        } else {
                            body = Some(bytes);
                        }
                    }
                }
            }
            crate::message::decode_mime_part(
                &mime_headers.unwrap_or_default(),
                &body.unwrap_or_default(),
            )
            .ok_or_else(|| Error::Backend(format!("could not decode MIME section {path}")))
        })
        .await
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
            for (mbox, _delimiter, attrs) in listing {
                if !mailbox_is_selectable(&attrs) {
                    continue;
                }
                let name = mailbox_name(&mbox);
                // STATUS is best-effort because some selectable servers
                // still restrict it for special-purpose mailboxes.
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
                MessageDataItemName::InternalDate,
                MessageDataItemName::BodyStructure,
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
        self.store_flags_and_keywords(mailbox, server_uid, flags, None)
            .await
    }
}

fn mailbox_is_selectable(attributes: &[FlagNameAttribute<'_>]) -> bool {
    !attributes.contains(&FlagNameAttribute::Noselect)
}

// ------------------------------------------------------------------
// Blocking helpers
// ------------------------------------------------------------------

fn connect_blocking(config: &ImapConfig) -> Result<(ImapClientStd, Vec<Capability<'static>>)> {
    let scheme = if config.tls { "imaps" } else { "imap" };
    let port = config.port.unwrap_or(if config.tls { 993 } else { 143 });
    let url = Url::parse(&format!("{scheme}://{}:{port}", config.host))
        .map_err(|e| Error::Config(format!("invalid IMAP server URL: {e}")))?;
    let tls = pimalaya_stream::tls::Tls::default();
    let sasl = build_sasl(config, &url)?;

    let (mut client, greeting_capabilities) =
        ImapClientStd::connect(&url, &tls, config.starttls, sasl, None).map_err(backend_err)?;

    // The greeting's capability list is often minimal (Dovecot hides
    // CONDSTORE/QRESYNC until after auth); refresh post-login and fall
    // back to the greeting list if the server refuses.
    let mut capabilities = client.capability().unwrap_or(greeting_capabilities);

    // WORKAROUND (imap-codec 2.0.0-alpha.8): the CAPABILITY response
    // parser mis-decodes CONDSTORE/QRESYNC (they come back as duplicate
    // UNSELECT variants). Cross-check against the raw response text and
    // patch the known-good variants in. TODO: drop when the pinned
    // io-imap/imap-codec is bumped past the fix; file upstream.
    if let Ok(raw) = client.raw("CAPABILITY") {
        let raw = raw.to_uppercase();
        if raw.contains("CONDSTORE")
            && !capabilities
                .iter()
                .any(|c| matches!(c, Capability::CondStore))
        {
            capabilities.push(Capability::CondStore);
        }
        if raw.contains("QRESYNC")
            && !capabilities
                .iter()
                .any(|c| matches!(c, Capability::QResync))
        {
            capabilities.push(Capability::QResync);
        }
    }
    Ok((client, capabilities))
}

/// SELECT a mailbox with default options (internal helper).
fn select_blocking(client: &mut ImapClientStd, mailbox: &str) -> Result<()> {
    client
        .select(parse_mailbox(mailbox)?, ImapMailboxSelectOptions::default())
        .map_err(backend_err)?;
    Ok(())
}

/// FETCH item list for envelope reads: UID + FLAGS + ENVELOPE + BODYSTRUCTURE
/// + RFC822.SIZE + selected threading headers.
fn envelope_item_names() -> MacroOrMessageDataItemNames<'static> {
    let headers = Vec1::try_from(vec![
        AString::try_from("References".to_string()).expect("static header name"),
        AString::try_from("In-Reply-To".to_string()).expect("static header name"),
    ])
    .expect("non-empty header list");
    MacroOrMessageDataItemNames::MessageDataItemNames(vec![
        MessageDataItemName::Uid,
        MessageDataItemName::Flags,
        MessageDataItemName::Envelope,
        MessageDataItemName::InternalDate,
        MessageDataItemName::BodyStructure,
        MessageDataItemName::Rfc822Size,
        MessageDataItemName::BodyExt {
            section: Some(Section::HeaderFields(None, headers)),
            partial: None,
            peek: true,
        },
    ])
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

fn uid_sequence(server_uid: u32) -> Result<SequenceSet> {
    let uid =
        NonZeroU32::new(server_uid).ok_or_else(|| Error::Backend("UID 0 is invalid".into()))?;
    SequenceSet::try_from(vec![uid]).map_err(|_| Error::Backend("invalid UID set".into()))
}

fn peek_fetch_options() -> ImapMessageFetchOptions {
    ImapMessageFetchOptions {
        uid: true,
        modifiers: vec![],
    }
}

fn imap_part(path: &str) -> Result<Part> {
    let numbers: Vec<NonZeroU32> = path
        .split('.')
        .map(|value| {
            value
                .parse::<u32>()
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or_else(|| Error::Backend(format!("invalid MIME part path `{path}`")))
        })
        .collect::<Result<_>>()?;
    let numbers: Vec1<NonZeroU32> = numbers
        .try_into()
        .map_err(|_| Error::Backend(format!("empty MIME part path `{path}`")))?;
    Ok(Part(numbers))
}

fn fetched_section_key(section: &Section<'_>) -> Option<(bool, String)> {
    match section {
        Section::Mime(part) => Some((true, imap_part_path(part))),
        Section::Part(part) => Some((false, imap_part_path(part))),
        _ => None,
    }
}

fn imap_part_path(part: &Part) -> String {
    part.0
        .as_ref()
        .iter()
        .map(|number| number.get().to_string())
        .collect::<Vec<_>>()
        .join(".")
}

fn mailbox_name(mbox: &ImapMailbox<'_>) -> String {
    match mbox {
        ImapMailbox::Inbox => "INBOX".to_string(),
        // io-imap decodes modified UTF-7 in LIST responses before exposing
        // mailbox values. Do not decode a second time here.
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
    let mut message_id = None;
    let mut thread_id = None;
    let mut flags = Vec::new();
    let mut keywords = Vec::new();
    let mut has_attachment = false;
    let mut subject = String::new();
    let mut from = Vec::new();
    let mut to = Vec::new();
    let mut date = None;
    let mut received_at = None;
    let mut size = 0;

    for item in items {
        match item {
            MessageDataItem::Uid(uid) => server_uid = Some(uid.get()),
            MessageDataItem::Flags(fetched) => {
                for fetch in fetched {
                    match fetch {
                        FlagFetch::Flag(ImapFlag::Seen) => flags.push(Flag::Seen),
                        FlagFetch::Flag(ImapFlag::Answered) => flags.push(Flag::Answered),
                        FlagFetch::Flag(ImapFlag::Flagged) => flags.push(Flag::Flagged),
                        FlagFetch::Flag(ImapFlag::Deleted) => flags.push(Flag::Deleted),
                        FlagFetch::Flag(ImapFlag::Draft) => flags.push(Flag::Draft),
                        FlagFetch::Flag(ImapFlag::Extension(ext)) => {
                            let s = ImapFlag::Extension(ext).to_string();
                            keywords.push(s.trim_start_matches('\\').to_string());
                        }
                        FlagFetch::Flag(ImapFlag::Keyword(kw)) => {
                            keywords.push(ImapFlag::Keyword(kw).to_string());
                        }
                        FlagFetch::Recent => {}
                    }
                }
            }
            MessageDataItem::Envelope(env) => {
                if let Some(s) = env.subject.into_option() {
                    subject = decode_mime_words(s.as_ref());
                }
                if let Some(d) = env.date.into_option() {
                    date = Some(String::from_utf8_lossy(d.as_ref()).trim().to_string());
                }
                if let Some(m) = env.message_id.into_option() {
                    message_id = Some(
                        String::from_utf8_lossy(m.as_ref())
                            .trim()
                            .trim_matches(['<', '>'])
                            .to_string(),
                    );
                }
                from = env.from.iter().map(address_from).collect();
                to = env.to.iter().map(address_from).collect();
            }
            MessageDataItem::InternalDate(value) => {
                received_at = Some(value.as_ref().timestamp());
            }
            MessageDataItem::Rfc822Size(n) => size = n,
            MessageDataItem::BodyStructure(body) => {
                has_attachment = body_structure_has_attachment(&body);
            }
            MessageDataItem::BodyExt { data, .. } => {
                if let Some(bytes) = data.0 {
                    thread_id = reference_root(bytes.as_ref());
                }
            }
            _ => {}
        }
    }

    if thread_id.is_none() {
        thread_id = message_id.clone();
    }
    Envelope {
        id: uuid::Uuid::now_v7().to_string(),
        mailbox_id: String::new(),
        subject,
        from,
        to,
        date,
        received_at,
        flags,
        keywords,
        has_attachment,
        size,
        server_uid: server_uid.or(Some(seq)),
        message_id,
        thread_id,
        sources: Vec::new(),
    }
}

fn body_structure_has_attachment(body: &BodyStructure<'_>) -> bool {
    !display_structure(body).attachments.is_empty()
}

struct DisplayStructure {
    parts: Vec<MimePart>,
    attachments: Vec<AttachmentMeta>,
    display_paths: Vec<String>,
}

fn display_structure(body: &BodyStructure<'_>) -> DisplayStructure {
    let mut structure = DisplayStructure {
        parts: Vec::new(),
        attachments: Vec::new(),
        display_paths: Vec::new(),
    };
    let root_path = if matches!(body, BodyStructure::Multi { .. }) {
        ""
    } else {
        "1"
    };
    collect_display_structure(body, root_path, &mut structure);
    structure
}

fn collect_display_structure(
    body: &BodyStructure<'_>,
    path: &str,
    structure: &mut DisplayStructure,
) {
    match body {
        BodyStructure::Multi {
            bodies, subtype, ..
        } => {
            structure.parts.push(MimePart {
                path: path.to_string(),
                mime: format!("multipart/{}", imap_text(subtype).to_ascii_lowercase()),
                charset: None,
                disposition: None,
                filename: None,
                content_id: None,
                content_location: None,
                encoded_size: 0,
                decoded_size: 0,
                inline: false,
                attachment: false,
                text: false,
                html: false,
                encoding_problem: false,
            });
            for (index, child) in bodies.as_ref().iter().enumerate() {
                let child_path = if path.is_empty() {
                    (index + 1).to_string()
                } else {
                    format!("{path}.{}", index + 1)
                };
                collect_display_structure(child, &child_path, structure);
            }
        }
        BodyStructure::Single {
            body,
            extension_data,
        } => {
            let parameter = |key: &str| {
                body.basic
                    .parameter_list
                    .iter()
                    .find(|(name, _)| name.as_ref().eq_ignore_ascii_case(key.as_bytes()))
                    .map(|(_, value)| imap_text(value))
            };
            let disposition_info = extension_data
                .as_ref()
                .and_then(|extension| extension.tail.as_ref())
                .and_then(|disposition| disposition.disposition.as_ref())
                .map(|(kind, parameters)| {
                    let filename = parameters
                        .iter()
                        .find(|(name, _)| name.as_ref().eq_ignore_ascii_case(b"filename"))
                        .map(|(_, value)| imap_text(value));
                    (imap_text(kind).to_ascii_lowercase(), filename)
                });
            let (mime, text, html, nested_message) = match &body.specific {
                SpecificFields::Basic { r#type, subtype } => (
                    format!(
                        "{}/{}",
                        imap_text(r#type).to_ascii_lowercase(),
                        imap_text(subtype).to_ascii_lowercase()
                    ),
                    false,
                    false,
                    false,
                ),
                SpecificFields::Text { subtype, .. } => {
                    let subtype = imap_text(subtype).to_ascii_lowercase();
                    (format!("text/{subtype}"), true, subtype == "html", false)
                }
                SpecificFields::Message { .. } => {
                    ("message/rfc822".to_string(), false, false, true)
                }
            };
            let content_name = parameter("name");
            let disposition_name = disposition_info
                .as_ref()
                .and_then(|(_, filename)| filename.clone());
            let filename = disposition_name.or(content_name);
            let disposition = disposition_info.as_ref().map(|(kind, _)| kind.clone());
            let marked_attachment = filename.is_some()
                || disposition.as_deref() == Some("attachment")
                || nested_message;
            let displayable = text && !marked_attachment;
            let attachment = !displayable;
            let inline = disposition.as_deref() == Some("inline");
            let content_id = body
                .basic
                .id
                .0
                .as_ref()
                .map(|value| imap_text(value).trim_matches(['<', '>']).to_string());
            structure.parts.push(MimePart {
                path: path.to_string(),
                mime,
                charset: parameter("charset"),
                disposition: disposition.clone(),
                filename: filename.clone(),
                content_id: content_id.clone(),
                content_location: None,
                encoded_size: body.basic.size as usize,
                decoded_size: body.basic.size as usize,
                inline,
                attachment,
                text,
                html,
                encoding_problem: false,
            });
            if displayable {
                structure.display_paths.push(path.to_string());
            } else {
                let index = structure.attachments.len();
                structure.attachments.push(AttachmentMeta {
                    index,
                    part_path: path.to_string(),
                    name: filename,
                    mime: structure
                        .parts
                        .last()
                        .map(|part| part.mime.clone())
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    size: body.basic.size as usize,
                    inline,
                    cid: content_id,
                });
            }
        }
    }
}

fn imap_text(value: &impl AsRef<[u8]>) -> String {
    String::from_utf8_lossy(value.as_ref()).into_owned()
}

fn reference_root(raw_headers: &[u8]) -> Option<String> {
    let parsed = mail_parser::MessageParser::default().parse_headers(raw_headers)?;
    parsed
        .references()
        .as_text_list()
        .and_then(|references| references.first().map(|reference| reference.to_string()))
        .or_else(|| parsed.in_reply_to().as_text().map(str::to_string))
        .map(|id| id.trim_matches(['<', '>']).to_string())
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

fn flags_and_keywords(fetched: Vec<FlagFetch<'static>>) -> (Vec<Flag>, Vec<String>) {
    let mut flags = Vec::new();
    let mut keywords = Vec::new();
    for fetch in fetched {
        match fetch {
            FlagFetch::Flag(ImapFlag::Seen) => flags.push(Flag::Seen),
            FlagFetch::Flag(ImapFlag::Answered) => flags.push(Flag::Answered),
            FlagFetch::Flag(ImapFlag::Flagged) => flags.push(Flag::Flagged),
            FlagFetch::Flag(ImapFlag::Deleted) => flags.push(Flag::Deleted),
            FlagFetch::Flag(ImapFlag::Draft) => flags.push(Flag::Draft),
            FlagFetch::Flag(ImapFlag::Extension(extension)) => {
                let value = ImapFlag::Extension(extension).to_string();
                keywords.push(value.trim_start_matches('\\').to_string());
            }
            FlagFetch::Flag(ImapFlag::Keyword(keyword)) => {
                keywords.push(ImapFlag::Keyword(keyword).to_string());
            }
            FlagFetch::Recent => {}
        }
    }
    (flags, keywords)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noselect_hierarchy_nodes_are_not_synchronized() {
        assert!(mailbox_is_selectable(&[]));
        assert!(mailbox_is_selectable(&[FlagNameAttribute::Marked]));
        assert!(!mailbox_is_selectable(&[FlagNameAttribute::Noselect]));
    }

    #[test]
    fn mailbox_name_preserves_upstream_decoded_unicode() {
        let original = "旅行 & プロジェクト";
        let mailbox: ImapMailbox<'static> = original.to_string().try_into().unwrap();
        assert_eq!(mailbox_name(&mailbox), original);
    }
}
