---
title: "Source: hardening, caching, and palette spec"
type: source
status: current
updated: 2026-09-27
sources:
  - docs/superpowers/specs/2026-09-26-hardening-caching-palette-design.md
  - docs/superpowers/plans/2026-09-26-open-speed-caching.md
  - docs/superpowers/plans/2026-09-26-hardening.md
  - docs/superpowers/plans/2026-09-26-palette.md
---

# Source — hardening, caching, and palette spec

**Ingested:** 2026-09-27 · Landed via PR #1 (`harden-cache-palette`, 39 commits, merge
`7389968`). Spec and the three implementation plans are now tracked under
`docs/superpowers/`.

## What it is

A three-track design produced by a brainstorming pass (2026-09-26):

- **Track A — open-speed caching**: display LRU, priority prefetch queue, predictive and
  viewport prefetch, instant header paint, cache eviction. → [[display-cache-and-prefetch]]
- **Track B — hardening**: security/data-safety gates plus runtime robustness.
- **Track C — palette**: richer, warmer color system with an accessibility gate. →
  [[ui-state-and-rendering]]

## Track B outcomes (distilled)

| Area | What landed | Page |
|---|---|---|
| Permissions | Existing-data `0700`/`0600` tightening smoke (trigger-proof fixture) | [[store-and-search]] |
| Supply chain | `cargo-audit` + `cargo-deny` policy in CI (`deny.toml`), rustls/chacha20 lock bumps | [[build-and-verification]] |
| Secrets | Pattern + resolved-secret redaction on errors, outbox rows, replay | [[accounts-and-secrets]] |
| Hostile MIME | Caps (depth 32, header 64 KiB, attach decode 64 MiB) + in-code corpus, fold-aware depth scan, `catch_unwind` totality | [[store-and-search]] |
| Blob/attachment scoping | 64-hex content-address gate; per-message extraction | [[content-addressed-store]] |
| Link schemes | Shared `allowedLinkHref` allowlist (http/https/mailto), tab/newline obfuscation stripped | [[ui-state-and-rendering]] |
| Sync resilience | `is_transient` classification, ±20 % jittered backoff, reconnect limiter (10-streak → 60 s slow mode); permanent errors never kill the account loop | [[sync-engine]] |
| Outbox poison | Terminal `failed_at` state, per-row Retry, 5-attempt bound for transient failures | [[offline-outbox]] |
| Crash safety | Trigger-proven transactional batches, WAL checkpoint on quit, draft flush | [[store-and-search]] |
| Network hygiene | 60 s `with_network_timeout` wraps, 30 s bounded lock waits, panic audit | [[sync-engine]] |

## Evidence and gaps

- CI run 36308544416 (PR #1) green: rust (fmt/clippy/test/audit/deny), ui, package_arch,
  package_windows; publish skipped (tag-only). Local: 15 Rust test binaries, 91 UI tests,
  contrast gate over 3 themes.
- **Gaps carried forward**: Docker-harness end-to-end for sync failure isolation and
  poison replay unrun; manual GUI/forced-colors pass owed; deferred minors recorded during
  execution (redact edge cases, cosmetic chip copy, viewport test breadth). See
  [[release-readiness]].

## Related

- [[display-cache-and-prefetch]] · [[sync-engine]] · [[store-and-search]] ·
  [[ui-state-and-rendering]] · [[offline-outbox]] · [[build-and-verification]]
