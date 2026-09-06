//! Store unit tests (in-memory SQLite).

use origami_core::message::ParsedMessage;
use origami_core::model::{Address, Envelope, Flag, MailboxRole, OutboxOp, SyncState};
use origami_core::store::Store;

fn envelope(uid: u32, subject: &str) -> Envelope {
    Envelope {
        id: uuid::Uuid::now_v7().to_string(),
        mailbox_id: String::new(),
        subject: subject.to_string(),
        from: vec![Address {
            name: Some("Alice".to_string()),
            addr: "alice@example.org".to_string(),
        }],
        to: vec![],
        date: Some("Sat, 18 Jul 2026 10:00:00 +0000".to_string()),
        received_at: None,
        flags: vec![Flag::Seen],
        keywords: vec![],
        has_attachment: false,
        size: 100,
        server_uid: Some(uid),
        message_id: Some(format!("msg-{uid}@example.org")),
        thread_id: Some(format!("thread-{uid}@example.org")),
        sources: Vec::new(),
    }
}

fn setup() -> (Store, String, String) {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("test", "Test", "t@example.org")
        .unwrap();
    let folder = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    (store, account, folder)
}

#[test]
fn upsert_account_and_folder_are_idempotent() {
    let store = Store::open_in_memory().unwrap();
    let a1 = store.upsert_account("test", "Test", "t@x.org").unwrap();
    let a2 = store.upsert_account("test", "Test", "t@x.org").unwrap();
    assert_eq!(a1, a2);
    let f1 = store
        .upsert_folder(&a1, "INBOX", MailboxRole::Inbox)
        .unwrap();
    let f2 = store
        .upsert_folder(&a1, "INBOX", MailboxRole::Inbox)
        .unwrap();
    assert_eq!(f1, f2);
    assert_eq!(store.folder_role(&f1).unwrap(), Some(MailboxRole::Inbox));
}

#[test]
fn envelope_upsert_insert_then_update() {
    let (store, _account, folder) = setup();
    let (id1, inserted) = store
        .upsert_envelope(&folder, &envelope(1, "hello"))
        .unwrap();
    assert!(inserted);

    let mut updated = envelope(1, "hello (updated)");
    updated.flags = vec![Flag::Seen, Flag::Flagged];
    let (id2, inserted) = store.upsert_envelope(&folder, &updated).unwrap();
    assert!(!inserted);
    assert_eq!(id1, id2); // app-owned key is stable across updates

    let list = store.list_envelopes(&folder, 1, 10).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].subject, "hello (updated)");
    assert!(list[0].flags.contains(&Flag::Flagged));

    let found = store.get_envelope(&folder, 1).unwrap().unwrap();
    assert_eq!(found.id, id1);
    assert_eq!(found.subject, "hello (updated)");
    assert_eq!(found.thread_id.as_deref(), Some("thread-1@example.org"));
    assert!(store.get_envelope(&folder, 99).unwrap().is_none());

    assert!(store
        .update_keywords(&folder, 1, &["Work".to_string()])
        .unwrap());
    assert_eq!(
        store.get_envelope(&folder, 1).unwrap().unwrap().keywords,
        vec!["Work"]
    );
}

#[test]
fn envelopes_page_newest_first() {
    let (store, _account, folder) = setup();
    for uid in 1..=5 {
        store
            .upsert_envelope(&folder, &envelope(uid, &format!("m{uid}")))
            .unwrap();
    }
    let page1 = store.list_envelopes(&folder, 1, 2).unwrap();
    let page2 = store.list_envelopes(&folder, 2, 2).unwrap();
    let page3 = store.list_envelopes(&folder, 3, 2).unwrap();
    assert_eq!(
        page1
            .iter()
            .map(|e| e.server_uid.unwrap())
            .collect::<Vec<_>>(),
        vec![5, 4]
    );
    assert_eq!(
        page2
            .iter()
            .map(|e| e.server_uid.unwrap())
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(page3.len(), 1);
}

#[test]
fn envelopes_are_ordered_by_received_date_not_insert_order() {
    let (store, _account, folder) = setup();
    let mut newer = envelope(1, "newer date");
    newer.received_at = Some(300);
    newer.date = Some("Fri, 14 Aug 2026 16:53:36 +0800".to_string());
    let mut older = envelope(2, "older date");
    older.received_at = Some(100);
    older.date = Some("Wed, 13 Jan 2016 15:25:14 +0200".to_string());

    // Deliberately insert newest first, then oldest, so rowid order is wrong.
    store.upsert_envelope(&folder, &newer).unwrap();
    store.upsert_envelope(&folder, &older).unwrap();

    let messages = store.list_envelopes(&folder, 1, 10).unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["newer date", "older date"]
    );
}

