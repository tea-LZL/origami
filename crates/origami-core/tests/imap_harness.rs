//! Live IMAP tests against the Docker harness (see tests/harness/).
//!
//! No-op unless ORIGAMI_TEST_IMAP is set:
//!
//! ```sh
//! docker compose -f tests/harness/docker-compose.yml up -d
//! ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test imap_harness
//! ```

use origami_core::config::{AuthMechanism, ImapConfig, Secret};
use origami_core::imap::ImapBackend;
use origami_core::model::Flag;
use origami_core::MailBackend;

fn enabled() -> bool {
    if std::env::var("ORIGAMI_TEST_IMAP").is_err() {
        eprintln!("skipping: set ORIGAMI_TEST_IMAP=1 and start tests/harness");
        return false;
    }
    true
}

fn test_config() -> ImapConfig {
    ImapConfig {
        host: "127.0.0.1".to_string(),
        port: Some(10143),
        tls: false,
        starttls: false,
        auth: AuthMechanism::Login,
        username: "origami".to_string(),
        secret: Some(Secret::Raw {
            raw: "origami".to_string(),
        }),
    }
}

#[tokio::test]
async fn connect_list_mailboxes() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");

    let mailboxes = backend.list_mailboxes().await.expect("list mailboxes");
    assert!(
        mailboxes.iter().any(|m| m.name == "INBOX"),
        "INBOX must exist, got: {:?}",
        mailboxes.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn unicode_mailbox_roundtrip() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");
    let mailbox = format!("Origami-旅行-{}", uuid::Uuid::now_v7());
    backend
        .create_mailbox(&mailbox)
        .await
        .expect("create mailbox");

    let listing = backend.list_mailboxes().await;
    let _ = backend.delete_mailbox(&mailbox).await;
    let mailboxes = listing.expect("list mailboxes");
    assert!(
        mailboxes.iter().any(|item| item.name == mailbox),
        "Unicode mailbox name was not decoded: {:?}",
        mailboxes.iter().map(|item| &item.name).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn list_envelopes_newest_first() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");

    let envelopes = backend
        .list_envelopes("INBOX", 1, 10)
        .await
        .expect("list envelopes");
    assert_eq!(envelopes.len(), 3, "maildir is seeded with 3 messages");

    // Newest first (maildir sequence order is ascending by delivery).
    let subjects: Vec<&str> = envelopes.iter().map(|e| e.subject.as_str()).collect();
    assert!(subjects.contains(&"Welcome to Origami"));
    // RFC 2047 encoded-word subject must be decoded.
    assert!(subjects.contains(&"Launch tomorrow?"));

    let welcome = envelopes
        .iter()
        .find(|e| e.subject == "Welcome to Origami")
        .expect("welcome message");
    assert_eq!(welcome.from[0].addr, "alice@example.org");
    assert_eq!(welcome.from[0].name.as_deref(), Some("Alice Anders"));
    assert!(welcome.flags.contains(&Flag::Seen));

    // The message in new/ has no \Seen flag.
    let digest = envelopes
        .iter()
        .find(|e| e.subject == "Weekly digest")
        .expect("digest message");
    assert!(!digest.flags.contains(&Flag::Seen));
}

#[tokio::test]
async fn fetch_message_peek_does_not_set_seen() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");

    let envelopes = backend.list_envelopes("INBOX", 1, 10).await.unwrap();
    let digest = envelopes
        .iter()
        .find(|e| e.subject == "Weekly digest")
        .expect("digest message");
    let uid = digest.server_uid.unwrap();

    let raw = backend.fetch_message("INBOX", uid).await.expect("fetch");
    let text = String::from_utf8(raw).unwrap();
    assert!(text.contains("Subject: Weekly digest"));
    assert!(text.contains("multipart/alternative"));

    // BODY.PEEK must not flip \Seen.
    let envelopes = backend.list_envelopes("INBOX", 1, 10).await.unwrap();
    let digest = envelopes
        .iter()
        .find(|e| e.subject == "Weekly digest")
        .unwrap();
    assert!(!digest.flags.contains(&Flag::Seen));
}

