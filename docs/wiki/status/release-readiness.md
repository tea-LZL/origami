---
title: Release readiness
type: status
status: current
updated: 2026-09-12
sources:
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
- GitLab CI verifies Rust/UI, builds an Arch package in an Arch container, and publishes
  tagged package/checksum artifacts — **the remote pipeline is not yet verified**.
- `packaging/arch/PKGBUILD` is tracked and installs the native binary, desktop entry, and
  hicolor icons.
- The Tauri `deb` target remains, while Arch distribution uses the tracked PKGBUILD.
- Signed pacman repository support is deferred until the app matures.
- The manifest declares MIT/Apache-2.0, but **license text files are absent**.
- **No v1 tag yet.**

### 2. Provider evidence
- The Gmail / Outlook / Fastmail / Posteo / Dovecot release matrix is not recorded.
- Current real-provider OAuth cannot be revalidated until the revoked/invalid grant is
  reauthorized ([[accounts-and-secrets]]).

### 3. End-to-end confidence
- The GreenMail SMTP round-trip tolerates a known `io-smtp` greeting-parser incompatibility
  ([[test-suite-md]]).
- No Playwright daily-driver suite and no native WebKit automated workflow.
- No explicit 10,000-row, cached-open, search, or sync performance budgets.

### 4. Security and accessibility gates
- An audit found existing data/config directories at `0755` and mail/config/database files
  at `0644`; this worktree enforces `0700` roots plus `0600` config/database/new-blob files
  on the next open/write, but **an existing-data native smoke is still required**
  ([[store-and-search]]).
- The npm dependency tree is audit-clean in this worktree; no `cargo-audit`/`cargo-deny`
  policy is installed or automated yet.
- Sanitizer/component tests exist, but no broad hostile-message corpus or navigation fuzz
  gate; keyboard/focus behavior is only partially tested.
- No automated accessibility or visual-regression checks.

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
3. Label global-search context correctly and show its result count.
4. Add configurable delayed mark-as-read and message zoom.
5. Add recipient chips over the existing learned correspondent data.
6. Complete dialog focus entry/trapping/restoration and keyboard interaction tests.
7. Add folder hierarchy/subscription support.
8. Add signatures/plain-text compose mode before inline-image editing.
9. Consolidate repeated dialog/button/status patterns only as each flow is touched.

Avoid speculative provider abstractions, new background services, or broad component
rewrites.

## Related

- [[build-and-verification]] · [[known-drift]] · [[improvement-plan-md]] · [[plan-md]]
