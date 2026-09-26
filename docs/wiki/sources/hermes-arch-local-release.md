---
title: "Source: .hermes Arch local release plan"
type: source
status: current
updated: 2026-09-22
sources: [".hermes/plans/2026-08-18_225754-origami-arch-local-release.md"]
---

# Source — Arch local release (PKGBUILD + pacman install) plan

**Ingested:** 2026-09-12 · Execution plan, 2026-08-18.

## What it is

The plan that produced `packaging/arch/PKGBUILD` and `packaging/arch/release-local.sh` — build
Origami as an Arch package from the enclosing git tree and install it locally with `pacman`.

## Task structure

1. Release gate check.
2. Create `packaging/arch/PKGBUILD` (builds from the repo; installs binary, desktop entry,
   hicolor icons).
3. Create `packaging/arch/release-local.sh` (`--direct` uses `pacman -U`; otherwise builds a
   local repo and installs via `pacman -S origami`).
4. Build and install the package.
5. Verify the installed package.
6. Release bookkeeping **only with explicit user request**.
7. Install instructions, tests/validation summary, risks/open questions, execution handoff.

## Verbatim excerpt — context and approach

```text
## Current context / assumptions

- Project: `/home/tea/Projects/Origami`, git remote `git@gitlab.com:tea-LZL/origami.git`, branch `master`. Version `0.1.0` (workspace `Cargo.toml` + `tauri.conf.json` agree).
- **`packaging/arch/` is empty** — MEMORY.md explicitly lists "no `PKGBUILD`" and "Tauri targets `deb`, not an Arch package" as release blockers (MEMORY.md lines 93–95). This plan closes those.
- Built binary exists at `target/release/origami` (18.6 MB, built 2026-08-15) — the PKGBUILD rebuilds it from source; the existing `target/` dir makes that incremental/fast.
- Verified runtime linkage (ldd): `webkit2gtk-4.1`, `gtk3`, `libsoup3`, `javascriptcoregtk-4.1`, `libsecret-1`, `dbus-1`. Tray (`tray-icon`) dlopens `libayatana-appindicator` at runtime → include in `depends`.
- Assets ready: `assets/origami.desktop` (`Exec=origami %u`, `Icon=origami`, `MimeType=x-scheme-handler/mailto`), `assets/origami-bird.svg`, icons in `crates/origami-app/icons/` (32/64/128 px PNGs + 512×512 `icon.png`).
- Tooling present on this machine: `cargo`, `cargo-tauri 2.11.1`, `node 26.4.0`/`npm` (mise), `makepkg`, `repo-add`, `pacman`, `desktop-file-validate`, `gtk-launch`, `gio`, `install`.
- `/etc/pacman.conf` already contains `SigLevel = Optional TrustAll` sections (1password-beta pattern) — a local-repo entry follows the same pattern. **Editing `/etc/pacman.conf` requires sudo and explicit user confirmation at execution time (user rule: never run system-affecting commands without confirmation).**
- Data/config dirs already exist from development runs (`~/.local/share/dev.origami.mail`) — package install is additive; no migration concerns for a first release.
- Out of scope (YAGNI): AUR upload, AppImage/deb targets, CI release workflow, signing keys, `origami-cli` packaging, version bump. `tauri.conf.json`'s `bundle.targets: ["deb"]` is left untouched — it is irrelevant to the pacman path.
- Assumption: install target is this machine (`x86_64`). `pkgrel` bumps cover future rebuilds of 0.1.0.

---

```

## Verbatim excerpt — risks, tradeoffs, open questions

```text
## Risks, tradeoffs, open questions

- **System-file edit:** `release-local.sh` appends to `/etc/pacman.conf` and runs sudo pacman — blocked on explicit user confirmation per standing rule. `--direct` (`pacman -U`) avoids repo registration entirely and is the minimal-risk fallback.
- **Rebuild cost:** `npm install` + `cargo build` run inside makepkg each time (a few minutes warm). Caches (`target/`, npm cache) are reused via `CARGO_TARGET_DIR`; a clean checkout rebuilds from scratch.
- **Reinstall bumps:** `repo-add` replaces the same pkgver only after `pkgrel`/`pkgver` bump; `--direct` reinstall works regardless. Intentional — no silent version tricks.
- **Tray visibility:** depends on `libayatana-appindicator` being dlopen'd; on Hyprland without a StatusNotifier host the tray may not show regardless of packaging. Non-blocking for installation; note in release notes.
- **WebKitGTK is X11-based** (Tauri 2 on Linux): the app runs under XWayland on Hyprland. Normal for WebKitGTK; not a packaging concern.
- **`npm ci` vs `npm install`:** PKGBUILD uses `install` for lockfile robustness; if `ui/package-lock.json` is missing, install still works (vendored versions from package.json ranges).
- **Open question (low stakes):** include `origami-cli` as a second package? Default decision: no — it is a debug/dogfooding surface, not a release artifact.
- **No signing:** package and local repo are unsigned (`SigLevel = Optional TrustAll`), consistent with local personal use. AUR publication later would need GPG signing — out of scope.

---

## Execution handoff

Plan complete and saved. Ready to execute using subagent-driven-development — I'll dispatch a fresh subagent per task with two-stage review (spec compliance then code quality), pausing at Task 4 Step 3 for explicit confirmation on the `/etc/pacman.conf` + sudo change. Shall I proceed?
```

## Fed into

[[github-cicd-md]] · [[build-and-verification]] · [[release-readiness]] · [[known-drift]]