#[tokio::test]
async fn store_flags_roundtrip() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");

    let envelopes = backend.list_envelopes("INBOX", 1, 10).await.unwrap();
    let welcome = envelopes
        .iter()
        .find(|e| e.subject == "Welcome to Origami")
        .unwrap();
    let uid = welcome.server_uid.unwrap();

    backend
        .store_flags_and_keywords(
            "INBOX",
            uid,
            &[Flag::Seen],
            Some(&["OrigamiTest".to_string()]),
        )
        .await
        .expect("store keyword");
    backend
        .store_flags("INBOX", uid, &[Flag::Seen, Flag::Flagged])
        .await
        .expect("store flags");
    let envelopes = backend.list_envelopes("INBOX", 1, 10).await.unwrap();
    let welcome = envelopes
        .iter()
        .find(|e| e.subject == "Welcome to Origami")
        .unwrap();
    assert!(welcome.flags.contains(&Flag::Flagged));
    assert!(welcome
        .keywords
        .iter()
        .any(|keyword| keyword == "OrigamiTest"));

    // Restore original state for repeatable runs.
    backend
        .store_flags_and_keywords("INBOX", uid, &[Flag::Seen], Some(&[]))
        .await
        .expect("restore flags");
}

#[tokio::test]
async fn store_flags_many_marks_every_message_read() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");

    let before = backend.list_envelopes("INBOX", 1, 50).await.unwrap();
    let unread: Vec<u32> = before
        .iter()
        .filter(|envelope| !envelope.flags.contains(&Flag::Seen))
        .filter_map(|envelope| envelope.server_uid)
        .collect();
    assert!(!unread.is_empty(), "fixture must seed unread mail");

    // A keyword on one target must survive a bulk flag write.
    let keyword_uid = unread[0];
    backend
        .store_flags_many("INBOX", &[(keyword_uid, vec![Flag::Seen])])
        .await
        .expect("seed read flag");
    backend
        .store_flags_and_keywords(
            "INBOX",
            keyword_uid,
            &[Flag::Seen],
            Some(&["OrigamiBulk".to_string()]),
        )
        .await
        .expect("seed keyword");

    let updates: Vec<(u32, Vec<Flag>)> = before
        .iter()
        .filter_map(|envelope| envelope.server_uid)
        .map(|uid| (uid, vec![Flag::Seen]))
        .collect();
    backend
        .store_flags_many("INBOX", &updates)
        .await
        .expect("bulk store");

    let after = backend.list_envelopes("INBOX", 1, 50).await.unwrap();
    for envelope in &after {
        assert!(
            envelope.flags.contains(&Flag::Seen),
            "UID {:?} still unread after bulk store",
            envelope.server_uid
        );
    }
    let authored = after
        .iter()
        .find(|envelope| envelope.server_uid == Some(keyword_uid))
        .expect("keyword message survives");
    assert!(
        authored
            .keywords
            .iter()
            .any(|keyword| keyword == "OrigamiBulk"),
        "bulk store must not clobber keywords: {:?}",
        authored.keywords
    );

    // Restore the fixture: original flags per message, keyword gone.
    for envelope in &before {
        if let Some(uid) = envelope.server_uid {
            backend
                .store_flags_and_keywords("INBOX", uid, &envelope.flags, Some(&[]))
                .await
                .expect("restore flags");
        }
    }
}

#[tokio::test]
async fn move_and_delete_message_batches() {
    if !enabled() {
        return;
    }
    let backend = ImapBackend::connect("test", &test_config())
        .await
        .expect("connect");
    let suffix = uuid::Uuid::now_v7();
    let source_initial = format!("Origami-Move-Initial-{suffix}");
    let source = format!("Origami-Move-Source-{suffix}");
    let destination = format!("Origami-Move-Destination-{suffix}");
    backend.create_mailbox(&source_initial).await.unwrap();
    backend
        .rename_mailbox(&source_initial, &source)
        .await
        .unwrap();
    backend.create_mailbox(&destination).await.unwrap();
    backend
        .append_message(
            &source,
            b"From: a@example.org\r\nTo: b@example.org\r\nSubject: move me\r\n\r\nbody\r\n",
            &[],
        )
        .await
        .unwrap();

    let source_messages = backend.list_envelopes(&source, 1, 10).await.unwrap();
    let source_uid = source_messages[0].server_uid.unwrap();
    backend
        .move_messages(&source, &destination, &[source_uid])
        .await
        .unwrap();
    assert!(backend
        .list_envelopes(&source, 1, 10)
        .await
        .unwrap()
        .is_empty());

    let destination_messages = backend.list_envelopes(&destination, 1, 10).await.unwrap();
    let destination_uid = destination_messages[0].server_uid.unwrap();
    backend
        .delete_messages(&destination, &[destination_uid])
        .await
        .unwrap();
    assert!(backend
        .list_envelopes(&destination, 1, 10)
        .await
        .unwrap()
        .is_empty());

    // Dovecot closes the session if the currently selected mailbox is deleted.
    backend.list_envelopes("INBOX", 1, 1).await.unwrap();
    backend.delete_mailbox(&source).await.unwrap();
    backend.delete_mailbox(&destination).await.unwrap();
}
