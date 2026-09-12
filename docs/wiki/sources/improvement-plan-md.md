---
title: "Source: docs/IMPROVEMENT_PLAN.md"
type: source
status: current
updated: 2026-09-12
sources: [docs/IMPROVEMENT_PLAN.md]
---

# Source — `docs/IMPROVEMENT_PLAN.md`

**Ingested:** 2026-09-12 · The daily-driver backlog, ordered P0–P7 (108 lines).

## What it is

A checkbox backlog ordered by user impact and backend dependency. At ingest it reported
**64 of 81 items complete (79%)**.

## Themes by priority

| Band | Theme |
|---|---|
| P0 | Correctness and speed: cached bodies without new IMAP connections, indexed lookups, stale-response rejection, MIME display-section fetching and caching |
| P1 | Shell/status polish: classified error cards, retry/settings actions, scrollable folders, measured virtual-list viewport, pane splitters |
| P2 | Selection and actions: multi-select, batch flags, pagination, archive/move/trash/junk/labels, optimistic undo, offline replay |
| P3 | Composition: reply/reply-all/forward, drafts, duplicate-send protection, attachments, offline SMTP queue, Sent-copy retry |
| P4 | Search and conversations: global search, structured filters, saved searches, highlighting, conversation grouping, unified inbox |
| P5 | Customization/motion: themes, density, motion preferences, focus styling |
| P6 | Mail-system completeness: settings without exposing secrets, OAuth refresh/reauth, folder and deletion reconciliation, contacts/completion, outbox inspector |
| P7 | Release quality: UI tests, Playwright, accessibility/visual regression, performance budgets, provider matrix, plan/evidence reconciliation |

Open P1–P6 items map directly onto [[release-readiness]] blocker groups 4 and 5.

## Caveat

Checkbox state measures feature breadth, not release confidence. Do not quote the 79% as
readiness.

## Fed into

[[offline-outbox]] · [[ui-state-and-rendering]] · [[accounts-and-secrets]] ·
[[release-readiness]]
