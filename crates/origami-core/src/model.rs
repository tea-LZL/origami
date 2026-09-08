//! Domain model shared across all backends.
//!
//! Key ownership rule (Thunderbird Panorama ADR-0002): Origami owns all
//! primary keys. Server identifiers (IMAP UID, UIDVALIDITY, RFC 8474
//! OBJECTID) are stored as attributes, never used as our keys.

use serde::{Deserialize, Serialize};

/// An email account configured in Origami.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// App-owned UUID primary key.
    pub id: String,
    pub name: String,
    pub email: String,
}

/// Role of a mailbox, normalized across backends (IANA special-use,
/// RFC 6154).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Junk,
    Other,
}

impl MailboxRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MailboxRole::Inbox => "inbox",
            MailboxRole::Sent => "sent",
            MailboxRole::Drafts => "drafts",
            MailboxRole::Trash => "trash",
            MailboxRole::Archive => "archive",
            MailboxRole::Junk => "junk",
            MailboxRole::Other => "other",
        }
    }
}

impl std::str::FromStr for MailboxRole {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "inbox" => MailboxRole::Inbox,
            "sent" => MailboxRole::Sent,
            "drafts" => MailboxRole::Drafts,
            "trash" => MailboxRole::Trash,
            "archive" => MailboxRole::Archive,
            "junk" => MailboxRole::Junk,
            _ => MailboxRole::Other,
        })
    }
}

/// A mailbox (folder) on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mailbox {
    /// App-owned UUID primary key.
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub role: MailboxRole,
    pub total: u32,
    pub unread: u32,
}

/// Envelope: message metadata without the body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// App-owned UUID primary key (never the IMAP UID).
    pub id: String,
    pub mailbox_id: String,
    pub subject: String,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    /// RFC 5322 date, kept as string until the store layer normalizes it.
    pub date: Option<String>,
    /// Server-received timestamp (IMAP INTERNALDATE), in Unix seconds.
    #[serde(default)]
    pub received_at: Option<i64>,
    pub flags: Vec<Flag>,
    pub has_attachment: bool,
    /// RFC822.SIZE in bytes.
    pub size: u32,
    /// Server-side identifier (IMAP UID within the mailbox), attribute only.
    pub server_uid: Option<u32>,
    /// RFC 5322 Message-ID header, attribute only (never a key).
    pub message_id: Option<String>,
    /// Canonical root Message-ID derived from References/In-Reply-To.
    pub thread_id: Option<String>,
    /// IMAP keyword flags (tag names, e.g. "$Forwarded", "Important").
    pub keywords: Vec<String>,
    /// Physical folder/UID references for this logical message.
    #[serde(default)]
    pub sources: Vec<EnvelopeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvelopeSource {
    pub mailbox_id: String,
    pub server_uid: u32,
}

impl Envelope {
    /// Stable identity for the same message copied across provider labels.
    /// Message-ID is the strongest component when present, but it is combined
    /// with stable envelope metadata because broken senders sometimes reuse a
    /// Message-ID for unrelated messages. The metadata fallback is deliberately
    /// limited to fields that survive label copies.
    pub fn logical_id(&self, account_id: &str) -> String {
        let account = account_id.trim().to_ascii_lowercase();
        let metadata = self.metadata_identity();
        if let Some(message_id) = self
            .message_id
            .as_deref()
            .map(normalize_message_id)
            .filter(|value| !value.is_empty())
        {
            return format!("{account}:message-id:{message_id}|{metadata}");
        }

        if self
            .date
            .as_deref()
            .is_some_and(|date| !date.trim().is_empty())
            && self.size > 0
            && !self.from.is_empty()
        {
            return format!("{account}:metadata:{metadata}");
        }

        // Without a Message-ID and without enough stable metadata, merging
        // would be more dangerous than showing a duplicate. The app-owned id
        // is unique for each physical row and therefore gives us a safe stop.
        format!("{account}:physical:{}", self.id)
    }

    fn metadata_identity(&self) -> String {
        let mut from = self
            .from
            .iter()
            .map(|address| address.addr.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();
        let mut to = self
            .to
            .iter()
            .map(|address| address.addr.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();
        from.sort();
        to.sort();
        format!(
            "{}|{}|{}|{}|{}",
            normalize_text(&self.subject),
            from.join(","),
            to.join(","),
            self.date.as_deref().map(normalize_text).unwrap_or_default(),
            self.size,
        )
    }
}

fn normalize_message_id(value: &str) -> String {
    value.trim().trim_matches(['<', '>']).to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub addr: String,
}

/// Normalized IMAP system flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flag {
    Seen,
    Answered,
    Flagged,
    Deleted,
    Draft,
}

/// A user-defined label (IMAP keyword with a colour).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

/// Per-folder synchronization checkpoint (ADR-0003).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    /// UIDVALIDITY of the folder at last sync; a change forces full resync.
    pub uid_validity: u32,
    /// HIGHESTMODSEQ at last sync (CONDSTORE); 0 when unsupported.
    pub highest_modseq: u64,
    /// Highest UID seen so far; new-mail search starts after it.
    pub last_uid: u32,
}

/// An offline operation queued for replay (ADR-0003 §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutboxOp {
    StoreFlags {
        mailbox: String,
        server_uid: u32,
        flags: Vec<Flag>,
        #[serde(default)]
        keywords: Option<Vec<String>>,
    },
    MoveMessages {
        source_mailbox: String,
        destination_mailbox: String,
        server_uids: Vec<u32>,
    },
    DeleteMessages {
        mailbox: String,
        server_uids: Vec<u32>,
    },
    SendMessage {
        raw_base64: String,
    },
    AppendSent {
        raw_base64: String,
    },
}

/// A persisted outbox row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub id: i64,
    pub account_id: String,
    pub op: OutboxOp,
    pub created_at: i64,
    pub attempts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearch {
    pub id: String,
    pub name: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Correspondent {
    pub name: Option<String>,
    pub addr: String,
    pub message_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeywordCount {
    pub name: String,
    pub count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_models_serialize_with_camel_case_fields() {
        let mailbox = Mailbox {
            id: "folder".into(),
            account_id: "account".into(),
            name: "INBOX".into(),
            role: MailboxRole::Inbox,
            total: 1,
            unread: 1,
        };
        let envelope = Envelope {
            id: "message".into(),
            mailbox_id: "folder".into(),
            subject: String::new(),
            from: Vec::new(),
            to: Vec::new(),
            date: None,
            received_at: None,
            flags: Vec::new(),
            has_attachment: false,
            size: 0,
            server_uid: Some(1),
            message_id: None,
            thread_id: None,
            keywords: Vec::new(),
            sources: Vec::new(),
        };

        let mailbox = serde_json::to_value(mailbox).unwrap();
        let envelope = serde_json::to_value(envelope).unwrap();
        assert_eq!(mailbox["accountId"], "account");
        assert!(mailbox.get("account_id").is_none());
        assert_eq!(envelope["serverUid"], 1);
        assert!(envelope.get("server_uid").is_none());
    }
}
