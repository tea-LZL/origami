---
title: "Source: MEMORY.md"
type: source
status: superseded
updated: 2026-09-12
sources: [MEMORY.md]
---

# Source — `MEMORY.md` (superseded)

**Ingested:** 2026-09-12 · Former operational snapshot, last updated 2026-08-24.
The file is now reduced to a pointer; this page preserves what it was and where its content
went.

## What it was

A verified operational snapshot for developers and agents: project identity, a repository
index, runtime architecture, current behavior, production readiness, release blockers,
recommended QoL order, UI/motion rules, security and data-loss invariants, build commands,
editing rules, and known drift.

## Migration map

| Content | Now lives in |
|---|---|
| Project identity, stack, current behavior | [[overview]] |
| Repository index, runtime architecture | [[runtime-and-layers]] |
| Dual message identity | [[logical-vs-physical-message]] |
| Production readiness, blockers, QoL order | [[release-readiness]] |
| Build commands, last-verified results | [[build-and-verification]] |
| UI/motion rules | [[ui-state-and-rendering]] |
| Security and data-loss invariants | [[store-and-search]], [[ui-state-and-rendering]], [[offline-outbox]] |
| Editing rules | [[SCHEMA]] |
| Known drift | [[known-drift]] |

## Caveat

The snapshot's line/file counts ("~17,700 lines across 66 files") were true as of
2026-08-24 and have not been re-counted since. Treat dated claims as freshness-bounded.
