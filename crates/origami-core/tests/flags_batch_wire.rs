//! Wire-level check for bulk flag writes against an in-process IMAP server.
//!
//! Bulk triage on a few hundred messages used to cost SELECT + FETCH + STORE
//! per message (≈600 round trips, minutes on a real account). The batch path
//! must stay at one `FETCH` plus one `STORE` per distinct delta.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use origami_core::config::{AuthMechanism, ImapConfig, Secret};
use origami_core::imap::ImapBackend;
use origami_core::model::Flag;

#[derive(Default)]
struct WireState {
    commands: Vec<String>,
    seen: HashSet<u32>,
}

fn spawn_server(message_count: u32) -> (u16, Arc<Mutex<WireState>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test IMAP");
    let port = listener.local_addr().expect("local addr").port();
    let state = Arc::new(Mutex::new(WireState::default()));
    let server_state = Arc::clone(&state);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            if !serve(stream, message_count, &server_state) {
                break;
            }
        }
    });
    (port, state)
}

fn test_config(port: u16) -> ImapConfig {
    ImapConfig {
        host: "127.0.0.1".to_string(),
        port: Some(port),
        tls: false,
        starttls: false,
        auth: AuthMechanism::Login,
        username: "origami".to_string(),
        secret: Some(Secret::Raw {
            raw: "origami".to_string(),
        }),
    }
}

/// Serve one session; returns false when the listener should stop.
fn serve(stream: std::net::TcpStream, message_count: u32, state: &Arc<Mutex<WireState>>) -> bool {
    let mut writer = stream.try_clone().expect("clone stream");
    let mut reader = BufReader::new(stream);
    let mut reply = |line: &str| {
        let _ = writer.write_all(line.as_bytes());
        let _ = writer.write_all(b"\r\n");
        let _ = writer.flush();
    };

    reply("* OK [CAPABILITY IMAP4rev1 AUTH=PLAIN AUTH=LOGIN] test ready");

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(_) => return true,
        }
        let command = line.trim_end().to_string();
        if command.is_empty() {
            continue;
        }
        let mut tokens = command.split_whitespace();
        let tag = tokens.next().unwrap_or("*").to_string();
        let mut verb = tokens.next().unwrap_or("").to_ascii_uppercase();
        let mut args: Vec<String> = tokens.map(str::to_string).collect();
        if verb == "UID" {
            verb = args
                .first()
                .map(|value| value.to_ascii_uppercase())
                .unwrap_or_default();
            args.remove(0);
        }
        state
            .lock()
            .expect("wire state")
            .commands
            .push(format!("{verb} {}", args.join(" ")));

        match verb.as_str() {
            "CAPABILITY" => {
                reply("* CAPABILITY IMAP4rev1 AUTH=PLAIN AUTH=LOGIN");
                reply(&format!("{tag} OK CAPABILITY completed"));
            }
            "LOGIN" | "AUTHENTICATE" => reply(&format!("{tag} OK LOGIN completed")),
            "SELECT" | "EXAMINE" => {
                reply("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)");
                reply(&format!("* {message_count} EXISTS"));
                reply("* OK [UIDVALIDITY 1] UIDs valid");
                reply("* OK [UIDNEXT 1] Predicted next UID");
                reply("* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft \\*)] Limited");
                reply(&format!("{tag} OK [READ-WRITE] SELECT completed"));
            }
            "FETCH" => {
                let uids = parse_uids(
                    args.first().map(String::as_str).unwrap_or(""),
                    message_count,
                );
                let seen = state.lock().expect("wire state").seen.clone();
                for (index, uid) in uids.iter().enumerate() {
                    let flags = if seen.contains(uid) { "\\Seen" } else { "" };
                    reply(&format!(
                        "* {} FETCH (UID {uid} FLAGS ({flags}))",
                        index + 1
                    ));
                }
                reply(&format!("{tag} OK FETCH completed"));
            }
            "STORE" => {
                let uids = parse_uids(
                    args.first().map(String::as_str).unwrap_or(""),
                    message_count,
                );
                let op = args
                    .get(1)
                    .cloned()
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                let flag_text = args.join(" ").to_ascii_uppercase();
                let mut state = state.lock().expect("wire state");
                for uid in &uids {
                    // Only the system flags this test exercises are modelled.
                    if flag_text.contains("\\SEEN") {
                        if op.starts_with('+') {
                            state.seen.insert(*uid);
                        } else if op.starts_with('-') {
                            state.seen.remove(uid);
                        }
                    }
                }
                reply(&format!("{tag} OK STORE completed"));
            }
            "LOGOUT" => {
                reply("* BYE logging out");
                reply(&format!("{tag} OK LOGOUT completed"));
                return true;
            }
            _ => reply(&format!("{tag} OK {verb} completed")),
        }
    }
}

