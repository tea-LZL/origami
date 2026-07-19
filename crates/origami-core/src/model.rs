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

/// A mailbox (folder) on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct Envelope {
    /// App-owned UUID primary key (never the IMAP UID).
    pub id: String,
    pub mailbox_id: String,
    pub subject: String,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    /// RFC 5322 date, kept as string until the store layer normalizes it.
    pub date: Option<String>,
    pub flags: Vec<Flag>,
    pub has_attachment: bool,
    /// RFC822.SIZE in bytes.
    pub size: u32,
    /// Server-side identifier (IMAP UID within the mailbox), attribute only.
    pub server_uid: Option<u32>,
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
