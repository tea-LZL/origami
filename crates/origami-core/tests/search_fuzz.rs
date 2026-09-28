//! Navigation/search boundary fuzz gate: arbitrary query strings — including
//! SQL metacharacters, stray colons, and filter tokens — must execute without
//! error and keep `search_count` consistent with `search_page`.

use origami_core::model::{Address, Envelope, MailboxRole};
use origami_core::store::Store;
use proptest::prelude::*;

fn envelope(uid: u32, subject: &str) -> Envelope {
    Envelope {
        id: format!("fuzz-{uid}"),
        mailbox_id: String::new(),
        subject: subject.to_string(),
        from: vec![Address {
            name: None,
            addr: "f@example.org".into(),
        }],
        to: vec![],
        date: None,
        received_at: Some(uid as i64),
        flags: vec![],
        keywords: vec![],
        has_attachment: false,
        size: 1,
        server_uid: Some(uid),
        message_id: Some(format!("<fuzz-{uid}@example.org>")),
        thread_id: None,
        sources: vec![],
    }
}

fn seeded() -> (Store, String, String) {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("fuzz", "Fuzz", "f@example.org")
        .unwrap();
    let folder = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    for uid in 1..=8u32 {
        let mut envelope = envelope(uid, &format!("subject with uid {uid}"));
        envelope.mailbox_id = folder.clone();
        store.upsert_envelope(&folder, &envelope).unwrap();
    }
    (store, account, folder)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn search_handles_arbitrary_queries(
        query in r"[\x20-\x7e]{0,80}",
    ) {
        prop_assume!(!query.contains('\u{0}'));
        let (store, _account, _folder) = seeded();

        // Both paths execute; the count is always consistent with the page.
        let count = store.search_count(&query).unwrap();
        let page = store.search_page(&query, 1, 50).unwrap();
        prop_assert!(count as usize >= page.len());

        // Filter tokens never blow up either.
        let _ = store.search_count(&format!("from:{query}")).unwrap();
        let _ = store.search_count(&format!("subject:{query}")).unwrap();
        let _ = store.search_count(&format!("is:{query}")).unwrap();
        let _ = store.search_count(&format!("has:{query}")).unwrap();
        let _ = store.search_count(&format!("tag:{query}")).unwrap();
        let _ = store.search_count(&format!("account:{query}")).unwrap();
        let _ = store.search_count(&format!("folder:{query}")).unwrap();
    }

    #[test]
    fn repeated_upserts_stay_idempotent(
        uid in 1u32..1000,
        subjects in prop::collection::vec(r"[\x20-\x7e]{0,60}", 1..5),
    ) {
        let (store, _account, folder) = seeded();
        for subject in &subjects {
            let mut envelope = envelope(uid, subject);
            envelope.mailbox_id = folder.clone();
            store.upsert_envelope(&folder, &envelope).unwrap();
        }
        let rows = store.list_envelopes(&folder, 1, 100).unwrap();
        let matching = rows
            .iter()
            .filter(|envelope| envelope.server_uid == Some(uid))
            .count();
        prop_assert_eq!(matching, 1, "one physical row per uid regardless of repeats");
    }
}
