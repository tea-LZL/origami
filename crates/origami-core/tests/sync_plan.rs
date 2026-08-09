//! Property tests for the pure sync planner (ADR-0003).

use origami_core::model::SyncState;
use origami_core::sync::{plan_sync, SyncAction};
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
