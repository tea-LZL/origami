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

fn seed_label_copies(store: &Store, folder: &str, total: u32) {
    const PAGE: usize = 500;
    let mut batch = Vec::with_capacity(PAGE);
    for uid in 1..=total {
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
            message_id: Some(format!("<copy-{uid}@example.org>")),
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
}

fn seed_no_message_ids(store: &Store, folder: &str, total: u32) {
    const PAGE: usize = 500;
    let mut batch = Vec::with_capacity(PAGE);
    for uid in 1..=total {
        let envelope = Envelope {
            id: format!("no-id-{uid}"),
            mailbox_id: folder.to_string(),
            subject: format!("no id message {uid}"),
            from: vec![Address {
                name: None,
                addr: format!("sender{}@example.org", uid % 97),
            }],
            to: vec![],
            date: Some(format!("Sat, 18 Jul 2026 10:{:02}:00 +0000", uid % 60)),
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
            message_id: None,
            thread_id: Some(format!("thread-{}", uid / 4)),
            sources: Vec::new(),
        };
        batch.push(envelope);
        if batch.len() == PAGE {
            store.upsert_envelopes(folder, &batch).unwrap();
            batch.clear();
        }
    }
    if !batch.is_empty() {
        store.upsert_envelopes(folder, &batch).unwrap();
    }
}

#[test]
#[ignore = "2k no-Message-ID benchmark; run explicitly"]
fn no_message_id_page_budget() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("perf", "Perf", "perf@example.org")
        .unwrap();
    let inbox = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    seed_no_message_ids(&store, &inbox, 2_000);

    for page in 1..=5 {
        let start = Instant::now();
        let listed = store
            .list_envelopes_in_folders(std::slice::from_ref(&inbox), page, 200, false)
            .unwrap();
        let elapsed = start.elapsed();
        eprintln!("2k no-Message-ID page {page}: {elapsed:?}");
        assert_eq!(listed.len(), 200);
        within("2k no-Message-ID page", elapsed, Duration::from_millis(250));
    }
}

#[test]
#[ignore = "40k-row label-copy benchmark; run explicitly"]
fn forty_thousand_label_copy_page_budget() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("perf", "Perf", "perf@example.org")
        .unwrap();
    let inbox = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    let archive = store
        .upsert_folder(&account, "[Gmail]/All Mail", MailboxRole::Archive)
        .unwrap();

    seed_label_copies(&store, &inbox, 20_000);
    seed_label_copies(&store, &archive, 20_000);

    let folders = [inbox, archive];
    let start = Instant::now();
    let listed = store
        .list_envelopes_in_folders(&folders, 1, 200, false)
        .unwrap();
    let elapsed = start.elapsed();
    eprintln!("40k label-copy page 1: {elapsed:?}");
    assert_eq!(listed.len(), 200);
    within("40k label-copy page 1", elapsed, Duration::from_millis(250));

    let start = Instant::now();
    let unread = store
        .list_envelopes_in_folders(&folders, 1, 200, true)
        .unwrap();
    let elapsed = start.elapsed();
    eprintln!("40k label-copy unread page 1: {elapsed:?}");
    assert!(!unread.is_empty());
    within(
        "40k label-copy unread page 1",
        elapsed,
        Duration::from_millis(250),
    );

    let start = Instant::now();
    let (total, unread_total) = store.logical_envelope_counts(&folders).unwrap();
    let elapsed = start.elapsed();
    eprintln!("40k label-copy counts: {elapsed:?}");
    assert_eq!(total, 20_000);
    assert!(unread_total > 0);
    within("40k label-copy counts", elapsed, Duration::from_secs(2));
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

    let listed_start = Instant::now();
    let listed = store
        .list_envelopes_in_folders(std::slice::from_ref(&folder), 1, 200, false)
        .unwrap();
    assert_eq!(listed.len(), 200);
    within(
        "list page 1 (200 rows)",
        listed_start.elapsed(),
        Duration::from_millis(500),
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
