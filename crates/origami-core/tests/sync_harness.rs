//! Live sync-engine tests against the Docker harness (see tests/harness/).
//!
//! No-op unless ORIGAMI_TEST_IMAP is set:
//!
//! ```sh
//! docker compose -f tests/harness/docker-compose.yml up -d
//! ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test sync_harness
//! ```

use origami_core::blob::BlobStore;
use origami_core::config::{AccountConfig, AuthMechanism, ImapConfig, Secret};
use origami_core::imap::ImapBackend;
use origami_core::model::{Flag, MailboxRole};
use origami_core::store::Store;
use origami_core::sync::{SyncEngine, SyncEvent};
use origami_core::MailBackend;

fn enabled() -> bool {
    if std::env::var("ORIGAMI_TEST_IMAP").is_err() {
        eprintln!("skipping: set ORIGAMI_TEST_IMAP=1 and start tests/harness");
        return false;
    }
    true
}

fn test_account() -> AccountConfig {
    AccountConfig {
        name: "Harness".to_string(),
        email: "origami@localhost".to_string(),
        default: true,
        imap: Some(ImapConfig {
            host: "127.0.0.1".to_string(),
            port: Some(10143),
            tls: false,
            starttls: false,
            auth: AuthMechanism::Login,
            username: "origami".to_string(),
            secret: Some(Secret::Raw {
                raw: "origami".to_string(),
            }),
        }),
        smtp: None,
    }
}

fn engine() -> (SyncEngine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db.sqlite3")).unwrap();
    let blobs = BlobStore::open(&dir.path().join("blobs")).unwrap();
    (SyncEngine::new(store, blobs), dir)
}

#[tokio::test]
async fn initial_sync_populates_store() {
    if !enabled() {
        return;
    }
    let (engine, _dir) = engine();
    let results = engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();

    let (folder, stats) = results.iter().find(|(f, _)| f == "INBOX").unwrap();
    assert_eq!(stats.added, 3, "seeded maildir has 3 messages");

    let store = engine.store();
    let account = store
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    let folders = store.list_folders(&account).unwrap();
    let inbox = folders.iter().find(|f| f.name == *folder).unwrap();
    assert_eq!(inbox.role, MailboxRole::Inbox);
    assert_eq!(inbox.total, 3);
    assert_eq!(inbox.unread, 1, "the message in new/ is unread");
}

#[tokio::test]
async fn second_sync_is_a_cheap_delta() {
    if !enabled() {
        return;
    }
    let (engine, _dir) = engine();
    engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();

    // Subscribe before the second sync to observe FolderSynced stats.
    let mut events = engine.subscribe();
    let results = engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();
    let (_folder, stats) = results.iter().find(|(f, _)| f == "INBOX").unwrap();
    assert_eq!(stats.added, 0, "no new messages expected");
    assert_eq!(stats.removed, 0);

    // The event bus must have broadcast the sync of INBOX.
    let mut saw_folder_sync = false;
    while let Ok(event) = events.try_recv() {
        saw_folder_sync |= matches!(event, SyncEvent::FolderSynced { .. });
    }
    assert!(saw_folder_sync);
}

