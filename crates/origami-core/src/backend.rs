//! The `MailBackend` trait: the seam between Origami and any protocol
//! implementation.
//!
//! The first implementation wraps pimalaya's git-pinned `io-imap` / `io-smtp`
//! crates (see `docs/PLAN.md` §Architecture). All upstream churn is contained
//! behind this trait; JMAP/Gmail backends can be added later without touching
//! the sync engine or UI.

use async_trait::async_trait;

use crate::model::{Envelope, Flag, Mailbox};
use crate::Result;

/// Read-side mail operations every backend must provide.
#[async_trait]
pub trait MailBackend: Send + Sync {
    /// List all mailboxes for the account.
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>>;

    /// List envelopes in a mailbox, newest first.
    async fn list_envelopes(
        &self,
        mailbox: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Envelope>>;

    /// Fetch the full RFC 822 bytes of a message (never sets `\Seen`).
    async fn fetch_message(&self, mailbox: &str, server_uid: u32) -> Result<Vec<u8>>;

    /// Replace the flag set of a message.
    async fn store_flags(&self, mailbox: &str, server_uid: u32, flags: &[Flag]) -> Result<()>;
}

/// Write/send path, kept separate so backends without SMTP (JMAP, Gmail API)
/// can implement sending natively.
#[async_trait]
pub trait SmtpSender: Send + Sync {
    /// Send a fully-built RFC 822 message.
    async fn send_message(&self, raw: &[u8]) -> Result<()>;
}
