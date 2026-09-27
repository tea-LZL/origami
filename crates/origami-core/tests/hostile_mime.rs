//! Hostile-MIME corpus: parsing must be total — never panic, never allocate
//! unbounded — under malformed, oversized, or adversarial inputs. Caps live
//! in `origami_core::message` (MAX_MIME_PART_DEPTH, MAX_HEADER_BYTES,
//! MAX_ATTACH_DECODE_BYTES). Fixtures are built in-code so the corpus stays
//! reviewable and the repo stays blob-free.

use origami_core::message::{attachment_bytes, decode_mime_part, parse, MAX_MIME_PART_DEPTH};

fn headers(subject: &str) -> Vec<u8> {
    format!("From: a@example.org\r\nTo: b@example.org\r\nSubject: {subject}\r\n").into_bytes()
}

fn build_deep_multiparts(depth: usize) -> Vec<u8> {
    let mut raw = headers("deep");
    for level in 0..depth {
        raw.extend_from_slice(
            format!("Content-Type: multipart/mixed; boundary=b{level}\r\n\r\n--b{level}\r\n")
                .as_bytes(),
        );
    }
    raw.extend_from_slice(b"Content-Type: text/plain\r\n\r\ndeep body");
    raw
}

fn build_huge_headers(size: usize) -> Vec<u8> {
    let mut raw = headers("huge");
    raw.extend_from_slice(b"X-Pad: ");
    raw.extend(std::iter::repeat(b'A').take(size));
    raw.extend_from_slice(b"\r\n\r\nbody");
    raw
}

fn build_header_injection() -> Vec<u8> {
    let mut raw = b"From: friend@example.org\r\n".to_vec();
    raw.extend_from_slice(b"Subject: hi\r\nBcc: evil@example.org\r\nX-Injected: yes\r\n");
    raw.extend_from_slice(b"Reply-To: attacker@example.org\r\n\r\nbody");
    raw
}

fn build_cid_bomb(refs: usize) -> Vec<u8> {
    let mut raw = headers("cid bomb");
    raw.extend_from_slice(b"Content-Type: text/html; charset=utf-8\r\n\r\n<html><body>");
    for i in 0..refs {
        raw.extend_from_slice(format!(r#"<img src="cid:ref{i}">"#).as_bytes());
    }
    raw.extend_from_slice(b"</body></html>");
    raw
}

fn build_malformed_base64() -> Vec<u8> {
    let mut raw = headers("bad64");
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=bb\r\n\r\n--bb\r\n",
    );
    raw.extend_from_slice(
        b"Content-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\n\r\n",
    );
    raw.extend_from_slice(b"=xx!!not base64!!\r\n--bb--\r\n");
    raw
}

fn build_attachment_size_lie() -> Vec<u8> {
    let mut raw = headers("liar");
    raw.extend_from_slice(b"Content-Length: 4294967295\r\n");
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=cc\r\n\r\n--cc\r\n",
    );
    raw.extend_from_slice(
        b"Content-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"big.bin\"\r\n\r\n",
    );
    raw.extend_from_slice(b"tiny\r\n--cc--\r\n");
    raw
}

#[test]
fn deep_multiparts() {
    let raw = build_deep_multiparts(1000);
    let parsed = parse(&raw).expect("truncated-safe result");
    let warnings = parsed.parse_warnings.join(" ");
    assert!(
        warnings.to_lowercase().contains("depth") || warnings.to_lowercase().contains("nest"),
        "depth cap must trip a warning: {warnings:?}"
    );
}

#[test]
fn huge_headers() {
    let raw = build_huge_headers(1024 * 1024);
    let parsed = parse(&raw).expect("truncated-safe result");
    let warnings = parsed.parse_warnings.join(" ");
    assert!(
        warnings.to_lowercase().contains("header") || warnings.to_lowercase().contains("size"),
        "header cap must trip a warning: {warnings:?}"
    );
}

#[test]
fn header_injection() {
    let parsed = parse(&build_header_injection()).expect("parse returns");
    // Totality is the contract; the sanitizer layer owns display-time safety.
    assert!(parsed.text.is_some() || parsed.html.is_some() || !parsed.parse_warnings.is_empty());
}

#[test]
fn cid_bomb() {
    let raw = build_cid_bomb(10_000);
    let parsed = parse(&raw).expect("parse returns");
    // Output must stay proportional to input, not to ref count squared.
    let html_len = parsed.html.as_deref().map(str::len).unwrap_or(0);
    assert!(html_len <= raw.len() * 2, "html blew up: {html_len} vs {}", raw.len());
}

#[test]
fn malformed_base64() {
    let raw = build_malformed_base64();
    let parsed = parse(&raw).expect("parse returns");
    // Decoding problems warn, never panic.
    let _ = parsed.parse_warnings;
}

#[test]
fn attachment_size_lie() {
    let raw = build_attachment_size_lie();
    let parsed = parse(&raw).expect("parse returns");
    let text_len = parsed.text.as_deref().map(str::len).unwrap_or(0);
    assert!(text_len <= raw.len() * 2, "declared size trusted: {text_len}");
}

#[test]
fn attachment_decode_respects_byte_cap() {
    // Oversized decode is refused: body claims more than the cap allows.
    let huge_body = vec![b'a'; origami_core::message::MAX_ATTACH_DECODE_BYTES + 1];
    let decoded = decode_mime_part(
        b"Content-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\n\r\n",
        &huge_body,
    );
    assert!(decoded.is_none(), "decode over cap must be refused");
    assert!(attachment_bytes(&huge_body, 0).is_none());
}

#[test]
fn parse_is_total() {
    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        ("deep_multiparts", build_deep_multiparts(1000)),
        ("huge_headers", build_huge_headers(1024 * 1024)),
        ("header_injection", build_header_injection()),
        ("cid_bomb", build_cid_bomb(10_000)),
        ("malformed_base64", build_malformed_base64()),
        ("attachment_size_lie", build_attachment_size_lie()),
    ];
    for (name, raw) in &fixtures {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(raw)));
        assert!(result.is_ok(), "{name} panicked");
        if let Ok(Some(parsed)) = result {
            let output = parsed
                .text
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
                .saturating_add(parsed.html.as_deref().map(str::len).unwrap_or(0));
            assert!(
                output <= raw.len() * 2 + 4096,
                "{name} output unbounded: {output} vs {}",
                raw.len()
            );
        }
    }
    // Cap constant sanity.
    assert_eq!(MAX_MIME_PART_DEPTH, 32);
}
