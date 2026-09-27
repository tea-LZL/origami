//! Secret redaction for error strings and persisted diagnostics.

const MASK: &str = "[redacted]";

/// Mask values of well-known secret-shaped tokens (auth headers,
/// `password=`/`PASS` assignments, OAuth JSON fields).
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    for key in ["password=", "pass "] {
        out = mask_after_key(&out, key, false);
    }
    out = mask_after_key(&out, "authorization: ", true);
    for json_key in ["\"oauthAccessToken\"", "\"refresh_token\""] {
        out = mask_json_value(&out, json_key);
    }
    out
}

/// Mask every occurrence of the caller-known secrets (e.g. the account
/// password or token held in config for the operation that failed).
pub fn redact_with(input: &str, secrets: &[&str]) -> String {
    let mut out = input.to_string();
    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        out = out.replace(secret, MASK);
    }
    out
}

/// Mask the run after `key` (ASCII-case-insensitive) up to whitespace, or up
/// to end-of-line when `to_eol`.
fn mask_after_key(input: &str, key: &str, to_eol: bool) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(found) = find_ascii_ignore_case(rest, key) {
        let value_start = found + key.len();
        let value_end = if to_eol {
            rest[value_start..]
                .find(['\r', '\n'])
                .map(|offset| value_start + offset)
                .unwrap_or(rest.len())
        } else {
            rest[value_start..]
                .find(char::is_whitespace)
                .map(|offset| value_start + offset)
                .unwrap_or(rest.len())
        };
        out.push_str(&rest[..value_start]);
        if value_start < value_end {
            out.push_str(MASK);
        }
        rest = &rest[value_end..];
    }
    out.push_str(rest);
    out
}

/// Byte offset of the first `needle` match under ASCII case-insensitivity.
/// Windowed on char boundaries — safe with multi-byte haystacks.
fn find_ascii_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| {
        haystack
            .get(i..i + needle.len())
            .is_some_and(|window| window.eq_ignore_ascii_case(needle))
    })
}

/// Mask the string value of `"key": "…"` JSON (and `key: "…"` loose form).
fn mask_json_value(input: &str, json_key: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(found) = rest.find(json_key) {
        let after_key = found + json_key.len();
        let Some(value_start_rel) = rest[after_key..].find('"') else {
            break;
        };
        let value_start = after_key + value_start_rel + 1;
        let Some(value_end_rel) = rest[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + value_end_rel;
        out.push_str(&rest[..value_start]);
        if value_start < value_end {
            out.push_str(MASK);
        }
        rest = &rest[value_end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_secret() {
        assert_eq!(
            redact_with("token abc123 ok", &["abc123"]),
            "token [redacted] ok"
        );
    }

    #[test]
    fn redacts_auth_headers() {
        let input = "send failed\r\nAuthorization: Bearer eyJhbGciOi.abc\r\ndone";
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("eyJhbGciOi.abc"));
        assert!(redacted.contains("Authorization: [redacted]"));
    }

    #[test]
    fn redacts_json_tokens() {
        let input = r#"{"oauthAccessToken":"secret-1","refresh_token":"secret-2"}"#;
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("secret-1"));
        assert!(!redacted.contains("secret-2"));
    }

    #[test]
    fn redacts_password_assignments() {
        assert!(!redact_secrets("login password=hunter2 retry").contains("hunter2"));
        assert!(!redact_secrets("PASS hunter2").contains("hunter2"));
    }

    #[test]
    fn no_false_positive_on_short_strings() {
        let input = "no secrets here";
        assert_eq!(redact_secrets(input), input);
        assert_eq!(redact_with(input, &[]), input);
    }

    #[test]
    fn redacts_non_ascii_errors_without_panicking() {
        // `İ`.to_lowercase() changes byte length — offsets must not desync.
        let input = "hata: İKİ gün sonra password=hunter2 denendi";
        let redacted = redact_secrets(input);
        assert_eq!(
            redacted, "hata: İKİ gün sonra password=[redacted] denendi",
            "mis-masked: {redacted}"
        );
    }
}