#[tokio::test]
async fn remote_folder_removal_is_reconciled() {
    if !enabled() {
        return;
    }
    let (engine, _dir) = engine();
    let config = test_account();
    let backend = ImapBackend::connect("harness", &config.imap.clone().unwrap())
        .await
        .unwrap();
    let mailbox = format!("Origami-Reconcile-{}", uuid::Uuid::now_v7());
    backend.create_mailbox(&mailbox).await.unwrap();

    engine.sync_account("harness", &config).await.unwrap();
    let account = engine
        .store()
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    assert!(engine
        .store()
        .folder_id(&account, &mailbox)
        .unwrap()
        .is_some());

    backend.delete_mailbox(&mailbox).await.unwrap();
    engine.sync_account("harness", &config).await.unwrap();
    assert!(engine
        .store()
        .folder_id(&account, &mailbox)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn remote_message_removal_is_reconciled() {
    if !enabled() {
        return;
    }
    const MAILBOX: &str = "OrigamiMessageReconcile";

    let config = test_account();
    let backend = ImapBackend::connect("harness", &config.imap.clone().unwrap())
        .await
        .unwrap();
    let _ = backend.delete_mailbox(MAILBOX).await;
    backend.create_mailbox(MAILBOX).await.unwrap();
    let raw = b"From: Reconcile <reconcile@example.org>\r\nTo: Origami <origami@localhost>\r\nSubject: remove me\r\nMessage-ID: <remove-me@example.org>\r\n\r\nbody\r\n";
    backend.append_message(MAILBOX, raw, &[]).await.unwrap();

    let (engine, _dir) = engine();
    engine.sync_account("harness", &config).await.unwrap();
    let account = engine
        .store()
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    let folder = engine
        .store()
        .folder_id(&account, MAILBOX)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine.store().list_envelopes(&folder, 1, 10).unwrap().len(),
        1
    );

    let uid = backend
        .list_envelopes(MAILBOX, 1, 10)
        .await
        .unwrap()
        .first()
        .and_then(|envelope| envelope.server_uid)
        .unwrap();
    backend.delete_message(MAILBOX, uid).await.unwrap();

    let results = engine.sync_account("harness", &config).await.unwrap();
    let stats = results
        .iter()
        .find(|(mailbox, _)| mailbox == MAILBOX)
        .map(|(_, stats)| *stats)
        .unwrap();
    assert_eq!(stats.removed, 1);
    assert!(engine
        .store()
        .list_envelopes(&folder, 1, 10)
        .unwrap()
        .is_empty());

    backend.delete_mailbox(MAILBOX).await.unwrap();
}

#[tokio::test]
async fn body_fetch_blobs_and_indexes() {
    if !enabled() {
        return;
    }
    let (engine, _dir) = engine();
    engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();

    let store = engine.store();
    let account = store
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    let folder = store.folder_id(&account, "INBOX").unwrap().unwrap();
    let envelopes = store.list_envelopes(&folder, 1, 10).unwrap();
    let digest = envelopes
        .iter()
        .find(|e| e.subject == "Weekly digest")
        .unwrap();
    let uid = digest.server_uid.unwrap();

    // Fetch body → blob + FTS index; idempotent on second call.
    let backend = ImapBackend::connect("harness", &test_account().imap.unwrap())
        .await
        .unwrap();
    let hash1 = engine
        .ensure_body(&backend, &folder, "INBOX", uid)
        .await
        .unwrap();
    let hash2 = engine
        .ensure_body(&backend, &folder, "INBOX", uid)
        .await
        .unwrap();
    assert_eq!(hash1, hash2);
    assert!(engine.blobs().contains(&hash1));
    let cached = store.parsed_message(&folder, uid).unwrap().unwrap();
    assert!(cached.html.is_some() || cached.text.is_some());

    // FTS now finds the body text of the HTML part.
    let hits = store.search("digest", 10).unwrap();
    assert!(hits.iter().any(|e| e.subject == "Weekly digest"));
}

#[tokio::test]
async fn recent_prefetch_caches_display_data_without_a_blob() {
    if !enabled() {
        return;
    }
    let config = test_account();
    let backend = ImapBackend::connect("harness", &config.imap.clone().unwrap())
        .await
        .unwrap();
    let mailbox = format!("OrigamiPrefetch-{}", uuid::Uuid::now_v7());
    backend.create_mailbox(&mailbox).await.unwrap();
    backend
        .append_message(
            &mailbox,
            concat!(
                "From: Prefetch <prefetch@example.org>\r\n",
                "To: Origami <origami@localhost>\r\n",
                "Date: Sat, 01 Jan 2000 00:00:00 +0000\r\n",
                "Subject: prefetch me\r\n",
                "Message-ID: <prefetch-me@example.org>\r\n",
                "Content-Type: text/plain; charset=utf-8\r\n",
                "\r\n",
                "This body should be warm before opening.\r\n"
            )
            .as_bytes(),
            &[],
        )
        .await
        .unwrap();

    let (engine, _dir) = engine();
    engine.sync_account("harness", &config).await.unwrap();
    let account = engine
        .store()
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    let folder = engine
        .store()
        .folder_id(&account, &mailbox)
        .unwrap()
        .unwrap();
    let envelope = engine
        .store()
        .list_envelopes(&folder, 1, 10)
        .unwrap()
        .into_iter()
        .find(|envelope| envelope.subject == "prefetch me")
        .unwrap();
    let uid = envelope.server_uid.unwrap();
    assert!(
        envelope.received_at.is_some(),
        "IMAP INTERNALDATE is required"
    );
    assert!(engine
        .store()
        .parsed_message(&folder, uid)
        .unwrap()
        .is_none());

    assert!(engine.prefetch_recent("harness", &config).await.unwrap() >= 1);
    let parsed = engine
        .store()
        .parsed_message(&folder, uid)
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.text.as_deref(),
        Some("This body should be warm before opening.")
    );
    assert!(engine.store().blob_hash(&folder, uid).unwrap().is_none());

    backend.delete_mailbox(&mailbox).await.unwrap();
}

