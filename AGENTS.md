# Agent instructions — Origami

This repository maintains an LLM-owned project wiki at `docs/wiki/`.

**Before working on this repository's knowledge, read `docs/wiki/SCHEMA.md`.** It
defines the wiki's layout, page conventions, source precedence, and the
ingest / query / lint workflows. Start retrieval at `docs/wiki/index.md`.

Quick orientation:

- `docs/wiki/overview.md` — current state of the project.
- `docs/wiki/architecture/` — how the system works today.
- `docs/wiki/decisions/` — why it works that way (ADRs, cross-linked).
- `docs/wiki/status/release-readiness.md` — what stands between here and a release.
- `MEMORY.md` — superseded by the wiki; now only a pointer.

Project working rules (also in the schema): read `docs/adr/` before changing identity,
backend, or sync semantics; keep the UI provider-neutral; prefer root-cause Rust/store
fixes over Svelte-side data reconstruction; add one runnable regression check per fix;
preserve unrelated dirty-worktree changes; do not commit unless explicitly requested.
