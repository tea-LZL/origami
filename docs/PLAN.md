# Origami — Master Plan

A fast, offline-first desktop email client for **Arch Linux / Wayland (Hyprland-first)**,
with the reliability of Thunderbird's data model and himalaya's protocol correctness,
wrapped in a bespoke UI carrying the Origami CI (blue folded-paper bird).

## Locked Decisions

| Decision | Choice |
|---|---|
| Language | **Rust** (core) + **TypeScript** (UI) |
| GUI | **Tauri 2** + **Svelte 5** (runes) + Vite, webkit2gtk-4.1 webview |
| Protocol layer | pimalaya **`io-imap` / `io-smtp`** git-pinned, wrapped behind our own backend trait |
| Scope v1 | IMAP+SMTP multi-account · offline-first sync + FTS5 local search · OAuth2 (Gmail/Outlook) |
| Composer | Rich HTML (TipTap) |
| UI style | Bespoke custom-styled (Origami blue accent system) |
| Packaging | **AUR PKGBUILD** (`origami`, `origami-git`) + CI release builds |
| Not in v1 | PGP (v1.1, sequoia), JMAP/Gmail-API/Graph backends (v1.2+, architecture permits) |

## Why This Stack (Research Summary)

- **Himalaya v2** is binary-only, but its `io-*` crates are MIT/Apache and exactly what we need:
  `io-imap` (sans-IO coroutines on `imap-codec 2.0-alpha`, with **IDLE, CONDSTORE/QRESYNC,
  UIDPLUS, MOVE, SORT/THREAD, OAUTHBEARER/XOAUTH2, SCRAM-SHA-256**), `io-smtp`, plus
  `mail-parser`/`mail-builder`. We pin git revisions and isolate them behind our own
  `MailBackend` trait modeled on himalaya's `src/shared/client.rs` — upstream churn touches
  one module, not the app.
- **Thunderbird's hard-won lessons** we adopt directly:
  - **Panorama ADR-0002: never use IMAP UIDs as primary keys.** One global SQLite DB,
    app-owned keys, server UID stored as an attribute.
  - **One index, one view layer**: SQLite FTS5 for all search — not three parallel systems.
  - **Remote content: block by default**, per-origin exceptions, one-shot override;
    scam heuristics on links.
  - Avoid: bespoke DB formats (Mork), per-folder DBs, C++-style giant state machines.
- **Tauri on Hyprland**: webview = WebKitGTK (mature Wayland support, DMA-BUF renderer),
  ~15 MB native binary, Rust core in-process with tokio. HTML email rendering is free and
  sandboxed. Stable `app_id` (`dev.origami.mail`) so Hyprland window rules work; frameless
  window + custom titlebar for the bespoke look.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Svelte 5 UI (webview) — bespoke Origami design system  │
│  3-pane layout · virtualized lists · TipTap composer    │
└──────────────▲──────────────────────────▲───────────────┘
               │ Tauri commands (invoke)   │ Tauri events (sync deltas, new mail)
┌──────────────┴───────────────────────────┴───────────────┐
│  origami-app (Tauri shell, Rust)                         │
│  command layer · window/tray/notifications · keyring     │
├──────────────────────────────────────────────────────────┤
│  origami-core (pure Rust lib, no Tauri deps)             │
│  ┌────────────┐ ┌──────────────┐ ┌─────────────────────┐ │
│  │ SyncEngine │ │ Store        │ │ Accounts/Secrets    │ │
│  │ (tokio,    │ │ SQLite+FTS5  │ │ (TOML + libsecret   │ │
│  │  per-acct  │ │ + blob store │ │  via oo7, OAuth2    │ │
│  │  actors)   │ │ (maildir-ish)│ │  token manager)     │ │
│  └─────┬──────┘ └──────────────┘ └─────────────────────┘ │
│  ┌─────┴───────────────────────────────────────────────┐ │
│  │ MailBackend trait  →  ImapBackend (io-imap) ·       │ │
│  │ SmtpSender (io-smtp) · future: JMAP/Gmail backends  │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

**Process model**: single process; tokio multi-thread runtime owns sync; webview on the
main thread. UI **never blocks on IMAP** — it reads only from SQLite, sync pushes deltas
as events. This is the core responsiveness guarantee.

**Concurrency**: one actor per account (`AccountActor`) owning its IMAP connection pool
(1 session for IDLE on INBOX + a small pool for fetches), command mailbox via
`tokio::mpsc`. Blocking `io-*` std-clients are pumped on `tokio::task::spawn_blocking`
workers (io-imap's sans-IO core also permits a fully-async adapter later — the trait
hides the choice).

## Data & Sync Engine (the reliability heart)

**Storage**
- `~/.local/share/origami/db.sqlite` — one global DB (WAL mode): `accounts`, `folders`,
  `messages` (app-owned UUID PK; `server_uid`, `folder_id`, `message_id` header as
  attributes; RFC 8474 OBJECTID when offered), `threads` (jwz-style on
  References/In-Reply-To), `attachments` (metadata), `tags` (IMAP keywords ↔ colored
  labels), `contacts`, `sync_state` (per-folder UIDVALIDITY + HIGHESTMODSEQ), `outbox`.
