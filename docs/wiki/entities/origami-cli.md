---
title: origami-cli
type: entity
status: current
updated: 2026-09-12
sources:
  - crates/origami-cli/src/main.rs
  - docs/PLAN.md
---

# origami-cli

A thin debug CLI over `origami-core` — the "dogfooding tool" from [[plan-md]]. It exists so
protocol and store behavior can be exercised without the desktop shell.

Documented smoke commands include `account check`, `folder list`, and `envelope list`
(milestone M1). See `crates/origami-cli/src/main.rs` for the current surface.

## Related

- [[origami-core]] · [[test-suite-md]] · [[build-and-verification]]
