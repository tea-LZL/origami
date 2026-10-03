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
        "purelymail.com" => Some(KnownProvider {
            imap: ("imap.purelymail.com", 993),
            smtp: ("smtp.purelymail.com", 465),
            auth: AuthMechanism::Login,
            description: "Purelymail",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purelymail_hints_fill_imap_smtp_login() {
        let p = for_domain("purelymail.com").expect("purelymail.com should be a known provider");
        assert_eq!(p.imap, ("imap.purelymail.com", 993));
        assert_eq!(p.smtp, ("smtp.purelymail.com", 465));
        assert_eq!(p.auth, AuthMechanism::Login);
        assert_eq!(p.description, "Purelymail");

        let imap = default_imap("purelymail.com").expect("imap defaults");
        assert_eq!(imap.host, "imap.purelymail.com");
        assert_eq!(imap.port, Some(993));
        assert!(imap.tls && !imap.starttls);

        let smtp = default_smtp("purelymail.com").expect("smtp defaults");
        assert_eq!(smtp.host, "smtp.purelymail.com");
        assert_eq!(smtp.port, Some(465));
        assert!(smtp.tls && !smtp.starttls);
    }
}
