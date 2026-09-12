---
title: "Source: docs/GITLAB_CICD.md"
type: source
status: current
updated: 2026-09-12
sources: [docs/GITLAB_CICD.md]
---

# Source — `docs/GITLAB_CICD.md`

**Ingested:** 2026-09-12 · CI/CD and release procedure for GitLab (32 lines).

## What it is

The GitLab pipeline runs Rust and UI checks, then builds an Arch package with `makepkg` in an
Arch container. Merge requests and branch pushes produce CI results and downloadable job
artifacts.

## Release facts

- Local reproduction: `cd packaging/arch && makepkg --syncdeps --cleanbuild --force`.
- The recipe runs `npm ci`, builds `ui/`, builds the locked Rust release binary, and installs
  the binary, desktop entry, and hicolor icons.
- Version tags publish the package and checksum to GitLab's Package Registry.
- **A tag version must match four files**: `Cargo.toml`, `crates/origami-app/tauri.conf.json`,
  `ui/package.json`, and `packaging/arch/PKGBUILD`. All four report `0.1.0` today.
- Branch runs do not publish; only `vX.Y.Z` tags run the publish stage.

## Fed into

[[release-readiness]] · [[build-and-verification]] · [[known-drift]] · [[hermes-arch-local-release]]
