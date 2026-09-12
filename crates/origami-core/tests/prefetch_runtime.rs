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
