# Sync performance and message-card border

Two independent changes. Do the card first: the cause is local and visible. Do sync second, and only the steps below. Do not open a second IMAP connection, do not fetch full RFC 822 during envelope sync, and do not change ADR-0003's envelope-first rule.

## 1. Message card border (no attachment footer)

The reading sheet is three stacked pieces in `ui/src/lib/MessageView.svelte`:

- `header` draws the full border and the top radius.
- `.body` draws only the left and right borders (`border-inline`).
- `footer` draws the bottom border and the bottom radius, and it is rendered only when `attachments.length > 0`.

A message with no attachments therefore has side borders that stop with no bottom edge. The email iframe's own bottom radius sits inside that open sheet, so the card looks cut off.

**Change**

- When there is no footer, close the sheet on `.message > .body`: bottom border, bottom radius (`var(--radius-lg)`), and the same sheet shadow the footer uses.
- When a footer is present, leave `.body` as side borders only, and remove `footer { margin-top: 32px }` so the footer meets the body instead of floating 32px below an open edge.
- Do not change header chrome, attachment download behavior, or the iframe sandbox.

`:has(> footer)` is enough; no new markup is required. WebKitGTK in this app supports it.

**Proof**

- Extend `ui/src/lib/MessageView.test.ts`. The fixture already has `attachments: []`. Assert the body (`[data-navigation="message-body"]`) has a class such as `sheet-end` when there are no attachments.
- Add one case with a single attachment: the Attachments footer is present, and the body does not have `sheet-end`.
- Class is the regression check. Confirm the actual border in the running app on a message with no attachments and one with attachments, at desktop width and under 760px.

## 2. Sync performance

Current account sync (`sync_account_inner` in `crates/origami-core/src/sync.rs`) walks every mailbox on one IMAP session, in `LIST` order. After that, `prefetch_recent` opens another session and fetches up to 500 display bodies, one message at a time, each `fetch_display_message` doing `SELECT` plus two `FETCH`s.

Two costs dominate, and both contradict the "O(delta)" claim in ADR-0003:

1. Every delta, even with CONDSTORE, calls `search_all_uids` and compares every local UID (`sync.rs` around the expunge loop). QRESYNC `VANISHED` is not implemented.
2. Prefetch then does hundreds of serial body fetches before the next IDLE watch.

Initial envelope paging (200 per page, one SQLite transaction per page) is already reasonable. Leave it.

### Step A — time the existing passes

Add `tracing` spans (or fields on the existing `FolderSynced` path) for:

- per-folder duration, mailbox role, added/changed/removed
- duration of the full-UID search inside a delta
- prefetch count and duration

No behavior change. Run one real account sync and keep the numbers. Stop if INBOX-first plus a smaller prefetch already makes the wait acceptable; do not start Step C without that measurement.

### Step B — make the visible folder finish first, and prefetch less

Still one IMAP session. Still envelope-first.

- In `sync_account_inner`, stable-sort mailboxes so `Inbox` is first, then the folder the UI has open if that is known, then everything else. `LIST` order must not put a huge Gmail label ahead of INBOX.
- Cut `PREFETCH_MAX_MESSAGES` from 500 to the first page of the open folder (30). Skip a candidate when its mailbox was just selected and the next candidate is in the same mailbox, instead of `SELECT`ing again inside `fetch_display_message` for every UID.
- Prefetch must stay display-only (`BODY.PEEK` / `fetch_display_message`). Do not switch it to full RFC 822.

Proof: a unit test that the mailbox sort puts Inbox before `All Mail` / `[Gmail]/All Mail` regardless of input order. Existing prefetch tests must still skip messages over `PREFETCH_MAX_MESSAGE_SIZE` and must not mark mail read.

### Step C — only if Step A shows the UID sweep is the large cost

Replace the unconditional `search_all_uids` on a delta when the server advertised QRESYNC and the watch/select response includes `VANISHED`. Delete those UIDs only. Keep the full UID search as the fallback when CONDSTORE/QRESYNC is absent, or when the vanished set was not returned.

Do not drop the fallback. A folder that only supports plain IMAP must still converge.

Proof: extend `sync_harness` (or a focused sync test) with two cases:

- QRESYNC vanished UID is deleted locally and the full-folder UID search is not required for that pass.
- No QRESYNC: a UID that disappeared still disappears locally via the existing search.

## Out of scope

- Parallel IMAP connections.
- Syncing only INBOX and skipping other folders.
- Code signing, Windows packaging, or product-copy changes.
- Rewriting the message HTML iframe.

## Order

1. Card border and its test.
2. Sync timing (Step A).
3. Inbox-first and smaller prefetch (Step B).
4. VANISHED expunge (Step C) only if the timings say the UID sweep is the problem.
