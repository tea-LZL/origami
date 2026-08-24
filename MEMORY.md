# Origami repository memory

> Operational snapshot for developers and agents. Updated 2026-08-24. Verify this file against source before changing a release claim.

## Project identity

Origami is a provider-neutral, offline-first Linux desktop email client. The product target is Arch Linux and Wayland/Hyprland first, while the architecture remains portable.

- Core: Rust 2021 (`origami-core`)
- Desktop shell: Tauri 2 (`origami-app`)
- UI: Svelte 5 runes + TypeScript + Vite (`ui/`)
- Debug surface: Rust CLI (`origami-cli`)
- Local persistence: SQLite/WAL + FTS5 + content-addressed blobs
- Protocols: IMAP and SMTP behind Origami-owned traits
- Current version: `0.1.0`

The tracked source is approximately 17,700 lines across 66 Rust, Svelte, TypeScript, and CSS files, excluding dependencies and build output.

## Repository index

| Path | Responsibility |
|---|---|
| `crates/origami-core/src/backend.rs` | Provider-neutral read/send traits |
| `crates/origami-core/src/imap.rs` | IMAP adapter and MIME display-section retrieval |
| `crates/origami-core/src/smtp.rs` | SMTP adapter |
| `crates/origami-core/src/store.rs` | SQLite schema, migrations, FTS, logical projection, cache, outbox |
| `crates/origami-core/src/sync.rs` | In-process account sync loops and recent-message prefetch |
| `crates/origami-core/src/message.rs` | MIME parsing and normalized display data |
| `crates/origami-core/src/model.rs` | Provider-neutral domain models and physical source identity |
| `crates/origami-app/src/commands.rs` | Tauri command/DTO boundary |
| `crates/origami-app/src/state.rs` | Store, config, workers, OAuth refresh, cancellation ownership |
| `crates/origami-app/src/lib.rs` | Tauri lifecycle, tray, sync events, notifications |
| `ui/src/lib/stores.svelte.ts` | UI state, requests, optimistic actions, persisted UI preferences |
| `ui/src/lib/ThreadList.svelte` | Search, selection, batch actions, conversation projection |
| `ui/src/lib/MessageView.svelte` | Isolated message rendering and message actions |
| `ui/src/lib/messageHtml.ts` | HTML sanitization, CSP, resource policy, iframe document |
| `ui/src/app.css` | Theme, density, motion, focus, and accessibility tokens |
| `.github/workflows/ci.yml` | Local CI definition for Rust and UI gates |
| `docs/adr/` | Primary-key, backend seam, and sync invariants |
| `docs/IMPROVEMENT_PLAN.md` | Feature/QoL backlog; checkbox state is not release evidence |
| `docs/TEST_SUITE.md` | Docker/live protocol test setup and current limitations |
| `tests/harness/` | Optional Dovecot/GreenMail integration harness |

## Runtime architecture

```text
Svelte UI
  -> typed Tauri commands/events
origami-app
  -> AppState (config + Store + SyncEngine + cancellation)
origami-core
  -> SQLite/FTS/blob cache
  -> MailBackend / SmtpSender
  -> IMAP/SMTP servers
```

The UI reads normalized DTOs and must not contain provider-specific logic. Sync, cache warming, outbox replay, notifications, and UI commands run in the existing process; there is no daemon or startup service.

Messages have two identities:

1. Origami-owned logical identity for deduplicated presentation.
2. Every physical `(mailbox_id, server_uid)` source for reads, flags, labels, moves, deletes, attachments, and sync.

Never treat an IMAP UID as globally unique or discard a retained physical source.

## Current behavior

The current worktree implements the main daily-driver flows:

- multi-account onboarding, password auth, Gmail/Microsoft OAuth, and keyring-backed secrets;
- indexed envelope sync, SQLite/FTS search, saved searches, unified Inbox, tags, and conversations;
- body-on-demand loading plus bounded recent display-cache warming;
- offline flag/move/delete/send operations with replay and an Outbox inspector;
- reply, reply-all, forward, attachments, drafts, and duplicate-send protection;
- logical cross-label deduplication while retaining all physical sources;
- chronological ordering by normalized server-received time;
- isolated HTML email rendering with DOMPurify, restrictive CSP, link interception, and remote images blocked by default;
- light/dark/system themes, density, motion preferences, resizable layouts, tray lifecycle, notifications, and keyboard list navigation.

The uncommitted worktree currently also contains the logical-source projection, recent prefetch, tray lifecycle, HTML rendering fixes, sender-address metadata, and broad UI polish. Do not overwrite or broadly restore these files.

## Production readiness

**Current classification: feature-rich beta / daily-driver candidate, not public-production ready.**

`docs/IMPROVEMENT_PLAN.md` reports 64 of 81 items complete (79% checklist coverage), but that measures feature breadth, not release confidence. Public release readiness is lower because several proof and distribution gates are absent.

### Release blockers

1. **Distribution and automation**
   - GitLab CI now verifies Rust/UI, builds an Arch package in an Arch container, and publishes tagged package/checksum artifacts; the remote pipeline is not yet verified;
   - `packaging/arch/PKGBUILD` is tracked and installs the native binary, desktop entry, and hicolor icons;
   - the Tauri `deb` target remains, while Arch distribution uses the tracked PKGBUILD;
   - signed pacman repository support is deferred until the app matures;
   - manifest declares MIT/Apache-2.0, but license text files are absent;
   - no v1 tag yet.
2. **Provider evidence**
   - Gmail, Outlook, Fastmail, Posteo, and Dovecot release matrix is not recorded;
   - current real-provider OAuth cannot be revalidated until the revoked/invalid grant is reauthorized.
