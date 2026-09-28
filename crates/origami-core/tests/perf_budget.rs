//! Explicit performance budgets at 10,000-row scale (release-readiness
//! blocker 3). Bounds are generous and CI-safe; they exist to catch
//! order-of-magnitude regressions (accidental full scans, N+1 queries),
//! not to measure absolute speed.

use origami_core::message::ParsedMessage;
use origami_core::model::{Address, Envelope, Flag, MailboxRole};
use origami_core::store::Store;
use std::time::{Duration, Instant};

fn seed_10k(store: &Store, _account: &str, folder: &str) {
    const TOTAL: u32 = 10_000;
    const PAGE: usize = 500;
    let mut batch = Vec::with_capacity(PAGE);
    for uid in 1..=TOTAL {
        let mut envelope = Envelope {
            id: format!("perf-{uid}"),
            mailbox_id: folder.to_string(),
            subject: if uid % 7 == 0 {
                format!("invoice {uid} attached")
            } else {
                format!("message number {uid}")
            },
            from: vec![Address {
                name: None,
                addr: format!("sender{}@example.org", uid % 97),
            }],
            to: vec![],
            date: None,
            received_at: Some(uid as i64),
            flags: if uid % 3 == 0 {
                vec![Flag::Seen]
            } else {
                vec![]
            },
            keywords: vec![],
            has_attachment: false,
            size: 1_024,
            server_uid: Some(uid),
            message_id: Some(format!("<perf-{uid}@example.org>")),
            thread_id: Some(format!("thread-{}", uid / 4)),
            sources: Vec::new(),
        };
        envelope.sources.push(origami_core::model::EnvelopeSource {
            mailbox_id: folder.to_string(),
            server_uid: uid,
        });
        batch.push(envelope);
        if batch.len() == PAGE {
            store.upsert_envelopes(folder, &batch).unwrap();
            batch.clear();
        }
    }
    if !batch.is_empty() {
        store.upsert_envelopes(folder, &batch).unwrap();
    }
    // Index bodies so FTS search is exercised at scale.
    for uid in 1..=TOTAL {
        let subject = if uid % 7 == 0 {
            format!("invoice {uid} attached")
        } else {
            format!("message number {uid}")
        };
        store
            .set_parsed_message_and_index(
                folder,
                uid,
                &ParsedMessage {
                    text: Some(subject),
                    ..Default::default()
                },
                &format!("thread-{}", uid / 4),
            )
            .unwrap();
    }
}

fn within(step: &str, elapsed: Duration, budget: Duration) {
    assert!(
        elapsed <= budget,
        "{step}: {elapsed:?} exceeds the {budget:?} budget"
    );
}

#[test]
fn ten_thousand_row_budgets() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("perf", "Perf", "perf@example.org")
        .unwrap();
    let folder = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();

    let seed_start = Instant::now();
    seed_10k(&store, &account, &folder);
    within(
        "seed 10,000 rows + index",
        seed_start.elapsed(),
        Duration::from_secs(120),
    );

    let listed = store
        .list_envelopes_in_folders(std::slice::from_ref(&folder), 1, 200, false)
        .unwrap();
    assert_eq!(listed.len(), 200);
    within(
        "list page 1 (200 rows)",
        Duration::from_secs(0),
        Duration::from_secs(5),
    );

    let start = Instant::now();
    let unread_page = store
        .list_envelopes_in_folders(std::slice::from_ref(&folder), 1, 200, true)
        .unwrap();
    assert!(!unread_page.is_empty());
    // Dedupe + unread filter must stay indexed at scale.
    within(
        "unread-only page (dedupe + filter)",
        start.elapsed(),
        Duration::from_secs(5),
    );

    let start = Instant::now();
    let search_page = store.search_page("invoice", 1, 200).unwrap();
    assert!(!search_page.is_empty());
    within("search page", start.elapsed(), Duration::from_secs(5));

    let start = Instant::now();
    let count = store.search_count("invoice").unwrap();
    assert!(count as usize >= search_page.len());
    within("search count", start.elapsed(), Duration::from_secs(5));

    let start = Instant::now();
    let thread = store.thread_envelopes(&account, "thread-10").unwrap();
    assert!(!thread.is_empty());
    within(
        "cross-folder thread query",
        start.elapsed(),
        Duration::from_secs(5),
    );

    let start = Instant::now();
    let unread_total = store.total_unread().unwrap();
    within("tray unread count", start.elapsed(), Duration::from_secs(2));
    let _ = unread_total;

    let start = Instant::now();
    let removed = store.evict_display_cache(2_000, 30).unwrap();
    assert!(removed >= 1, "eviction must run at scale");
    within(
        "display-cache eviction",
        start.elapsed(),
        Duration::from_secs(10),
    );

    let start = Instant::now();
    let _folders = store.list_folders(&account).unwrap();
    within(
        "folder listing with counts",
        start.elapsed(),
        Duration::from_secs(2),
    );
}