- FTS5 virtual table over subject/from/to/body-plain, porter tokenizer; indexed lazily
  after body fetch.
- Message bodies: raw RFC822 blobs in a content-addressed store (`blobs/<aa>/<hash>`),
  attachments decoded on demand. (Deliberately *files*, not mbox/Mork.)

**Sync algorithm per folder** (mirrors io-imap's `watch` machinery):
1. `ENABLE QRESYNC` → `SELECT (CONDSTORE)` → compare UIDVALIDITY
   (mismatch → full resync of that folder).
2. Delta pull: `UID FETCH <last+1>:* (FLAGS)` + `FETCH CHANGEDSINCE <modseq>` → upsert
   into SQLite → emit UI events.
3. New-mail: `NOOP`/IDLE wake on INBOX → fetch new envelopes → local notification.
4. Flag moves/deletes propagate bidirectionally; offline ops queue in `outbox` and replay
   on reconnect.
5. Envelope-first, body-on-demand (like Thunderbird autosync): headers sync eagerly,
   bodies fetched on open, background prefetch of recent N messages.

**Sending**: compose → `mail-builder` → append to Sent (UIDPLUS) + SMTP send via
`io-smtp`; offline → Outbox with retry.

## UI Design (bespoke, Origami CI)

- **Design system**: custom Svelte component kit on CSS custom properties. Origami blue
  ramp (primary ≈ #2B6CF6 family from the CI bird), paper-white surfaces, dark theme
  default-following (`org.freedesktop.appearance` portal), subtle folded-paper motif
  (angled section edges, origami bird logo, fold-in transitions). Typography: Inter/UI
  system stack, JetBrains Mono for raw view.
- **Layout**: classic 3-pane (folder list / thread list / message) with collapsible
  sidebar, vertical-layout option, unified Inbox toggle, tabbed messages
  (Thunderbird-style).
- **Responsiveness engineering**: virtualized thread list (10k+ rows at 60fps), skeleton
  states, optimistic flag changes, all queries pre-warmed from SQLite — no spinners on
  cached data.
- **Composer**: modal + detachable window, TipTap rich text (bold/lists/links/inline
  images/attachments, signature), serialized to `multipart/alternative`.
- **Message view**: sanitized HTML in an isolated webview partition; remote images
  blocked by default with per-sender/per-origin allow bar; phishing hint banner;
  attachment list with save/open.
- **Hyprland integration**: frameless window + custom titlebar (drag zones, double-click
  maximize), rounded corners left to compositor, stable `app_id`, XDG portals for
  notifications (`notify-rust`), tray via `libappindicator` (optional), "new mail" badge.

## Security & Accounts

- **Secrets**: `oo7`/libsecret (GNOME Keyring / KWallet via Secret Service) — OAuth
  refresh tokens and passwords never in the TOML config (himalaya's `command` secret
  pattern also supported for `pass` users).
- **OAuth2**: PKCE flow in a dedicated webview window (Google + Microsoft client IDs),
  token manager with proactive refresh, XOAUTH2/OAUTHBEARER SASL via io-imap. Password
  auth (PLAIN/LOGIN/SCRAM) equally supported.
- **TLS**: rustls with platform verifier (same as pimalaya-stream).
- **Rendering**: DOMPurify sanitize + CSP (`default-src 'none'; img-src data: cid:`),
  `cid:` served through a Tauri custom protocol scoped per-message.

## Repo Layout

```
Origami/
├─ Cargo.toml               # workspace
├─ crates/
│  ├─ origami-core/         # backend trait, sync engine, store, search, oauth, accounts
│  ├─ origami-app/          # Tauri shell: commands, events, tray, notifications, windows
│  └─ origami-cli/          # thin debug CLI (sync status, reindex) — dogfooding tool
├─ ui/                      # Svelte 5 + Vite + TS + Tailwind(tokens) + TipTap
│  └─ src/{lib,components,routes,stores}
├─ packaging/arch/PKGBUILD  # AUR
├─ assets/                  # origami bird icons (from CI sheet), .desktop file
├─ tests/                   # integration: dovecot/greenmail harness, sync fixtures
└─ docs/adr/                # our own ADRs (key ownership, backend trait, sync algo)
```

## Testing & Quality ("bug-free" mandate)

- **Unit**: core logic, jwz threading, search compiler, sync-state transitions
  (property tests for flag math).
- **Integration**: Docker **Dovecot + greenmail** harness in CI — real
  IMAP/IDLE/CONDSTORE conversations, offline-replay scenarios, UIDVALIDITY-flip resyncs.
- **Provider matrix** (manual per release, mirroring himalaya's `docs/testing/`):
  Gmail (OAuth2), Fastmail, Outlook, Posteo, plain Dovecot.
- **UI**: Playwright e2e against the Tauri dev server; list-perf benchmark gate
  (10k-message folder scroll).
- `clippy -D warnings`, `cargo-deny`, rustfmt in CI; Svelte `check` + eslint.

## Milestones

| # | Milestone | Deliverable |
|---|---|---|
| M0 | Scaffold | Workspace, Tauri+Svelte shell, CI, Hyprland-tested frameless window, design tokens from CI |
| M1 | Core spine | `origami-core`: MailBackend trait + io-imap/io-smtp wrappers, account config, connect/list/fetch/send against Dovecot harness |
| M2 | Store & sync | SQLite schema + FTS5, content-addressed blobs, CONDSTORE delta sync, IDLE watcher, outbox replay |
| M3 | UI v1 | 3-pane, thread list, message view (sanitized HTML + remote-content bar), rich composer, send/receive E2E |
| M4 | Accounts & OAuth2 | Multi-account, onboarding wizard (autodiscovery via `io-pim-discovery` patterns), Google/MS OAuth2 + libsecret |
| M5 | Polish | Unified inbox, tags, saved searches, notifications, tray, keyboard nav, theming, perf pass |
| M6 | Ship | AUR `origami` + `origami-git`, .desktop/icon integration, v1.0 |

Post-v1: PGP (sequoia), JMAP backend, conversation view across folders, filters/rules engine.

## Top Risks & Mitigations

1. **pimalaya alpha churn** → git-pinned revs, backend-trait isolation, `cargo update`
   only via deliberate bump PRs with the Dovecot harness green.
2. **WebKitGTK perf quirks on Wayland** → keep DOM lean, virtualize lists, GPU
   rasterization env workaround documented for NVIDIA; fallback
   `WEBKIT_DISABLE_DMABUF_RENDERER`.
3. **IMAP edge cases (Gmail SORT, UIDVALIDITY flips)** → provider test matrix + fallback
   policies copied from himalaya's `src/imap/backend.rs`.
4. **Sync correctness** → Thunderbird ADR-0002 key model + property tests + append-only
   sync journal for debugging.
5. **Scope creep** → M0–M6 gates; PGP/JMAP explicitly parked.

---

# Todo Checklist

## M0 — Scaffold ✅
- [x] Cargo workspace (`origami-core`, `origami-app`, `origami-cli`)
- [x] Tauri 2 + Svelte 5 + Vite + TS UI shell (`ui/`)
- [x] Origami design tokens (colors, typography) from CI sheet
- [x] Frameless window + custom titlebar, `app_id = dev.origami.mail`
- [x] App icons + `.desktop` file (origami bird)
- [x] Dev/build verification on Hyprland (`npm run tauri dev`, release build)
- [x] CI workflow (fmt, clippy, svelte-check, build)

## M1 — Core Spine ✅
- [x] ADR-0001: message key ownership · ADR-0002: backend trait · ADR-0003: sync algorithm
- [x] `origami-core` domain types (Account, Mailbox, Envelope, Flag, Message)
- [x] `MailBackend` trait + `SmtpSender` trait
- [x] `ImapBackend` over git-pinned `io-imap` (connect, list, envelopes, fetch, flags, move/copy)
- [x] `SmtpClient` over git-pinned `io-smtp` (send raw RFC822)
- [x] Account config TOML load/save + secret indirection
- [x] `origami-cli`: `account check`, `folder list`, `envelope list` smoke commands
- [x] Dovecot+greenmail Docker harness with seeded fixtures

## M2 — Store & Sync
- [ ] SQLite schema + migrations (sqlx/rusqlite), WAL, FTS5
- [ ] Content-addressed blob store
- [ ] SyncEngine: per-account actors, CONDSTORE/QRESYNC delta sync
- [ ] UIDVALIDITY-flip full resync path
- [ ] IDLE watcher on INBOX + reconnect/backoff
- [ ] Outbox queue + offline replay
- [ ] jwz threading pass
- [ ] FTS indexer (lazy after body fetch)
- [ ] Property tests: flag math, sync-state transitions

## M3 — UI v1
- [ ] 3-pane layout shell (folders / thread list / message)
- [ ] Virtualized thread list
- [ ] Message view: sanitized HTML, remote-content bar, attachments
- [ ] TipTap rich composer (multipart/alternative via mail-builder)
- [ ] Send/receive E2E against harness
- [ ] Optimistic flag updates + skeleton states

## M4 — Accounts & OAuth2
- [ ] Multi-account management + unified inbox
- [ ] Onboarding wizard with autodiscovery
- [ ] OAuth2 PKCE (Google + Microsoft), token manager
- [ ] libsecret storage via oo7 (+ `pass`-style command secrets)

## M5 — Polish
- [ ] Tags (IMAP keywords ↔ colored labels)
- [ ] Saved searches / virtual folders
- [ ] Notifications + tray + unread badge
- [ ] Full keyboard navigation
- [ ] Theming pass (dark/light), perf pass (10k scroll gate)
- [ ] Playwright e2e suite

## M6 — Ship
- [ ] AUR PKGBUILD (`origami`, `origami-git`)
- [ ] GitHub release workflow (artifacts, checksums)
- [ ] .desktop + icon theme integration
- [ ] v1.0 tag
