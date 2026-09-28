use origami_core::blob::BlobStore;
use origami_core::config::AccountConfig;
use origami_core::store::Store;
use origami_core::sync::{SyncEngine, SyncEvent};

#[tokio::test]
async fn failed_sync_emits_started_and_terminal_error() {
    let dir = tempfile::tempdir().unwrap();
    let engine = SyncEngine::new(
        Store::open_in_memory().unwrap(),
        BlobStore::open(&dir.path().join("blobs")).unwrap(),
    );
    let mut events = engine.subscribe();
    let account = AccountConfig {
        name: "No IMAP".into(),
        email: "none@example.org".into(),
        default: true,
        imap: None,
        smtp: None,
        signature: None,
    };

    assert!(engine.sync_account("missing", &account).await.is_err());
    assert!(matches!(
        events.recv().await.unwrap(),
        SyncEvent::AccountSyncStarted { ref account_id } if account_id == "missing"
    ));
    assert!(matches!(
        events.recv().await.unwrap(),
        SyncEvent::Error { ref account_id, .. } if account_id == "missing"
    ));
}

#[test]
fn queueing_an_operation_emits_outbox_changed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = SyncEngine::new(
        Store::open_in_memory().unwrap(),
        BlobStore::open(&dir.path().join("blobs")).unwrap(),
    );
    let account_db_id = engine
        .store()
        .upsert_account("work", "Work", "work@example.org")
        .unwrap();
    let mut events = engine.subscribe();

    engine
        .queue_move_messages(&account_db_id, "INBOX", "Archive", &[7])
        .unwrap();

    assert!(matches!(
        events.try_recv().unwrap(),
        SyncEvent::OutboxChanged { ref account_id } if account_id == "work"
    ));
}