/// Parse an IMAP sequence set (`1:208`, `3,9`, `1:*`) into UIDs.
fn parse_uids(spec: &str, message_count: u32) -> Vec<u32> {
    let mut uids = Vec::new();
    for part in spec.split(',') {
        match part.split_once(':') {
            Some((start, end)) => {
                let start: u32 = start.parse().unwrap_or(1);
                let end = if end == "*" {
                    message_count
                } else {
                    end.parse().unwrap_or(start)
                };
                for uid in start..=end.max(start) {
                    uids.push(uid);
                }
            }
            None => {
                if let Ok(uid) = part.parse() {
                    uids.push(uid);
                }
            }
        }
    }
    uids
}

fn store_commands(state: &Arc<Mutex<WireState>>) -> Vec<String> {
    state
        .lock()
        .expect("wire state")
        .commands
        .iter()
        .filter(|command| command.starts_with("STORE "))
        .cloned()
        .collect()
}

fn fetch_commands(state: &Arc<Mutex<WireState>>) -> Vec<String> {
    state
        .lock()
        .expect("wire state")
        .commands
        .iter()
        .filter(|command| command.starts_with("FETCH "))
        .cloned()
        .collect()
}

/// The direct bulk action (app layer) sends an intent, not a flag diff: one
/// `STORE` and no `FETCH`, so flags set on another device survive.
#[tokio::test]
async fn bulk_read_intent_is_one_store_and_no_fetch() {
    const MESSAGES: u32 = 208;
    let (port, state) = spawn_server(MESSAGES);
    let backend = ImapBackend::connect("wire", &test_config(port))
        .await
        .expect("connect");
    let uids: Vec<u32> = (1..=MESSAGES).collect();

    backend
        .store_flag_batch("INBOX", &uids, Flag::Seen, true)
        .await
        .expect("bulk read intent");

    let commands = state.lock().expect("wire state").commands.clone();
    assert!(
        fetch_commands(&state).is_empty(),
        "intent store never fetches: {commands:?}"
    );
    let stores = store_commands(&state);
    assert_eq!(stores.len(), 1, "one STORE for the whole batch: {stores:?}");
    assert!(
        stores[0].contains("+FLAGS") && stores[0].to_ascii_uppercase().contains("\\SEEN"),
        "adds Seen: {}",
        stores[0]
    );
    assert_eq!(
        state.lock().expect("wire state").seen.len(),
        MESSAGES as usize,
        "every message is read upstream"
    );

    // Unread is the mirrored remove intent.
    state.lock().expect("wire state").commands.clear();
    backend
        .store_flag_batch("INBOX", &uids[..2], Flag::Seen, false)
        .await
        .expect("bulk unread intent");
    let stores = store_commands(&state);
    assert_eq!(stores.len(), 1, "one STORE for the unread batch");
    assert!(
        stores[0].contains("-FLAGS") && stores[0].to_ascii_uppercase().contains("\\SEEN"),
        "removes Seen: {}",
        stores[0]
    );
    assert_eq!(
        state.lock().expect("wire state").seen.len(),
        MESSAGES as usize - 2,
        "only the targeted messages are unread again"
    );
}