#[test]
fn date_ordering_normalizes_timezone_offsets() {
    let (store, _account, folder) = setup();
    let mut old_khaki = envelope(1, "Old Khaki");
    old_khaki.date = Some("Fri, 14 Aug 2026 12:32:59 +0000".to_string());
    let mut evetech = envelope(2, "Evetech");
    evetech.date = Some("Fri, 14 Aug 2026 09:22:40 +0000".to_string());
    let mut deepseek = envelope(3, "DeepSeek");
    deepseek.date = Some("Fri, 14 Aug 2026 16:53:36 +0800".to_string());
    let mut fnb = envelope(4, "FNB");
    fnb.date = Some("Fri, 14 Aug 2026 07:22:55 +0100".to_string());

    // Insert in the reverse of the expected display order to prove the sort
    // uses normalized instants rather than SQLite insertion order or text.
    for message in [fnb, deepseek, evetech, old_khaki] {
        store.upsert_envelope(&folder, &message).unwrap();
    }

    let messages = store.list_envelopes(&folder, 1, 10).unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["Old Khaki", "Evetech", "DeepSeek", "FNB"]
    );
}

#[test]
fn search_pages_do_not_repeat_or_skip_results() {
    let (store, _account, folder) = setup();
    for uid in 1..=5 {
        store
            .upsert_envelope(&folder, &envelope(uid, &format!("page {uid}")))
            .unwrap();
    }

    let page1 = store.search_page("page", 1, 2).unwrap();
    let page2 = store.search_page("page", 2, 2).unwrap();
    let page3 = store.search_page("page", 3, 2).unwrap();
    assert_eq!(
        page1
            .iter()
            .map(|message| message.server_uid.unwrap())
            .collect::<Vec<_>>(),
        vec![5, 4]
    );
    assert_eq!(
        page2
            .iter()
            .map(|message| message.server_uid.unwrap())
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(
        page3
            .iter()
            .map(|message| message.server_uid.unwrap())
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn search_results_are_ordered_by_received_date_not_insert_order() {
    let (store, _account, folder) = setup();
    let mut newer = envelope(1, "ordered newer");
    newer.received_at = Some(300);
    let mut older = envelope(2, "ordered older");
    older.received_at = Some(100);
    store.upsert_envelope(&folder, &newer).unwrap();
    store.upsert_envelope(&folder, &older).unwrap();

    let messages = store.search_page("ordered", 1, 10).unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["ordered newer", "ordered older"]
    );
}

#[test]
fn unified_inbox_combines_accounts_but_not_other_folders() {
    let store = Store::open_in_memory().unwrap();
    for id in ["one", "two"] {
        let account = store
            .upsert_account(id, id, &format!("{id}@example.org"))
            .unwrap();
        let inbox = store
            .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
            .unwrap();
        store.upsert_envelope(&inbox, &envelope(1, id)).unwrap();
        let sent = store
            .upsert_folder(&account, "Sent", MailboxRole::Sent)
            .unwrap();
        store.upsert_envelope(&sent, &envelope(2, "sent")).unwrap();
    }
    let unified = store.list_unified_inbox(1, 10).unwrap();
    assert_eq!(unified.len(), 2);
    assert!(unified.iter().all(|message| message.subject != "sent"));
}

#[test]
fn remote_folder_reconciliation_removes_stale_rows_and_fts() {
    let (store, account, inbox) = setup();
    let stale = store
        .upsert_folder(&account, "Old project", MailboxRole::Other)
        .unwrap();
    store
        .upsert_envelope(&stale, &envelope(7, "stale project"))
        .unwrap();
    store
        .set_body(&stale, 7, "stale-hash", "stale project body")
        .unwrap();

    let removed = store
        .remove_folders_not_in(&account, &["INBOX".to_string()])
        .unwrap();
    assert_eq!(removed, vec!["Old project"]);
    assert!(store.folder_id(&account, "Old project").unwrap().is_none());
    assert_eq!(store.list_envelopes(&inbox, 1, 10).unwrap().len(), 0);
    assert!(store.search("stale project", 10).unwrap().is_empty());
}

#[test]
fn flags_update_and_delete_by_uid() {
    let (store, _account, folder) = setup();
    store.upsert_envelope(&folder, &envelope(1, "a")).unwrap();
    store.upsert_envelope(&folder, &envelope(3, "c")).unwrap();
    assert!(store.update_flags(&folder, 1, &[Flag::Flagged]).unwrap());
    assert!(!store.update_flags(&folder, 99, &[Flag::Flagged]).unwrap());
    assert_eq!(store.message_uids(&folder).unwrap(), vec![1, 3]);
    assert_eq!(store.max_server_uid(&folder).unwrap(), 3);
    assert!(store.delete_message_by_uid(&folder, 1).unwrap());
    assert!(!store.delete_message_by_uid(&folder, 1).unwrap());
    assert_eq!(store.message_uids(&folder).unwrap(), vec![3]);
    assert_eq!(store.max_server_uid(&folder).unwrap(), 3);
}

#[test]
fn sync_state_roundtrip() {
    let (store, _account, folder) = setup();
    assert_eq!(
        store.sync_state(&folder).unwrap(),
        Some(SyncState::default())
    );
    let state = SyncState {
        uid_validity: 42,
        highest_modseq: 1234,
        last_uid: 99,
    };
    store.set_sync_state(&folder, &state).unwrap();
    assert_eq!(store.sync_state(&folder).unwrap(), Some(state));
}

#[test]
fn clear_folder_messages_wipes_rows_and_fts() {
    let (store, _account, folder) = setup();
    store
        .upsert_envelope(&folder, &envelope(1, "digest"))
        .unwrap();
    store
        .set_body(&folder, 1, "hash1", "weekly digest body")
        .unwrap();
    assert_eq!(store.search("digest", 10).unwrap().len(), 1);
    assert_eq!(store.clear_folder_messages(&folder).unwrap(), 1);
    assert_eq!(store.list_envelopes(&folder, 1, 10).unwrap().len(), 0);
    assert_eq!(store.search("digest", 10).unwrap().len(), 0);
}

#[test]
fn body_indexing_feeds_fts() {
    let (store, _account, folder) = setup();
    store
        .upsert_envelope(&folder, &envelope(7, "Quarterly report"))
        .unwrap();
    assert!(store.blob_hash(&folder, 7).unwrap().is_none());
    // Envelope metadata is searchable before the body is downloaded.
    assert_eq!(store.search("quarterly", 10).unwrap().len(), 1);
    assert_eq!(store.search("alice@example.org", 10).unwrap().len(), 1);
    store
        .set_body(&folder, 7, "abc123", "revenue grew by twelve percent")
        .unwrap();
    assert_eq!(
        store.blob_hash(&folder, 7).unwrap().as_deref(),
        Some("abc123")
    );

    // FTS matches body, subject, and sender address text.
    assert_eq!(store.search("revenue", 10).unwrap().len(), 1);
    assert_eq!(store.search("quarterly", 10).unwrap().len(), 1);
    assert_eq!(store.search("alice@example.org", 10).unwrap().len(), 1);
    assert_eq!(store.search("nonexistent", 10).unwrap().len(), 0);
    assert_eq!(store.search("subject:quarterly", 10).unwrap().len(), 1);
    assert_eq!(store.search("from:alice@example.org", 10).unwrap().len(), 1);
    assert_eq!(store.search("is:read", 10).unwrap().len(), 1);
    assert_eq!(store.search("is:unread", 10).unwrap().len(), 0);
    store.update_flags(&folder, 7, &[]).unwrap();
    store
        .update_keywords(&folder, 7, &["Finance".to_string()])
        .unwrap();
    assert_eq!(
        store.search("is:unread label:Finance", 10).unwrap().len(),
        1
    );
}

#[test]
fn parsed_message_cache_roundtrips() {
    let (store, _account, folder) = setup();
    store
        .upsert_envelope(&folder, &envelope(11, "cached"))
        .unwrap();
    let parsed = ParsedMessage {
        text: Some("cached body".to_string()),
        ..ParsedMessage::default()
    };

    assert!(store.parsed_message(&folder, 11).unwrap().is_none());
    store.set_parsed_message(&folder, 11, &parsed).unwrap();
    assert_eq!(
        store.parsed_message(&folder, 11).unwrap().unwrap().text,
        parsed.text
    );
}

#[test]
fn display_cache_is_invalidated_after_mime_selection_fix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db.sqlite3");
    let (folder, uid) = {
        let store = Store::open(&path).unwrap();
        let (_account, folder) = {
            let account = store
                .upsert_account("test", "Test", "t@example.org")
                .unwrap();
            let folder = store
                .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
                .unwrap();
            (account, folder)
        };
        let message = envelope(41, "cached before parser fix");
        store.upsert_envelope(&folder, &message).unwrap();
        store
            .set_parsed_message(
                &folder,
                41,
                &ParsedMessage {
                    html: Some("<html><body>stale</body></html>".to_string()),
                    ..ParsedMessage::default()
                },
            )
            .unwrap();
        (folder, 41)
    };

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", 7_i64)
        .unwrap();
    drop(connection);

    let migrated = Store::open(&path).unwrap();
    assert!(migrated.parsed_message(&folder, uid).unwrap().is_none());
}

#[test]
fn recent_uncached_messages_use_received_time_and_skip_cached_rows() {
    let (store, account, folder) = setup();
    let mut recent = envelope(21, "recent");
    recent.received_at = Some(1_800_000_000);
    let mut old = envelope(22, "old");
    old.received_at = Some(1_700_000_000);
    let mut cached = envelope(23, "cached");
    cached.received_at = Some(1_800_000_100);

    store.upsert_envelope(&folder, &recent).unwrap();
    store.upsert_envelope(&folder, &old).unwrap();
    store.upsert_envelope(&folder, &cached).unwrap();
    store
        .set_parsed_message(&folder, 23, &ParsedMessage::default())
        .unwrap();

    let candidates = store
        .recent_uncached_messages(&account, 1_750_000_000, 20, None)
        .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.envelope.server_uid)
            .collect::<Vec<_>>(),
        vec![Some(21)]
    );
    assert_eq!(candidates[0].mailbox, "INBOX");
}

