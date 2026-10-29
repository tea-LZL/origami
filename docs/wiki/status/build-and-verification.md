---
title: Build and verification
type: status
status: current
updated: 2026-10-30
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
cargo audit && cargo deny check
cargo build -p origami-app --no-default-features

npm --prefix ui run check
npm --prefix ui test -- --run
npm --prefix ui run build
npm --prefix ui run test:e2e
npm --prefix ui run check:contrast

git diff --check
git status --short --branch
```

Live protocol tests need Docker and env-gated variables — see [[test-suite-md]].

The Playwright suite (`ui/e2e`) builds and previews the UI itself. Its Tauri bridge is
stubbed with `page.addInitScript` (`e2e/tauri.ts`), which serves scenario fixtures and
records every invoke, so the workflows exercise the real components without a Rust process.
Install the browser once with `npm --prefix ui run test:e2e:install`; CI runs the suite in
the `ui-e2e` job against the Arch `chromium` package
(`PLAYWRIGHT_CHROMIUM_EXECUTABLE=/usr/bin/chromium`). Vitest is scoped to `src/`
(`vitest.config.ts`) so the shared `.spec.ts` suffix stays out of `npm test`.
`a11y.spec.ts` runs axe over the message list in light, dark, and ember, plus Preferences and
Compose in the light theme; `keyboard.spec.ts` covers list navigation, Escape, and the dialog
focus trap.

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
4 release assets). The same tag pipeline published `v0.5.0` on 2026-10-29 in run 40931739272,
`v0.5.1` in run 40935229136, and `v0.6.0` in run 40963935760 on 2026-10-29T22:23Z (Arch
package 9.9 MB + unsigned NSIS installer 8.0 MB, each with sha256).
For native
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
  rust, package_arch green; artifact `origami-master` (9.2 MB) uploaded. Latest verified
  tag run: 37146060642 (`v0.2.0`, 2026-10-03) — package_arch, package_windows, publish
  green; master pushes skip rust/ui/package jobs by design (`if:` gates on PR/tag refs).

## Palette verification (2026-09-27)

- Automated: `npm test --prefix ui` (91 tests incl. theme resolution, ember warmth,
  folder-role hues, outbox chips), `npm run check --prefix ui` (0 errors),
  `node ui/scripts/check-contrast.mjs` (WCAG AA gate over light/dark/ember token
  pairs — `npm run check:contrast`). The gate now covers five themes (adding cyberpunk and
  synthwave); axe scans cover the message list in all five.
- `forced-colors` paths unchanged by the palette work (app.css + component blocks
  intact); pip/tint rules verified by inspection only.
- **Manual pass still owed** (GUI not runnable in the automated environment): open
  each theme (light/dark/ember) and check sidebar role dots, 12 tag chips, outbox
  chips, unread pip, skeletons, composer, dialogs; flip OS scheme with preference
  "system"; run once under `forced-colors: active`.

## PR #1 verification (harden-cache-palette, 2026-09-27)

Merged as `7389968` (39 commits). Local runs on the branch: `cargo test --workspace
--locked` (15 test binaries), `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo deny check`, `cargo audit`, `npm test --prefix ui` (91), `npm run check --prefix ui`,
contrast gate (3 themes). CI run 36308544416 green on all four jobs (rust incl. the new
audit/deny steps, ui, package_arch, package_windows); `publish` skipped (tag-only). A
pre-fix fmt failure (run 36308199291) is why the branch carries a dedicated `cargo fmt`
commit.

Cached-open budget check: `cargo test -p origami-app --test open_latency -- --ignored`
(mean LRU hit well under the 50 ms budget).

## Related

- [[release-readiness]] · [[test-suite-md]] · [[ui-frontend]] · [[origami-core]]