3. **End-to-end confidence**
   - GreenMail SMTP round-trip tolerates a known `io-smtp` greeting-parser incompatibility;
   - no Playwright daily-driver suite or native WebKit automated workflow;
   - no explicit 10,000-row, cached-open, search, or sync performance budgets.
4. **Security and accessibility gates**
   - the audit found existing data/config directories at `0755` and mail/config/database files at `0644`; this worktree enforces `0700` roots plus `0600` config/database/new-blob files on the next open/write, but an existing-data native smoke is still required;
   - the npm dependency tree is audit-clean in this worktree; no `cargo-audit`/`cargo-deny` policy is installed or automated yet;
   - sanitizer/component tests exist, but no broad hostile-message corpus or navigation fuzz gate;
   - keyboard/focus behavior is only partially tested;
   - no automated accessibility or visual-regression checks.
5. **Daily-driver gaps**
   - recipient chips/contact management, signatures, inline images, and plain-text compose mode;
   - folder hierarchy/subscriptions;
   - cross-folder thread query and per-folder view restoration;
   - font/message zoom and configurable mark-read behavior;
   - explicit discard/start-clean draft action and search-context/result-count header;
   - grouped notification routing and unread badge behavior.

A 1.0 release should not be declared until the first four groups have explicit evidence. The fifth group can be split into required 1.0 items and documented post-1.0 scope.

## Recommended QoL order

1. Preserve folder/search selection and scroll state.
2. Add explicit Discard/Start clean actions for recovered composer drafts.
3. Label global-search context correctly and show its result count.
4. Add configurable delayed mark-as-read and message zoom.
5. Add recipient chips over the existing learned correspondent data.
6. Complete dialog focus entry/trapping/restoration and keyboard interaction tests.
7. Add folder hierarchy/subscription support.
8. Add signatures/plain-text compose mode before inline-image editing.
9. Consolidate repeated dialog/button/status patterns only as each flow is touched.

Avoid speculative provider abstractions, new background services, or broad component rewrites.

## UI and motion rules

- Maintain a dense Thunderbird-like hierarchy and Himalaya-like keyboard speed.
- Use theme tokens; define light, dark, and system-dark values together.
- Prefer crisp borders and selected-state bars over glow, blur, or large shadows.
- Interaction motion should be about 120 ms; surface entry may be about 220 ms.
- Animate opacity, color, and small transforms—not list geometry or email layout.
- Every animation must respect both OS reduced motion and Origami's motion preference.
- Keep focus-visible and forced-colors behavior intact.
- Use plain text labels rather than decorative emoji.

## Security and data-loss invariants

- Never log, document, cache in UI state, or expose credentials/tokens.
- Keep HTML email script-free, sandboxed, sanitized, and CSP-restricted.
- Keep remote images, external CSS, tracking resources, and attachments blocked from automatic download.
- Validate external URLs and open them through the system handler; never navigate the app webview.
- Preserve all physical message sources when deduplicating logical messages.
- Apply irreversible mail actions through guarded commands and retain offline replay/undo semantics.
- Use IMAP `INTERNALDATE`/`received_at` for ordering and recentness, with RFC `Date` fallback.
- Keep cache warming display-only; do not prefetch attachment bytes or remote resources.

## Build and verification

Run from the repository root unless noted:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
cargo build -p origami-app --no-default-features

npm --prefix ui run check
npm --prefix ui test -- --run
npm --prefix ui run build

git diff --check
git status --short --branch
```

Last verified locally on 2026-08-15:

- Rust fmt, strict clippy, workspace check/tests, and app build passed.
- All 25 UI tests passed; Svelte reported 0 errors/0 warnings; the production UI build passed.
- `npm audit --audit-level=moderate` reported 0 vulnerabilities.
- A real-browser CSS check confirmed the 220 ms surface animation, centered toast geometry, and disabled reduced-motion pulse.
- `cargo tauri build --no-bundle --ci` built the optimized native binary at `target/release/origami`.
- `packaging/arch/PKGBUILD` built `origami-0.1.0-1-x86_64.pkg.tar.zst`; checksum, package metadata, archive contents, and desktop entry validation passed on 2026-08-24.

This does not replace remote CI, Docker/live-provider tests, package installation, accessibility/performance gates, or a native daily-driver smoke.

For native release confidence, also build/run the Tauri app on WebKitGTK and smoke account onboarding, cached opening, sync/reconnect, search, compose/send/outbox, tray restore/quit, external links, remote-content controls, keyboard navigation, and both themes.

## Editing rules

- Read `docs/adr/` before changing identity, backend, or sync semantics.
- Keep the UI provider-neutral and the backend trait boundary narrow.
- Prefer root-cause changes in Rust/store/DTO layers over Svelte-side data reconstruction.
- Reuse existing helpers and dependencies; minimize files and abstractions.
- Add one runnable regression check for each parser, query, race, or trust-boundary fix.
- Preserve unrelated dirty-worktree changes. Do not commit unless explicitly requested.

## Known drift

- The plan's architecture sections mix implemented behavior with target-state language; verify symbols before relying on them.
- The plan's UI tree mentions Tailwind and `components/routes/stores`; the current UI uses custom CSS and a flatter `ui/src/lib` structure.
- `packaging/arch/PKGBUILD` and `.gitlab-ci.yml` are tracked; local package validation passed, but the first remote pipeline/release remains pending.
- GitHub CI remains the existing baseline; GitLab CI now watches merge requests, branches, and semantic version tags.
- Release checkboxes must be updated only from real build, provider, package, and native-smoke evidence.