#[test]
fn recent_uncached_messages_prefer_selected_folder_then_inbox() {
    let (store, account, inbox) = setup();
    let archive = store
        .upsert_folder(&account, "Archive", MailboxRole::Archive)
        .unwrap();
    let lists = store
        .upsert_folder(&account, "Lists", MailboxRole::Other)
        .unwrap();
    let mut inbox_mail = envelope(41, "inbox");
    inbox_mail.received_at = Some(1_800_000_000);
    let mut archive_mail = envelope(42, "archive");
    archive_mail.received_at = Some(1_800_000_000);
    let mut lists_mail = envelope(43, "lists");
    lists_mail.received_at = Some(1_800_000_000);
    store.upsert_envelope(&inbox, &inbox_mail).unwrap();
    store.upsert_envelope(&archive, &archive_mail).unwrap();
    store.upsert_envelope(&lists, &lists_mail).unwrap();

    let candidates = store
        .recent_uncached_messages(&account, 1_750_000_000, 20, Some(&lists))
        .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.mailbox.as_str())
            .collect::<Vec<_>>(),
        vec!["Lists", "INBOX", "Archive"]
    );
}

#[test]
fn logical_cache_lookup_reuses_a_cached_copy_from_another_folder() {
    let (store, account, inbox) = setup();
    let archive = store
        .upsert_folder(&account, "Archive", MailboxRole::Archive)
        .unwrap();
    let primary = envelope(31, "logical cache");
    let mut copy = envelope(32, "logical cache");
    copy.message_id = primary.message_id.clone();
    copy.thread_id = primary.thread_id.clone();
    store.upsert_envelope(&inbox, &primary).unwrap();
    store.upsert_envelope(&archive, &copy).unwrap();
    store
        .set_parsed_message(
            &archive,
            32,
            &ParsedMessage {
                text: Some("cached through another label".to_string()),
                ..ParsedMessage::default()
            },
        )
        .unwrap();

    let parsed = store
        .parsed_message_for_logical_message(&inbox, 31)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.text.as_deref(), Some("cached through another label"));
}