#[tokio::test]
async fn display_fetch_skips_attachment_body_until_requested() {
    if !enabled() {
        return;
    }
    const MAILBOX: &str = "OrigamiDisplayFetch";
    let config = test_account();
    let backend = ImapBackend::connect("harness", &config.imap.clone().unwrap())
        .await
        .unwrap();
    let _ = backend.delete_mailbox(MAILBOX).await;
    backend.create_mailbox(MAILBOX).await.unwrap();
    let raw = concat!(
        "From: Display <display@example.org>\r\n",
        "To: Origami <origami@localhost>\r\n",
        "Subject: display sections\r\n",
        "Message-ID: <display-sections@example.org>\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/mixed; boundary=display-boundary\r\n",
        "\r\n",
        "--display-boundary\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "\r\n",
        "display body\r\n",
        "--display-boundary\r\n",
        "Content-Type: application/octet-stream; name=secret.bin\r\n",
        "Content-Disposition: attachment; filename=secret.bin\r\n",
        "Content-Transfer-Encoding: base64\r\n",
        "\r\n",
        "c2VjcmV0\r\n",
        "--display-boundary--\r\n",
    );
    backend
        .append_message(MAILBOX, raw.as_bytes(), &[])
        .await
        .unwrap();
    let envelope = backend
        .list_envelopes(MAILBOX, 1, 10)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let uid = envelope.server_uid.unwrap();

    let display = backend.fetch_display_message(MAILBOX, uid).await.unwrap();
    assert_eq!(display.attachments.len(), 1);
    assert_eq!(display.attachments[0].part_path, "2");
    assert_eq!(
        display
            .sections
            .iter()
            .map(|section| section.path.as_str())
            .collect::<Vec<_>>(),
        vec!["1"]
    );
    let parsed = origami_core::message::parse_display(&display).unwrap();
    assert_eq!(parsed.text.as_deref(), Some("display body"));

    let attachment = backend
        .fetch_attachment_section(MAILBOX, uid, "2")
        .await
        .unwrap();
    assert_eq!(attachment, b"secret");
    let after = backend
        .list_envelopes(MAILBOX, 1, 10)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert!(!after.flags.contains(&Flag::Seen));

    backend.delete_mailbox(MAILBOX).await.unwrap();
}

