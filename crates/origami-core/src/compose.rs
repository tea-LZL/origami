//! Message composition: build RFC 822 from UI drafts via mail-builder.
//!
//! Rich HTML from the TipTap composer becomes a multipart/alternative
//! message with a derived plain-text part.

use base64::Engine;
use serde::Deserialize;

use crate::{Error, Result};

/// A draft coming from the composer UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub from_name: Option<String>,
    pub from_addr: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    /// Rich HTML body from TipTap.
    pub html: String,
    /// Optional explicit plain-text alternative; derived from `html`
    /// when absent.
    pub text: Option<String>,
    /// RFC 5322 threading headers for replies.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<DraftAttachment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachment {
    pub name: String,
    pub mime: String,
    pub data_base64: String,
}

/// Build a raw RFC 822 message from a draft.
pub fn build_message(draft: &Draft) -> Result<Vec<u8>> {
    build_message_inner(draft, true)
}

/// Build a Drafts-mailbox message; incomplete drafts may omit recipients.
pub fn build_draft_message(draft: &Draft) -> Result<Vec<u8>> {
    build_message_inner(draft, false)
}

fn build_message_inner(draft: &Draft, require_recipients: bool) -> Result<Vec<u8>> {
    if require_recipients && draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
        return Err(Error::Backend("draft has no recipients".into()));
    }
    let text = draft
        .text
        .clone()
        .unwrap_or_else(|| html_to_text(&draft.html));

    let mut builder = mail_builder::MessageBuilder::new();
    builder = match &draft.from_name {
        Some(name) => builder.from((name.as_str(), draft.from_addr.as_str())),
        None => builder.from(draft.from_addr.as_str()),
    };
    if !draft.to.is_empty() {
        builder = builder.to(draft.to.join(", "));
    }
    if !draft.cc.is_empty() {
        builder = builder.cc(draft.cc.join(", "));
    }
    if !draft.bcc.is_empty() {
        builder = builder.bcc(draft.bcc.join(", "));
    }
    if let Some(in_reply_to) = &draft.in_reply_to {
        if !in_reply_to.trim().is_empty() {
            builder = builder.in_reply_to(in_reply_to.clone());
        }
    }
    if !draft.references.is_empty() {
        builder = builder.references(draft.references.clone());
    }
    builder = builder.subject(&draft.subject);
    builder = builder.text_body(text);
    if !draft.html.trim().is_empty() {
        builder = builder.html_body(&draft.html);
    }
    for attachment in &draft.attachments {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&attachment.data_base64)
            .map_err(|error| Error::Backend(format!("invalid attachment data: {error}")))?;
        builder = builder.attachment(attachment.mime.as_str(), attachment.name.as_str(), bytes);
    }

    builder
        .write_to_vec()
        .map_err(|e| Error::Backend(format!("cannot build message: {e}")))
}

/// Derive a plain-text alternative from composer HTML (naive tag strip;
/// block elements become line breaks).
fn html_to_text(html: &str) -> String {
    mail_parser::decoders::html::html_to_text(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_text_only_when_html_empty() {
        let draft = Draft {
            from_name: None,
            from_addr: "me@example.org".into(),
            to: vec!["you@example.org".into()],
            cc: vec![],
            bcc: vec![],
            subject: "plain".into(),
            html: String::new(),
            text: Some("just text".into()),
            in_reply_to: None,
            references: vec![],
            attachments: vec![],
        };

        let raw = String::from_utf8(build_message(&draft).unwrap()).unwrap();

        assert!(raw.contains("just text"));
        assert!(!raw.contains("multipart/alternative"));
        assert!(!raw.to_lowercase().contains("text/html"));
    }

    #[test]
    fn builds_multipart_alternative() {
        let draft = Draft {
            from_name: Some("Alice".to_string()),
            from_addr: "alice@example.org".to_string(),
            to: vec!["bob@example.net".to_string()],
            cc: vec![],
            bcc: vec![],
            subject: "Hello".to_string(),
            html: "<p>Hi <b>there</b></p>".to_string(),
            text: None,
            in_reply_to: None,
            references: vec![],
            attachments: vec![DraftAttachment {
                name: "note.txt".to_string(),
                mime: "text/plain".to_string(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(b"attachment"),
            }],
        };
        let raw = build_message(&draft).unwrap();
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        assert_eq!(
            parsed
                .from()
                .and_then(|address| address.first())
                .and_then(|address| address.address.as_deref()),
            Some("alice@example.org")
        );
        assert_eq!(
            parsed
                .to()
                .and_then(|address| address.first())
                .and_then(|address| address.address.as_deref()),
            Some("bob@example.net")
        );
        assert_eq!(parsed.subject(), Some("Hello"));
        assert_eq!(parsed.attachments().count(), 1);
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("multipart/alternative"));
    }

    #[test]
    fn rejects_empty_recipients() {
        let draft = Draft {
            from_name: None,
            from_addr: "a@example.org".to_string(),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: String::new(),
            html: String::new(),
            text: None,
            in_reply_to: None,
            references: vec![],
            attachments: vec![],
        };
        assert!(build_message(&draft).is_err());
    }

    #[test]
    fn preserves_reply_thread_headers() {
        let draft = Draft {
            from_name: None,
            from_addr: "a@example.org".to_string(),
            to: vec!["b@example.org".to_string()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: hello".to_string(),
            html: "<p>reply</p>".to_string(),
            text: None,
            in_reply_to: Some("parent@example.org".to_string()),
            references: vec![
                "root@example.org".to_string(),
                "parent@example.org".to_string(),
            ],
            attachments: vec![],
        };
        let raw = String::from_utf8(build_message(&draft).unwrap()).unwrap();
        assert!(raw.contains("In-Reply-To: <parent@example.org>"));
        assert!(raw.contains("References: <root@example.org> <parent@example.org>"));
    }
}