#[test]
fn display_cache_indexes_without_a_complete_blob() {
    let (store, _account, folder) = setup();
    store
        .upsert_envelope(&folder, &envelope(12, "display cache"))
        .unwrap();
    let parsed = ParsedMessage {
        text: Some("display-only body".to_string()),
        ..ParsedMessage::default()
    };

    store
        .set_parsed_message_and_index(&folder, 12, &parsed, "root@example.org")
        .unwrap();
    assert!(store.blob_hash(&folder, 12).unwrap().is_none());
    assert_eq!(
        store
            .get_envelope(&folder, 12)
            .unwrap()
            .unwrap()
            .thread_id
            .as_deref(),
        Some("root@example.org")
    );
    assert_eq!(store.search("display-only", 10).unwrap().len(), 1);
}

#[test]
fn outbox_lifecycle() {
    let (store, account, _folder) = setup();
    let other_account = store
        .upsert_account("other", "Other", "other@example.org")
        .unwrap();
    assert_eq!(store.outbox_count(&account).unwrap(), 0);
    let op = OutboxOp::StoreFlags {
        mailbox: "INBOX".to_string(),
        server_uid: 3,
        flags: vec![Flag::Seen],
        keywords: None,
    };
    let id = store.outbox_add(&account, &op).unwrap();
    assert_eq!(store.outbox_count(&account).unwrap(), 1);
    assert_eq!(store.outbox_count(&other_account).unwrap(), 0);
    let entries = store.outbox_list(&account).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].op, op);
    assert_eq!(entries[0].attempts, 0);

    store.outbox_mark_failed(id, "connection refused").unwrap();
    let entries = store.outbox_list(&account).unwrap();
    assert_eq!(entries[0].attempts, 1);
    assert_eq!(entries[0].last_error.as_deref(), Some("connection refused"));
    assert_eq!(store.outbox_count(&account).unwrap(), 1);

    store.outbox_remove(id).unwrap();
    assert!(store.outbox_list(&account).unwrap().is_empty());
    assert_eq!(store.outbox_count(&account).unwrap(), 0);
}