#[tokio::test]
async fn outbox_replays_to_server() {
    if !enabled() {
        return;
    }
    let (engine, _dir) = engine();
    engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();

    let store = engine.store();
    let account = store
        .upsert_account("harness", "Harness", "origami@localhost")
        .unwrap();
    let folder = store.folder_id(&account, "INBOX").unwrap().unwrap();
    let envelopes = store.list_envelopes(&folder, 1, 10).unwrap();
    let welcome = envelopes
        .iter()
        .find(|e| e.subject == "Welcome to Origami")
        .unwrap();
    let uid = welcome.server_uid.unwrap();

    // Queue an offline flag change, then replay it via a fresh sync.
    engine
        .queue_store_flags(
            &account,
            &folder,
            "INBOX",
            uid,
            &[Flag::Seen, Flag::Flagged],
        )
        .unwrap();
    assert_eq!(store.outbox_list(&account).unwrap().len(), 1);

    engine
        .sync_account("harness", &test_account())
        .await
        .unwrap();
    assert!(
        store.outbox_list(&account).unwrap().is_empty(),
        "outbox must be drained after replay"
    );

    // Server state must now include \Flagged.
    let backend = ImapBackend::connect("harness", &test_account().imap.unwrap())
        .await
        .unwrap();
    use origami_core::MailBackend;
    let server_envs = backend.list_envelopes("INBOX", 1, 10).await.unwrap();
    let welcome = server_envs
        .iter()
        .find(|e| e.subject == "Welcome to Origami")
        .unwrap();
    assert!(welcome.flags.contains(&Flag::Flagged));

    // Restore original server state for repeatable runs.
    backend
        .store_flags("INBOX", uid, &[Flag::Seen])
        .await
        .unwrap();
}

/// The IDLE/QRESYNC watch stream must observe an APPEND from another
/// session. Runs in a dedicated mailbox so concurrent tests sharing the
/// server are not disturbed.
#[tokio::test]
async fn idle_watch_observes_append() {
    if !enabled() {
        return;
    }
    const MAILBOX: &str = "OrigamiWatch";

    let config = test_account().imap.unwrap();
    let backend = ImapBackend::connect("harness", &config).await.unwrap();
    // Clean slate (ignore errors when the mailbox does not exist yet).
    let _ = backend.delete_mailbox(MAILBOX).await;
    backend.create_mailbox(MAILBOX).await.unwrap();

    // Seed one message: io-imap's watch seeds its shadow state with
    // FETCH 1:*, which is an invalid messageset on an empty mailbox.
    let seed = b"From: Watch <watch@example.org>\r\nTo: Origami <origami@localhost>\r\nSubject: seed\r\nDate: Sun, 19 Jul 2026 08:00:00 +0000\r\nMessage-ID: <watch-seed@example.org>\r\n\r\nseed\r\n";
    backend.append_message(MAILBOX, seed, &[]).await.unwrap();

    // Start the watch on a blocking thread.
    let watch_config = config.clone();
    let watcher = tokio::task::spawn_blocking(move || {
        let stream = ImapBackend::watch_mailbox_blocking(&watch_config, MAILBOX)?;
        // Wait for one event (the append below should trigger EnvelopeAdded).
        let event = stream.recv_timeout(std::time::Duration::from_secs(30));
        stream.close().ok();
        Ok::<_, origami_core::Error>(event)
    });

    // Give the watcher time to ENABLE QRESYNC + SELECT + enter IDLE.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let raw = b"From: Watch <watch@example.org>\r\nTo: Origami <origami@localhost>\r\nSubject: watch me\r\nDate: Sun, 19 Jul 2026 09:00:00 +0000\r\nMessage-ID: <watch-1@example.org>\r\n\r\nbody\r\n";
    backend.append_message(MAILBOX, raw, &[]).await.unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(40), watcher)
        .await
        .expect("watcher task timed out")
        .expect("watcher task panicked")
        .expect("watch setup failed");

    // Cleanup before asserting so reruns start clean.
    let _ = backend.delete_mailbox(MAILBOX).await;

    let event = event
        .expect("no watch event within 30 s")
        .expect("watch stream errored");
    let text = format!("{event:?}");
    assert!(
        text.contains("EnvelopeAdded") || text.contains("Flags"),
        "unexpected watch event: {text}"
    );
}
