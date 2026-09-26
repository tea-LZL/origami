---
title: "Source: README.md"
type: source
status: current
updated: 2026-09-22
sources: [README.md]
---

# Source — `README.md`

**Ingested:** 2026-09-12 · Repository entry point.

## What it is

Top-level orientation: what Origami is, how to develop and build, how to install the Arch
package, and the crate/directory layout.

## Key facts

- "A fast, offline-first desktop email client for Arch Linux (Wayland/Hyprland-first)."
- Develop: `npm --prefix ui install` once, then `cargo tauri dev`.
- Build: `cargo tauri build`.
- Arch install verifies a published checksum, then `sudo pacman -U` the package; a signed
  pacman repository is deferred until the app matures. GitHub Actions publishes the package
  and checksum as Release assets.
- Layout: `origami-core` (backend/sync/store, UI-agnostic), `origami-app` (Tauri shell),
  `origami-cli` (debug CLI), `ui/` (Svelte 5 + Vite + TS), `docs/` (plan and ADRs),
  `packaging/arch` (PKGBUILD used by GitHub Actions).
- It points readers to `MEMORY.md` (now superseded by this wiki) and `docs/PLAN.md`.

## Fed into

[[overview]] · [[runtime-and-layers]] · [[github-cicd-md]]
