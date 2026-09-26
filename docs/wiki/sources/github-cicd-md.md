---
title: "Source: docs/GITHUB_CICD.md"
type: source
status: current
updated: 2026-09-22
sources: [docs/GITHUB_CICD.md]
---

# Source — `docs/GITHUB_CICD.md`

**Ingested:** 2026-09-22 · CI/CD and release procedure for GitHub Actions (replaces
`docs/GITLAB_CICD.md`, superseded 2026-09-22 when CI moved back to GitHub).

## What it is

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs Rust and UI checks, then
builds an Arch package with `makepkg` in an Arch Linux container. Pull requests, `master`
pushes, and manual dispatch runs produce CI results and downloadable job artifacts.

## Release facts

- Local reproduction: `cd packaging/arch && makepkg --syncdeps --cleanbuild --force`.
- The recipe runs `npm ci`, builds `ui/`, builds the locked Rust release binary, and installs
  the binary, desktop entry, and hicolor icons.
- Version tags publish the package and checksum as a GitHub Release asset.
- **A tag version must match four files**: `Cargo.toml`, `crates/origami-app/tauri.conf.json`,
  `ui/package.json`, and `packaging/arch/PKGBUILD`. All four report `0.1.0` today.
- Branch runs do not publish; only `vX.Y.Z` tags run the publish job.

## Why GitHub over GitLab

CI builds consume large compute minutes (Rust workspace + Tauri/makepkg in an Arch
container). GitHub's free tier grants more compute minutes than GitLab's, so the
project moved back to GitHub (2026-09-22). GitLab package-registry publishing was
replaced by GitHub Releases.

## Fed into

[[release-readiness]] · [[build-and-verification]] · [[known-drift]] · [[hermes-arch-local-release]]
