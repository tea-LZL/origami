//! Regression: the warm-cache scheduler is fire-and-forget and must stay a
//! no-op, never a process abort, when reached from a thread with no Tokio
//! runtime.
//!
//! Tauri runs synchronous `#[tauri::command]` functions on the main thread.
//! `tokio::spawn` panics there, and because that panic crosses an FFI
//! callback boundary it aborts the process (see `prefetch_selected_folder`
//! in `origami-app`). This test fails without the runtime guard.

use std::sync::Arc;

use origami_core::blob::BlobStore;
use origami_core::config::AccountConfig;
use origami_core::store::Store;
use origami_core::sync::SyncEngine;

#[test]
fn spawn_recent_prefetch_without_a_runtime_does_not_panic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open_in_memory().expect("in-memory store");
    let blobs = BlobStore::open(&dir.path().join("blobs")).expect("blob store");
    let engine = Arc::new(SyncEngine::new(store, blobs));
    let account = AccountConfig {
        name: "test".into(),
        email: "test@example.org".into(),
        default: false,
        imap: None,
        smtp: None,
    };

    // Deliberately not a `#[tokio::test]`: no runtime is entered on this
    // thread, which is exactly the main-thread shape that aborted Origami.
    engine.spawn_recent_prefetch("account-1".to_string(), account, None);
}

#[tokio::test]
async fn prefetch_display_caches_without_flags() {
    use origami_core::message::{DisplayMessage, DisplaySection};
    use origami_core::model::{Address, Envelope, Flag, MailboxRole};
    use origami_core::prefetch_queue::{PrefetchPriority, PrefetchRequest};

    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open_in_memory().expect("in-memory store");
    let blobs = BlobStore::open(&dir.path().join("blobs")).expect("blob store");
    let engine = Arc::new(SyncEngine::new(store, blobs));

    let account = engine
        .store()
        .upsert_account("test", "Test", "test@example.org")
        .unwrap();
    let folder = engine
        .store()
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    let mut envelope = Envelope {
        id: uuid::Uuid::now_v7().to_string(),
        mailbox_id: folder.clone(),
        subject: "warm me".into(),
        from: vec![Address {
            name: Some("Alice".into()),
            addr: "alice@example.org".into(),
        }],
        to: vec![],
        date: None,
        received_at: None,
        flags: vec![Flag::Seen],
        keywords: vec![],
        has_attachment: false,
        size: 100,
        server_uid: Some(7),
        message_id: Some("<warm@example.org>".into()),
        thread_id: Some("<warm@example.org>".into()),
        sources: Vec::new(),
    };
    envelope.sources.push(origami_core::model::EnvelopeSource {
        mailbox_id: folder.clone(),
        server_uid: 7,
    });
    engine.store().upsert_envelope(&folder, &envelope).unwrap();
    assert!(engine.store().parsed_message(&folder, 7).unwrap().is_none());

    let display = DisplayMessage {
        headers: b"Subject: warm me\r\nFrom: Alice <alice@example.org>\r\n".to_vec(),
        parts: Vec::new(),
        attachments: Vec::new(),
        sections: vec![DisplaySection {
            path: "1".into(),
            mime_headers: b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec(),
            body: b"warm body".to_vec(),
        }],
    };

    // Stub fetch mirrors the production queue closure's write path.
    let queue = engine.prefetch_queue();
    let engine_for_fetch = engine.clone();
    let display_for_fetch = display.clone();
    queue.run(move |key: String| {
        let engine = engine_for_fetch.clone();
        let display = display_for_fetch.clone();
        async move {
            let (folder_id, uid) = key.rsplit_once(':').unwrap();
            engine
                .cache_display_message(folder_id, uid.parse().unwrap(), &display)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
    });
    assert!(queue.enqueue(PrefetchRequest::new(
        format!("{folder}:7"),
        PrefetchPriority::Viewport
    )));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let parsed = engine
        .store()
        .parsed_message(&folder, 7)
        .unwrap()
        .expect("display cache populated by queued prefetch");
    assert_eq!(parsed.text.as_deref(), Some("warm body"));
    let flags = engine
        .store()
        .list_envelopes(&folder, 1, 10)
        .unwrap()
        .remove(0)
        .flags;
    assert_eq!(flags, vec![Flag::Seen], "prefetch must never touch flags");
}
