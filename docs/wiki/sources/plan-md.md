---
title: "Source: docs/PLAN.md"
type: source
status: current
updated: 2026-09-12
sources: [docs/PLAN.md]
---

# Source — `docs/PLAN.md`

**Ingested:** 2026-09-12 · The master plan (264 lines, includes a milestone checklist).

## What it is

Locked technology decisions, a research summary of why this stack, target architecture
diagrams, the data/sync design, UI design, security model, repo layout, testing/quality
mandate, milestones M0–M6, top risks, and a checkbox checklist.

## Locked decisions

Rust core + TypeScript UI; Tauri 2 + Svelte 5 + Vite on webkit2gtk-4.1; pimalaya
`io-imap`/`io-smtp` git-pinned behind our own trait; scope v1 = IMAP+SMTP multi-account,
offline-first sync + FTS5 search, OAuth2 Gmail/Outlook; rich HTML composer; bespoke UI;
AUR PKGBUILD + CI release builds. PGP (v1.1), JMAP/Gmail-API/Graph (v1.2+) are explicitly
out of v1.

## Milestone status in the document

M1–M4 marked complete; M0 complete with CI evidence pending; M5 daily-driver features present
with release gates pending; M6 partially done (Arch package workflow done, AUR package and
v1.0 tag not).

## Caveats — read as intent, not fact

- The architecture prose mixes implemented behavior with target-state language
  ([[known-drift]]).
- The repo-layout block still says `packaging/arch/PKGBUILD` is untracked; it is tracked.
- The UI tree mentions Tailwind and a `components/routes/stores` layout; the implementation
  uses custom CSS and a flat `ui/src/lib`.
- Checkbox state is **not** release evidence ([[release-readiness]]).

## Fed into

[[overview]] · [[runtime-and-layers]] · [[sync-engine]] · [[store-and-search]] ·
[[message-model-and-threading]] · [[content-addressed-store]] · [[protocol-dependencies]] ·
[[release-readiness]] · [[known-drift]]