#[test]
fn move_and_delete_outbox_ops_roundtrip() {
    let (store, account, _folder) = setup();
    let operations = [
        OutboxOp::MoveMessages {
            source_mailbox: "INBOX".to_string(),
            destination_mailbox: "Archive".to_string(),
            server_uids: vec![4, 7],
        },
        OutboxOp::DeleteMessages {
            mailbox: "Trash".to_string(),
            server_uids: vec![9],
        },
        OutboxOp::SendMessage {
            raw_base64: "bWVzc2FnZQ==".to_string(),
        },
        OutboxOp::AppendSent {
            raw_base64: "c2VudA==".to_string(),
        },
    ];
    for operation in &operations {
        store.outbox_add(&account, operation).unwrap();
    }
    let entries = store.outbox_list(&account).unwrap();
    assert_eq!(
        entries
            .into_iter()
            .map(|entry| entry.op)
            .collect::<Vec<_>>(),
        operations
    );
}

#[test]
fn composer_draft_roundtrip() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.load_draft("composer").unwrap().is_none());
    store
        .save_draft("composer", r#"{"subject":"first"}"#)
        .unwrap();
    assert_eq!(
        store.load_draft("composer").unwrap().as_deref(),
        Some(r#"{"subject":"first"}"#)
    );
    store
        .save_draft("composer", r#"{"subject":"updated"}"#)
        .unwrap();
    assert_eq!(
        store.load_draft("composer").unwrap().as_deref(),
        Some(r#"{"subject":"updated"}"#)
    );
    store
        .set_draft_remote("composer", "account", "Drafts", 42)
        .unwrap();
    assert_eq!(
        store.draft_remote("composer").unwrap(),
        Some(("account".to_string(), "Drafts".to_string(), 42))
    );
    store.delete_draft("composer").unwrap();
    assert!(store.load_draft("composer").unwrap().is_none());
}

#[test]
fn saved_search_lifecycle() {
    let store = Store::open_in_memory().unwrap();
    let saved = store.save_search("Unread", "is:unread").unwrap();
    assert_eq!(store.list_saved_searches().unwrap(), vec![saved.clone()]);
    store.delete_saved_search(&saved.id).unwrap();
    assert!(store.list_saved_searches().unwrap().is_empty());
}

#[test]
fn correspondents_are_ranked_from_message_history() {
    let (store, _account, folder) = setup();
    store.upsert_envelope(&folder, &envelope(1, "one")).unwrap();
    store.upsert_envelope(&folder, &envelope(2, "two")).unwrap();
    let contacts = store.list_correspondents(10).unwrap();
    assert_eq!(contacts[0].addr, "alice@example.org");
    assert_eq!(contacts[0].name.as_deref(), Some("Alice"));
    assert_eq!(contacts[0].message_count, 2);
}

#[test]
fn logical_sent_sources_list_together_without_losing_mailbox_identity() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("test", "Test", "t@example.org")
        .unwrap();
    let sent = store
        .upsert_folder(&account, "Sent", MailboxRole::Sent)
        .unwrap();
    let gmail_sent = store
        .upsert_folder(&account, "[Gmail]/Sent Mail", MailboxRole::Sent)
        .unwrap();

    store.upsert_envelope(&sent, &envelope(1, "plain")).unwrap();
    store
        .upsert_envelope(&gmail_sent, &envelope(2, "gmail"))
        .unwrap();

    let sources = store.folder_source_ids(&sent).unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources.contains(&sent));
    assert!(sources.contains(&gmail_sent));

    let messages = store.list_envelopes_in_folders(&sources, 1, 10).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].subject, "gmail");
    assert_eq!(messages[0].mailbox_id, gmail_sent);
    assert_eq!(messages[1].mailbox_id, sent);
}

