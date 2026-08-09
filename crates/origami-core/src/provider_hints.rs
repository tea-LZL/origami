//! Common email provider hints: IMAP/SMTP defaults for popular domains.
//! This table powers the manual config autofill in the onboarding wizard.

use crate::config::{AuthMechanism, ImapConfig, SmtpConfig};

/// Provider defaults, keyed by domain (lowercase).
pub struct KnownProvider {
    pub imap: (&'static str, u16),
    pub smtp: (&'static str, u16),
    pub auth: AuthMechanism,
    pub description: &'static str,
}

pub fn for_domain(domain: &str) -> Option<KnownProvider> {
    match domain {
        "gmail.com" | "googlemail.com" | "google.com" => Some(KnownProvider {
            imap: ("imap.gmail.com", 993),
            smtp: ("smtp.gmail.com", 465),
            auth: AuthMechanism::Xoauth2,
            description: "Google / Gmail",
        }),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => Some(KnownProvider {
            imap: ("outlook.office365.com", 993),
            smtp: ("smtp.office365.com", 587),
            auth: AuthMechanism::Xoauth2,
            description: "Microsoft / Outlook",
        }),
        "yahoo.com" | "yahoo.co.uk" | "yahoo.co.jp" => Some(KnownProvider {
            imap: ("imap.mail.yahoo.com", 993),
            smtp: ("smtp.mail.yahoo.com", 465),
            auth: AuthMechanism::Login,
            description: "Yahoo Mail",
        }),
        "icloud.com" | "me.com" | "mac.com" => Some(KnownProvider {
            imap: ("imap.mail.me.com", 993),
            smtp: ("smtp.mail.me.com", 587),
            auth: AuthMechanism::Login,
            description: "Apple iCloud",
        }),
        "fastmail.com" | "fastmail.fm" => Some(KnownProvider {
            imap: ("imap.fastmail.com", 993),
            smtp: ("smtp.fastmail.com", 465),
            auth: AuthMechanism::Login,
            description: "Fastmail",
        }),
        "protonmail.com" | "proton.me" | "pm.me" => Some(KnownProvider {
            imap: ("127.0.0.1", 1143),
            smtp: ("127.0.0.1", 1025),
            auth: AuthMechanism::Login,
            description: "Proton Mail (via Bridge)",
        }),
        _ => None,
    }
}

/// Fill defaults for an IMAP config when the domain is known.
pub fn default_imap(domain: &str) -> Option<ImapConfig> {
    let p = for_domain(domain)?;
    Some(ImapConfig {
        host: p.imap.0.to_string(),
        port: Some(p.imap.1),
        tls: true,
        starttls: false,
        auth: p.auth,
        username: String::new(),
        secret: None,
    })
}

/// Fill defaults for an SMTP config when the domain is known.
pub fn default_smtp(domain: &str) -> Option<SmtpConfig> {
    let p = for_domain(domain)?;
    Some(SmtpConfig {
        host: p.smtp.0.to_string(),
        port: Some(p.smtp.1),
        tls: true,
        starttls: false,
        auth: p.auth,
        username: String::new(),
        secret: None,
    })
}
