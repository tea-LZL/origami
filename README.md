# Origami

A fast, offline-first desktop email client for Arch Linux (Wayland/Hyprland-first).

See [MEMORY.md](MEMORY.md) for the verified project snapshot and
[docs/PLAN.md](docs/PLAN.md) for the long-term plan.

## Develop

```sh
npm --prefix ui install   # once
cargo tauri dev           # run dev build with hot reload
```

## Build

```sh
cargo tauri build
```

## Install (Arch)

GitLab publishes an Arch package and checksum for version tags. Download both
files from the project's [Package Registry](https://gitlab.com/tea-LZL/origami/-/packages),
then install with:

```sh
sha256sum -c origami-<version>-<pkgrel>-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./origami-<version>-<pkgrel>-x86_64.pkg.tar.zst
```

Package dependencies come from the official Arch repositories. A signed pacman
repository is deferred until the application is more mature.

## Layout

- `crates/origami-core` — backend abstraction, sync engine, local store (UI-agnostic)
- `crates/origami-app` — Tauri shell (window, commands, events)
- `crates/origami-cli` — debug CLI
- `ui/` — Svelte 5 + Vite + TypeScript frontend
- `docs/` — plan and ADRs
- `packaging/arch` — Arch PKGBUILD used by GitLab CI
