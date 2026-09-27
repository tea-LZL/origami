---
title: Build and verification
type: status
status: current
updated: 2026-09-22
sources:
  - MEMORY.md
  - .github/workflows/ci.yml
---

# Build and verification

Run from the repository root unless noted.

## Gates

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
cargo build -p origami-app --no-default-features

npm --prefix ui run check
npm --prefix ui test -- --run
npm --prefix ui run build

git diff --check
git status --short --branch
```

Live protocol tests need Docker and env-gated variables — see [[test-suite-md]].

## Last verified locally

As of the 2026-08-15 verification (and 2026-08-24 package check):

- Rust fmt, strict clippy, workspace check/tests, and app build passed.
- All 25 UI tests passed; Svelte reported 0 errors/0 warnings; production UI build passed.
- `npm audit --audit-level=moderate` reported 0 vulnerabilities.
- A real-browser CSS check confirmed the 220 ms surface animation, centered toast geometry,
  and disabled reduced-motion pulse.
- `cargo tauri build --no-bundle --ci` built the optimized native binary at
  `target/release/origami`.
- `packaging/arch/PKGBUILD` built `origami-0.1.0-1-x86_64.pkg.tar.zst`; checksum, package
  metadata, archive contents, and desktop-entry validation passed on 2026-08-24.

> Treat the dates above as the freshness bound for any release claim. Re-run before relying
> on them; the wiki does not replace CI or a native smoke.

## What this does **not** cover

Docker/live-provider tests, package installation on a user machine, accessibility/performance
gates, and a native daily-driver smoke. Remote CI verified 2026-09-26/27: runs 36269969749
(master green), 36276664146 (Windows package green), 36278995728 (tag `v0.1.0` published —
4 release assets). For native
release confidence, also build/run the
Tauri app on WebKitGTK and smoke: account onboarding, cached opening, sync/reconnect, search,
compose/send/outbox, tray restore/quit, external links, remote-content controls, keyboard
navigation, and both themes.

## CI configuration

- `.github/workflows/ci.yml` — GitHub Actions pipeline: rust, ui, package_arch (Arch
  container, `makepkg`) and package_windows (unsigned NSIS installer on `windows-latest`)
  on pull requests, `master` pushes, and manual dispatch; the publish job uploads four
  assets (Arch package + checksum, Windows installer + checksum) as a GitHub Release for
  `vX.Y.Z` tags ([[github-cicd-md]]). First verified run: 36269969749 (2026-09-26) — ui,
  rust, package_arch green; artifact `origami-master` (9.2 MB) uploaded.

## Palette verification (2026-09-27)

- Automated: `npm test --prefix ui` (91 tests incl. theme resolution, ember warmth,
  folder-role hues, outbox chips), `npm run check --prefix ui` (0 errors),
  `node ui/scripts/check-contrast.mjs` (WCAG AA gate over light/dark/ember token
  pairs — `npm run check:contrast`).
- `forced-colors` paths unchanged by the palette work (app.css + component blocks
  intact); pip/tint rules verified by inspection only.
- **Manual pass still owed** (GUI not runnable in the automated environment): open
  each theme (light/dark/ember) and check sidebar role dots, 12 tag chips, outbox
  chips, unread pip, skeletons, composer, dialogs; flip OS scheme with preference
  "system"; run once under `forced-colors: active`.

## Related

- [[release-readiness]] · [[test-suite-md]] · [[ui-frontend]] · [[origami-core]]
