//! Property tests for the pure sync planner (ADR-0003).

use origami_core::imap::SelectInfo;
use origami_core::model::SyncState;
use origami_core::sync::{inbox_needs_sync, plan_sync, SyncAction};
use proptest::prelude::*;

fn arb_state() -> impl Strategy<Value = SyncState> {
    (any::<u32>(), any::<u64>(), any::<u32>()).prop_map(|(uv, modseq, uid)| SyncState {
        uid_validity: uv,
        highest_modseq: modseq,
        last_uid: uid,
    })
}

proptest! {
    /// A UIDVALIDITY flip ALWAYS forces a full resync (ADR-0003 §1).
    #[test]
    fn uidvalidity_flip_always_full_resync(
        state in arb_state(),
        server_uv in any::<u32>(),
        exists in any::<u32>(),
        condstore in any::<bool>(),
    ) {
        prop_assume!(state.uid_validity != server_uv);
        let action = plan_sync(Some(state), Some(server_uv), exists, condstore);
        prop_assert_eq!(action, SyncAction::FullResync);
    }

    /// No baseline + empty folder → nothing to do; non-empty → initial.
    #[test]
    fn fresh_state_initial_or_noop(exists in any::<u32>(), condstore in any::<bool>()) {
        let action = plan_sync(None, Some(1), exists, condstore);
        if exists == 0 {
            prop_assert_eq!(action, SyncAction::Noop);
        } else {
            prop_assert_eq!(action, SyncAction::InitialSync);
        }
    }

    /// Same UIDVALIDITY NEVER produces a full resync.
    #[test]
    fn same_uidvalidity_never_full_resync(
        state in arb_state(),
        exists in any::<u32>(),
        condstore in any::<bool>(),
    ) {
        let uv = state.uid_validity;
        let action = plan_sync(Some(state), Some(uv), exists, condstore);
        prop_assert!(action != SyncAction::FullResync);
    }

    /// Delta always resumes after the stored last_uid, and only uses
    /// CHANGEDSINCE when the server offers CONDSTORE and we have a
    /// modseq on record.
    #[test]
    fn delta_shape(
        state in arb_state(),
        exists in any::<u32>(),
        condstore in any::<bool>(),
    ) {
        let uv = state.uid_validity;
        prop_assume!(state.last_uid > 0 || exists == 0);
        prop_assume!(!(state.last_uid == 0 && exists == 0)); // Noop case
        let action = plan_sync(Some(state), Some(uv), exists, condstore);
        if state.last_uid == 0 && exists > 0 {
            prop_assert_eq!(action, SyncAction::InitialSync);
        } else if let SyncAction::Delta { after_uid, changed_since } = action {
            prop_assert_eq!(after_uid, state.last_uid);
            match (condstore, state.highest_modseq > 0) {
                (true, true) => prop_assert_eq!(changed_since, Some(state.highest_modseq)),
                _ => prop_assert_eq!(changed_since, None),
            }
        } else {
            return Err(TestCaseError::fail(format!(
                "expected Delta or InitialSync, got {action:?}"
            )));
        }
    }
}

fn checkpoint(uv: u32, modseq: u64) -> SyncState {
    SyncState {
        uid_validity: uv,
        highest_modseq: modseq,
        last_uid: 1,
    }
}

fn server(uv: u32, exists: u32, modseq: Option<u64>) -> SelectInfo {
    SelectInfo {
        exists,
        uid_validity: Some(uv),
        highest_modseq: modseq,
    }
}

/// INBOX poll decision: no checkpoint means we have never synced this box.
#[test]
fn inbox_poll_without_checkpoint_syncs() {
    assert!(inbox_needs_sync(None, &server(1, 0, Some(0)), 0));
}

/// A UIDVALIDITY flip always forces a sync, whatever the counts say.
#[test]
fn inbox_poll_uidvalidity_flip_syncs() {
    let stored = checkpoint(7, 100);
    assert!(inbox_needs_sync(
        Some(&stored),
        &server(8, 42, Some(100)),
        42
    ));
}

/// With CONDSTORE, a changed modseq is the signal to sync.
#[test]
fn inbox_poll_modseq_change_syncs() {
    let stored = checkpoint(7, 100);
    assert!(inbox_needs_sync(
        Some(&stored),
        &server(7, 42, Some(101)),
        42
    ));
}

/// Unchanged modseq means nothing changed, even if EXISTS is misleading.
#[test]
fn inbox_poll_steady_modseq_skips() {
    let stored = checkpoint(7, 100);
    assert!(!inbox_needs_sync(
        Some(&stored),
        &server(7, 42, Some(100)),
        42
    ));
    // EXISTS drift is ignored while the modseq is authoritative.
    assert!(!inbox_needs_sync(
        Some(&stored),
        &server(7, 99, Some(100)),
        42
    ));
}

/// Without a modseq, fall back to comparing EXISTS with the local count.
#[test]
fn inbox_poll_no_modseq_uses_exists() {
    let stored = checkpoint(7, 0);
    assert!(inbox_needs_sync(Some(&stored), &server(7, 99, None), 42));
    assert!(!inbox_needs_sync(Some(&stored), &server(7, 42, None), 42));
}