#[tokio::test]
async fn bulk_mark_read_is_one_fetch_and_one_store() {
    const MESSAGES: u32 = 208;
    let (port, state) = spawn_server(MESSAGES);
    let backend = ImapBackend::connect("wire", &test_config(port))
        .await
        .expect("connect");

    let updates: Vec<(u32, Vec<Flag>)> =
        (1..=MESSAGES).map(|uid| (uid, vec![Flag::Seen])).collect();
    backend
        .store_flags_many("INBOX", &updates)
        .await
        .expect("bulk store");

    assert_eq!(
        fetch_commands(&state).len(),
        1,
        "one FETCH for the whole batch: {:?}",
        fetch_commands(&state)
    );
    let stores = store_commands(&state);
    assert_eq!(stores.len(), 1, "one STORE for the whole batch: {stores:?}");
    assert!(
        stores[0].contains("+FLAGS") && stores[0].to_ascii_uppercase().contains("\\SEEN"),
        "delta store adds Seen: {}",
        stores[0]
    );
    assert_eq!(
        state.lock().expect("wire state").seen.len(),
        MESSAGES as usize,
        "every message is read upstream"
    );

    // Re-applying the same flags is a no-op on the wire.
    state.lock().expect("wire state").commands.clear();
    backend
        .store_flags_many("INBOX", &updates)
        .await
        .expect("redundant bulk store");
    assert_eq!(
        fetch_commands(&state).len(),
        1,
        "one FETCH on the no-op pass"
    );
    assert!(
        store_commands(&state).is_empty(),
        "no STORE when nothing changes: {:?}",
        store_commands(&state)
    );
}

#[tokio::test]
async fn divergent_flags_group_into_separate_stores() {
    const MESSAGES: u32 = 3;
    let (port, state) = spawn_server(MESSAGES);
    let backend = ImapBackend::connect("wire", &test_config(port))
        .await
        .expect("connect");

    // UID 2 keeps Flagged, so it cannot share UID 1 and 3's delta.
    let updates: Vec<(u32, Vec<Flag>)> = vec![
        (1, vec![Flag::Seen]),
        (2, vec![Flag::Seen, Flag::Flagged]),
        (3, vec![Flag::Seen]),
    ];
    backend
        .store_flags_many("INBOX", &updates)
        .await
        .expect("bulk store");

    let stores = store_commands(&state);
    assert_eq!(stores.len(), 2, "one store per distinct delta: {stores:?}");
    assert_eq!(
        state.lock().expect("wire state").seen,
        HashSet::from([1, 2, 3]),
        "all three are read upstream"
    );
}

#[tokio::test]
async fn queued_bulk_flags_replay_as_one_store() {
    use origami_core::blob::BlobStore;
    use origami_core::config::AccountConfig;
    use origami_core::store::Store;
    use origami_core::sync::SyncEngine;

    const MESSAGES: u32 = 120;
    let (port, state) = spawn_server(MESSAGES);
    let backend = ImapBackend::connect("wire", &test_config(port))
        .await
        .expect("connect");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db.sqlite3")).unwrap();
    let blobs = BlobStore::open(&dir.path().join("blobs")).unwrap();
    let engine = SyncEngine::new(store, blobs);
    let account = engine
        .store()
        .upsert_account("acct", "Wire", "wire@localhost")
        .unwrap();

    // Offline bulk triage: one outbox row per message.
    for uid in 1..=MESSAGES {
        engine
            .queue_store_flags(&account, "folder", "INBOX", uid, &[Flag::Seen])
            .unwrap();
    }
    assert_eq!(
        engine.store().outbox_list(&account).unwrap().len(),
        MESSAGES as usize
    );

    let config = AccountConfig {
        name: "Wire".to_string(),
        email: "wire@localhost".to_string(),
        default: true,
        imap: Some(test_config(port)),
        smtp: None,
        signature: None,
    };
    state.lock().expect("wire state").commands.clear();
    engine
        .replay_outbox(&backend, "acct", &account, &config)
        .await;

    assert!(
        engine.store().outbox_list(&account).unwrap().is_empty(),
        "replay drains the queue"
    );
    assert_eq!(
        fetch_commands(&state).len(),
        1,
        "queued flags replay in one FETCH: {:?}",
        fetch_commands(&state)
    );
    assert_eq!(
        store_commands(&state).len(),
        1,
        "queued flags replay in one STORE: {:?}",
        store_commands(&state)
    );
    assert_eq!(
        state.lock().expect("wire state").seen.len(),
        MESSAGES as usize,
        "every queued message is read upstream"
    );
}
