//! Message body parsing: extract displayable text/HTML and attachment
//! metadata from raw RFC 822 bytes (display sanitization happens in the
//! UI with DOMPurify; remote content blocking is a UI concern too).

use std::collections::HashMap;

use mail_parser::{MessageParser, MimeHeaders};
use serde::{Deserialize, Serialize};

use crate::model::Address;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    /// Position within `Message::attachments()` — used for downloads.
    pub index: usize,
    /// Stable MIME part path, e.g. `1.2`.
    pub part_path: String,
    pub name: Option<String>,
    pub mime: String,
    pub size: usize,
    pub inline: bool,
    /// Content-ID used by `cid:` references in HTML bodies.
    pub cid: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeaders {
    pub cc: Vec<Address>,
    pub reply_to: Vec<Address>,
    pub sender: Vec<Address>,
    pub message_id: Option<String>,
    pub in_reply_to: Vec<String>,
    pub references: Vec<String>,
    pub list_unsubscribe: Vec<String>,
    pub authentication_results: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimePart {
    pub path: String,
    pub mime: String,
    pub charset: Option<String>,
    pub disposition: Option<String>,
    pub filename: Option<String>,
    pub content_id: Option<String>,
    pub content_location: Option<String>,
    pub encoded_size: usize,
    pub decoded_size: usize,
    pub inline: bool,
    pub attachment: bool,
    pub text: bool,
    pub html: bool,
    pub encoding_problem: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub text: Option<String>,
    pub html: Option<String>,
    pub headers: MessageHeaders,
    pub attachments: Vec<AttachmentMeta>,
    pub parts: Vec<MimePart>,
    pub parse_warnings: Vec<String>,
}

/// One MIME section fetched without the enclosing multipart body.
#[derive(Debug, Clone)]
pub struct DisplaySection {
    pub path: String,
    pub mime_headers: Vec<u8>,
    pub body: Vec<u8>,
}

/// Server-derived MIME metadata plus the selected display sections.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub headers: Vec<u8>,
    pub parts: Vec<MimePart>,
    pub attachments: Vec<AttachmentMeta>,
    pub sections: Vec<DisplaySection>,
}

/// Maximum nested MIME multipart depth accepted before parsing is refused.
pub const MAX_MIME_PART_DEPTH: u32 = 32;
/// Maximum size of the raw header block accepted before parsing is refused.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// Maximum decoded attachment payload; oversized decodes are refused.
pub const MAX_ATTACH_DECODE_BYTES: usize = 64 * 1024 * 1024;

/// Parse a raw message into displayable parts. Total: never panics and never
/// allocates unbounded; caps trip `parse_warnings` and return a truncated-safe
/// result instead.
pub fn parse(raw: &[u8]) -> Option<ParsedMessage> {
    if let Some(warning) = mime_limit_warning(raw) {
        return Some(ParsedMessage {
            parse_warnings: vec![warning],
            ..Default::default()
        });
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse_inner(raw)))
        .unwrap_or_else(|_| {
            Some(ParsedMessage {
                parse_warnings: vec!["MIME parsing failed safely".to_string()],
                ..Default::default()
            })
        })
}

fn mime_limit_warning(raw: &[u8]) -> Option<String> {
    if multipart_count(raw) > MAX_MIME_PART_DEPTH {
        return Some(format!(
            "MIME nesting exceeds the depth limit of {MAX_MIME_PART_DEPTH}"
        ));
    }
    if header_block_len(raw) > MAX_HEADER_BYTES {
        return Some(format!(
            "MIME header block exceeds the {MAX_HEADER_BYTES}-byte limit"
        ));
    }
    None
}

fn multipart_count(raw: &[u8]) -> u32 {
    let needle = b"multipart/";
    let mut count = 0u32;
    let mut offset = 0usize;
    while let Some(found) = find_ascii_ignore_case(&raw[offset..], needle) {
        count += 1;
        offset += found + needle.len();
        if count > MAX_MIME_PART_DEPTH || offset >= raw.len() {
            break;
        }
    }
    count
}