#[test]
fn logical_counts_deduplicate_copies_across_source_folders() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("test", "Test", "t@example.org")
        .unwrap();
    let sent = store
        .upsert_folder(&account, "Sent", MailboxRole::Sent)
        .unwrap();
    let gmail_sent = store
        .upsert_folder(&account, "[Gmail]/Sent Mail", MailboxRole::Sent)
        .unwrap();

    let mut first = envelope(1, "same sent message");
    first.message_id = Some("<same-sent@example.org>".into());
    first.flags = vec![Flag::Seen];
    let mut second = first.clone();
    second.server_uid = Some(2);
    second.flags = Vec::new();
    store.upsert_envelope(&sent, &first).unwrap();
    store.upsert_envelope(&gmail_sent, &second).unwrap();

    let (total, unread) = store.logical_envelope_counts(&[sent, gmail_sent]).unwrap();
    assert_eq!((total, unread), (1, 1));
}

#[test]
fn label_copies_are_one_logical_envelope_with_one_unread_state() {
    let store = Store::open_in_memory().unwrap();
    let account = store
        .upsert_account("test", "Test", "t@example.org")
        .unwrap();
    let inbox = store
        .upsert_folder(&account, "INBOX", MailboxRole::Inbox)
        .unwrap();
    let archive = store
        .upsert_folder(&account, "[Gmail]/All Mail", MailboxRole::Archive)
        .unwrap();
    let important = store
        .upsert_folder(&account, "[Gmail]/Important", MailboxRole::Other)
        .unwrap();

    let mut first = envelope(10, "same message");
    first.message_id = Some("<same@example.org>".into());
    first.flags = vec![Flag::Seen];
    let mut second = first.clone();
    second.server_uid = Some(20);
    let mut third = first.clone();
    third.server_uid = Some(30);
    third.flags = Vec::new();
    store.upsert_envelope(&inbox, &first).unwrap();
    store.upsert_envelope(&archive, &second).unwrap();
    store.upsert_envelope(&important, &third).unwrap();

    let folders = vec![inbox.clone(), archive.clone(), important.clone()];
    let messages = store.list_envelopes_in_folders(&folders, 1, 10).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(!messages[0].flags.contains(&Flag::Seen));
    assert_eq!(messages[0].sources.len(), 3);
}

#[test]
fn duplicate_message_ids_in_one_folder_are_not_merged() {
    let (store, _account, folder) = setup();
    let mut first = envelope(40, "same metadata");
    first.message_id = Some("<duplicated@example.org>".into());
    let mut second = first.clone();
    second.server_uid = Some(41);

    store.upsert_envelope(&folder, &first).unwrap();
    store.upsert_envelope(&folder, &second).unwrap();

    let messages = store.list_envelopes(&folder, 1, 10).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| message.sources.len() == 1));
}
