---
title: Release readiness
type: status
status: current
updated: 2026-09-27
sources:
  - docs/superpowers/specs/2026-09-26-hardening-caching-palette-design.md
  - MEMORY.md
  - docs/IMPROVEMENT_PLAN.md
  - docs/PLAN.md
---

# Release readiness

**Current classification: feature-rich beta / daily-driver candidate — not public-production
ready.**

`docs/IMPROVEMENT_PLAN.md` reports 64 of 81 items complete (79% checklist coverage), but that
measures **feature breadth, not release confidence**. Public readiness is lower because
several proof and distribution gates are absent.

## Blockers

### 1. Distribution and automation
- GitHub Actions CI verifies Rust/UI, builds an Arch package in an Arch container and an
  unsigned Windows NSIS installer on `windows-latest`, and publishes both with checksums as
  GitHub Release assets. `v0.1.0` released 2026-09-27 with all four assets (runs 36269969749,
  36276664146, 36278995728). Windows installer is unsigned/experimental — SmartScreen warns
  ([[github-cicd-md]]). PR #1 (`harden-cache-palette`) ran the full matrix green
  (run 36308544416) including the new `cargo-audit` / `cargo-deny` steps.
- `packaging/arch/PKGBUILD` is tracked and installs the native binary, desktop entry, and
  hicolor icons.
- The Tauri `deb` target remains, while Arch distribution uses the tracked PKGBUILD.
- Signed pacman repository support is deferred until the app matures.
- The manifest declares MIT/Apache-2.0; the license texts are present as `LICENSE-MIT` and
  `LICENSE-APACHE` (root `LICENSE` remains the MIT copy).
- **No v1 tag yet.**

### 2. Provider evidence
- The Gmail / Outlook / Fastmail / Posteo / Dovecot release matrix is not recorded.
- Current real-provider OAuth cannot be revalidated until the revoked/invalid grant is
  reauthorized ([[accounts-and-secrets]]).

### 3. End-to-end confidence
- The GreenMail SMTP round-trip tolerates a known `io-smtp` greeting-parser incompatibility
  ([[test-suite-md]]).
- No Playwright daily-driver suite and no native WebKit automated workflow.
- **Cached-open budget landed** (mean LRU hit < 50 ms,
  `crates/origami-app/tests/open_latency.rs`, `--ignored`); explicit 10,000-row, search, and
  sync budgets are still missing.
- The Docker-harness end-to-end runs added/affected by the hardening track (folder failure
  isolation, poison outbox replay) have not been executed.

### 4. Security and accessibility gates
- **Resolved in PR #1**: existing-data permission smoke
  (`existing_data_perms_tightened`, including WAL sidecars, blob shards, and config, with
  symlink-safe tree tightening); `cargo-audit` + `cargo-deny` policy automated in CI;
  hostile-MIME corpus with depth/header/decode caps; blob-hash traversal gate and
  attachment scoping; link-scheme allowlist tests; secret redaction on errors and outbox
  rows ([[store-and-search]], [[content-addressed-store]],
  [[hardening-caching-palette-spec]]).
- Still open: no navigation fuzz gate; keyboard/focus behavior only partially tested; no
  automated accessibility or visual-regression checks (the palette contrast gate covers
  token contrast only — [[build-and-verification]]).

### 5. Daily-driver gaps
Recipient chips/contact management, signatures, inline images, plain-text compose; folder
hierarchy/subscriptions; cross-folder thread query and per-folder view restoration;
font/message zoom and configurable mark-read; explicit discard/start-clean draft and
search-context/result-count header; grouped notification routing and unread badge behavior.

## Release rule

Do not declare 1.0 until the first four groups have explicit evidence. The fifth group can be
split into required-1.0 items and documented post-1.0 scope.

## Recommended QoL order

1. Preserve folder/search selection and scroll state.
2. Add explicit Discard/Start-clean actions for recovered composer drafts.
3. ~~Label global-search context correctly and show its result count.~~ Delivered
   (branch `search-context-count`, 2026-09-27).
2. ~~Add explicit Discard/Start-clean actions for recovered composer drafts.~~ Delivered
   (PR `composer-draft-discard`, 2026-09-27).
3. Label global-search context correctly and show its result count.
4. ~~Add configurable delayed mark-as-read and message zoom.~~ Delivered (branch
   `markread-zoom`, 2026-09-27).
5. ~~Add recipient chips over the existing learned correspondent data.~~ Delivered (branch
   `recipient-chips`, 2026-09-27).
6. ~~Complete dialog focus entry/trapping/restoration and keyboard interaction tests.~~
   Delivered (branch `qol-batch`, 2026-09-27: Escape handling in `trapFocus`, all dialogs
   wired, Outbox integration test).
7. Add folder hierarchy/subscription support.
8. Add signatures/plain-text compose mode before inline-image editing.
9. Consolidate repeated dialog/button/status patterns only as each flow is touched.

Avoid speculative provider abstractions, new background services, or broad component
rewrites.

## Related

- [[build-and-verification]] · [[known-drift]] · [[improvement-plan-md]] · [[plan-md]]