fn header_block_len(raw: &[u8]) -> usize {
    let crlf = find_bytes(raw, b"\r\n\r\n");
    let lf = find_bytes(raw, b"\n\n");
    match (crlf, lf) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => raw.len(),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

fn find_ascii_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| {
        haystack[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn parse_inner(raw: &[u8]) -> Option<ParsedMessage> {
    let message = MessageParser::default().parse(raw)?;
    let text = message.body_text(0).map(|c| c.into_owned());
    let html = message.body_html(0).map(|c| c.into_owned());
    let mut parts = Vec::new();
    let mut part_paths = HashMap::new();
    let root_path = if message
        .parts
        .first()
        .is_some_and(|part| part.sub_parts().is_some())
    {
        ""
    } else {
        "1"
    };
    collect_parts(&message, 0, root_path, &mut parts, &mut part_paths);

    let attachments = message
        .attachments
        .iter()
        .enumerate()
        .filter_map(|(index, part_id)| {
            let part = message.parts.get(*part_id as usize)?;
            let path = part_paths
                .get(part_id)
                .cloned()
                .unwrap_or_else(|| format!("part-{part_id}"));
            Some(AttachmentMeta {
                index,
                part_path: path,
                name: part.attachment_name().map(str::to_string),
                mime: mime_type(part),
                size: part.len(),
                inline: part
                    .content_disposition()
                    .is_some_and(|disposition| disposition.is_inline()),
                cid: part
                    .content_id()
                    .map(|cid| cid.trim_matches(['<', '>']).to_string()),
            })
        })
        .collect();
    let parse_warnings = parts
        .iter()
        .filter(|part| part.encoding_problem)
        .map(|part| format!("MIME decoding problem in part {}", part.path))
        .collect();
    Some(ParsedMessage {
        text,
        html,
        headers: message_headers(&message),
        attachments,
        parts,
        parse_warnings,
    })
}

/// Parse top-level headers and independently fetched display MIME sections.
/// This avoids requiring multipart boundaries or attachment bytes locally.
pub fn parse_display(display: &DisplayMessage) -> Option<ParsedMessage> {
    let parser = MessageParser::default();
    let headers = parser.parse_headers(&display.headers)?;
    let mut text = None;
    let mut html = None;
    let mut html_from_plain_text = None;
    let mut parse_warnings = Vec::new();

    for section in &display.sections {
        let mut raw = section.mime_headers.clone();
        if !raw.ends_with(b"\r\n\r\n") && !raw.ends_with(b"\n\n") {
            raw.extend_from_slice(b"\r\n");
        }
        raw.extend_from_slice(&section.body);
        let Some(message) = parser.parse(&raw) else {
            parse_warnings.push(format!("Could not parse MIME section {}", section.path));
            continue;
        };
        let part = display.parts.iter().find(|part| part.path == section.path);
        if part.is_some_and(|part| part.html) {
            // Do not let the preceding text/plain alternative populate the
            // HTML field with mail-parser's text-to-HTML fallback. The real
            // HTML alternative must win regardless of MIME section order.
            if html.is_none() {
                html = message.body_html(0).map(|value| value.into_owned());
            }
            if text.is_none() {
                text = message.body_text(0).map(|value| value.into_owned());
            }
        } else if part.is_some_and(|part| part.text) {
            if text.is_none() {
                text = message.body_text(0).map(|value| value.into_owned());
            }
            if html_from_plain_text.is_none() {
                html_from_plain_text = message.body_html(0).map(|value| value.into_owned());
            }
        } else {
            // Keep malformed or incomplete BODYSTRUCTURE responses usable as
            // text, but never treat an unknown section as authoritative HTML.
            if text.is_none() {
                text = message.body_text(0).map(|value| value.into_owned());
            }
        }
        if message.parts.iter().any(|part| part.is_encoding_problem) {
            parse_warnings.push(format!("MIME decoding problem in part {}", section.path));
        }
    }
    if html.is_none() {
        html = html_from_plain_text;
    }

    Some(ParsedMessage {
        text,
        html,
        headers: message_headers(&headers),
        attachments: display.attachments.clone(),
        parts: display.parts.clone(),
        parse_warnings,
    })
}

/// Decode one independently fetched MIME part using its MIME headers.
pub fn decode_mime_part(mime_headers: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    if body.len() > MAX_ATTACH_DECODE_BYTES {
        return None;
    }
    let mut raw = mime_headers.to_vec();
    if !raw.ends_with(b"\r\n\r\n") && !raw.ends_with(b"\n\n") {
        raw.extend_from_slice(b"\r\n");
    }
    raw.extend_from_slice(body);
    let message = MessageParser::default().parse(&raw)?;
    let contents = message.parts.first()?.contents().to_vec();
    if contents.len() > MAX_ATTACH_DECODE_BYTES {
        return None;
    }
    Some(contents)
}

/// Extract the decoded bytes of the attachment at `index`.
pub fn attachment_bytes(raw: &[u8], index: usize) -> Option<Vec<u8>> {
    let message = MessageParser::default().parse(raw)?;
    let part_id = *message.attachments.get(index)?;
    let contents = message.parts.get(part_id as usize)?.contents().to_vec();
    if contents.len() > MAX_ATTACH_DECODE_BYTES {
        return None;
    }
    Some(contents)
}

/// Text suitable for local indexing without parsing the raw message again.
pub fn body_text_for_index(parsed: &ParsedMessage) -> String {
    parsed
        .text
        .clone()
        .or_else(|| parsed.html.as_deref().map(strip_html_tags))
        .unwrap_or_default()
}

fn message_headers(message: &mail_parser::Message<'_>) -> MessageHeaders {
    MessageHeaders {
        cc: address_list(message.cc()),
        reply_to: address_list(message.reply_to()),
        sender: address_list(message.sender()),
        message_id: message.message_id().map(normalize_header_id),
        in_reply_to: header_ids(message.in_reply_to()),
        references: header_ids(message.references()),
        list_unsubscribe: header_texts(message.list_unsubscribe()),
        authentication_results: message
            .header("Authentication-Results")
            .and_then(|header| header.as_text())
            .map(str::to_string),
    }
}

fn address_list(addresses: Option<&mail_parser::Address<'_>>) -> Vec<Address> {
    addresses
        .into_iter()
        .flat_map(|addresses| addresses.iter())
        .filter_map(|address| {
            Some(Address {
                name: address.name().map(str::to_string),
                addr: address.address()?.to_string(),
            })
        })
        .collect()
}

fn header_ids(value: &mail_parser::HeaderValue<'_>) -> Vec<String> {
    value
        .as_text_list()
        .into_iter()
        .flatten()
        .map(|value| normalize_header_id(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn header_texts(value: &mail_parser::HeaderValue<'_>) -> Vec<String> {
    value
        .as_text_list()
        .into_iter()
        .flatten()
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn normalize_header_id(value: &str) -> String {
    value.trim().trim_matches(['<', '>']).to_string()
}

fn collect_parts(
    message: &mail_parser::Message<'_>,
    part_id: u32,
    path: &str,
    parts: &mut Vec<MimePart>,
    paths: &mut HashMap<u32, String>,
) {
    let Some(part) = message.parts.get(part_id as usize) else {
        return;
    };
    paths.insert(part_id, path.to_string());
    let disposition = part
        .content_disposition()
        .map(|value| value.ctype().to_string());
    let filename = part.attachment_name().map(str::to_string);
    let mime = mime_type(part);
    let is_text = part.is_text();
    let is_html = part.is_text_html();
    let attachment = disposition
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("attachment"))
        || filename.is_some();
    parts.push(MimePart {
        path: path.to_string(),
        mime,
        charset: part
            .content_type()
            .and_then(|value| value.attribute("charset"))
            .map(str::to_string),
        disposition,
        filename,
        content_id: part
            .content_id()
            .map(|value| value.trim_matches(['<', '>']).to_string()),
        content_location: part.content_location().map(str::to_string),
        encoded_size: part.raw_len() as usize,
        decoded_size: part.len(),
        inline: part
            .content_disposition()
            .is_some_and(|value| value.is_inline()),
        attachment,
        text: is_text,
        html: is_html,
        encoding_problem: part.is_encoding_problem,
    });

    if let Some(children) = part.sub_parts() {
        for (index, child_id) in children.iter().enumerate() {
            let child_path = if path.is_empty() {
                (index + 1).to_string()
            } else {
                format!("{path}.{}", index + 1)
            };
            collect_parts(message, *child_id, &child_path, parts, paths);
        }
    }
}

fn mime_type<'a>(part: &impl MimeHeaders<'a>) -> String {
    part.content_type()
        .map(|content_type| {
            format!(
                "{}/{}",
                content_type.ctype(),
                content_type.subtype().unwrap_or("octet-stream")
            )
        })
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn strip_html_tags(html: &str) -> String {
    mail_parser::decoders::html::html_to_text(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "From: A <a@example.org>\r\nTo: B <b@example.net>\r\nSubject: mixed\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: multipart/alternative; boundary=y\r\n\r\n--y\r\nContent-Type: text/plain\r\n\r\nplain body\r\n--y\r\nContent-Type: text/html\r\n\r\n<p>html body</p>\r\n--y--\r\n--x\r\nContent-Type: application/pdf; name=doc.pdf\r\nContent-Disposition: attachment; filename=doc.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--x--\r\n";

    #[test]
    fn parses_bodies_and_attachments() {
        let parsed = parse(RAW.as_bytes()).unwrap();
        assert_eq!(parsed.text.as_deref(), Some("plain body"));
        assert!(parsed.html.unwrap().contains("html body"));
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].name.as_deref(), Some("doc.pdf"));
        assert_eq!(parsed.attachments[0].part_path, "2");
        assert_eq!(parsed.attachments[0].mime, "application/pdf");
        assert_eq!(parsed.attachments[0].size, 5); // "hello" decoded
        assert_eq!(parsed.parts.len(), 5);
    }

    #[test]
    fn attachment_bytes_are_decoded() {
        let bytes = attachment_bytes(RAW.as_bytes(), 0).unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn extracts_display_headers_and_mime_paths() {
        let raw = concat!(
            "From: Alice <alice@example.org>\r\n",
            "To: Bob <bob@example.net>\r\n",
            "Cc: Carol <carol@example.net>\r\n",
            "Reply-To: replies@example.org\r\n",
            "Message-ID: <message@example.org>\r\n",
            "References: <root@example.org> <parent@example.org>\r\n",
            "List-Unsubscribe: <mailto:unsubscribe@example.org>\r\n",
            "\r\n",
            "hello\r\n",
        );
        let parsed = parse(raw.as_bytes()).unwrap();
        assert_eq!(parsed.headers.cc[0].addr, "carol@example.net");
        assert_eq!(parsed.headers.reply_to[0].addr, "replies@example.org");
        assert_eq!(
            parsed.headers.message_id.as_deref(),
            Some("message@example.org")
        );
        assert_eq!(
            parsed.headers.references,
            ["root@example.org", "parent@example.org"]
        );
        assert_eq!(parsed.attachments, Vec::<AttachmentMeta>::new());
        assert_eq!(body_text_for_index(&parsed).trim(), "hello");
    }

    #[test]
    fn parses_independent_display_sections() {
        let display = DisplayMessage {
            headers: b"From: Alice <alice@example.org>\r\nSubject: display\r\n\r\n".to_vec(),
            parts: vec![MimePart {
                path: "1".to_string(),
                mime: "text/plain".to_string(),
                text: true,
                ..MimePart::default()
            }],
            attachments: Vec::new(),
            sections: vec![DisplaySection {
                path: "1".to_string(),
                mime_headers: b"Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n".to_vec(),
                body: b"aGVsbG8gZGlzcGxheQ==\r\n".to_vec(),
            }],
        };

        let parsed = parse_display(&display).unwrap();
        assert_eq!(parsed.text.as_deref(), Some("hello display"));
        assert_eq!(parsed.headers.message_id, None);
        assert_eq!(body_text_for_index(&parsed), "hello display");
    }

    #[test]
    fn prefers_html_alternative_over_plain_text_fallback() {
        let display = DisplayMessage {
            headers: b"From: Alice <alice@example.org>\r\nSubject: alternatives\r\n\r\n".to_vec(),
            parts: vec![
                MimePart {
                    path: "1".to_string(),
                    mime: "text/plain".to_string(),
                    text: true,
                    ..MimePart::default()
                },
                MimePart {
                    path: "2".to_string(),
                    mime: "text/html".to_string(),
                    text: true,
                    html: true,
                    ..MimePart::default()
                },
            ],
            attachments: Vec::new(),
            sections: vec![
                DisplaySection {
                    path: "1".to_string(),
                    mime_headers: b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec(),
                    body: b"plain alternative".to_vec(),
                },
                DisplaySection {
                    path: "2".to_string(),
                    mime_headers: b"Content-Type: text/html; charset=utf-8\r\n\r\n".to_vec(),
                    body: b"<html><head><style>table { color: red; }</style></head><body><table><tr><td>styled newsletter</td></tr></table></body></html>".to_vec(),
                },
            ],
        };

        let parsed = parse_display(&display).unwrap();
        assert_eq!(parsed.text.as_deref(), Some("plain alternative"));
        let html = parsed.html.as_deref().unwrap();
        assert!(html.contains("<table>"));
        assert!(html.contains("styled newsletter"));
        assert!(html.contains("table { color: red; }"));
        assert!(!html.contains("plain alternative"));
    }

    #[test]
    fn decodes_an_independent_mime_part() {
        let bytes = decode_mime_part(
            b"Content-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\n\r\n",
            b"aGVsbG8=\r\n",
        )
        .unwrap();
        assert_eq!(bytes, b"hello");
    }
}
