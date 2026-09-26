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
gates, and a native daily-driver smoke. Remote CI ran green for verify + package on
2026-09-26 (GitHub run 36269969749); the tag publish path remains unproven. For native
release confidence, also build/run the
Tauri app on WebKitGTK and smoke: account onboarding, cached opening, sync/reconnect, search,
compose/send/outbox, tray restore/quit, external links, remote-content controls, keyboard
navigation, and both themes.

## CI configuration

- `.github/workflows/ci.yml` — GitHub Actions pipeline: rust, ui, and package_arch jobs
  (Arch container, `makepkg`) on pull requests, `master` pushes, and manual dispatch; the
  publish_arch job uploads the Arch package + checksum as a GitHub Release for `vX.Y.Z`
  tags ([[github-cicd-md]]). First verified run: 36269969749 (2026-09-26) — ui, rust,
  package_arch green; artifact `origami-master` (9.2 MB) uploaded.

## Related

- [[release-readiness]] · [[test-suite-md]] · [[ui-frontend]] · [[origami-core]]
