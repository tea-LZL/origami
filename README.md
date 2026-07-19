# Origami

A fast, offline-first desktop email client for Arch Linux (Wayland/Hyprland-first).

See [docs/PLAN.md](docs/PLAN.md) for the master plan.

## Develop

```sh
npm --prefix ui install   # once
cargo tauri dev           # run dev build with hot reload
```

## Build

```sh
cargo tauri build
```

## Layout

- `crates/origami-core` — backend abstraction, sync engine, local store (UI-agnostic)
- `crates/origami-app` — Tauri shell (window, commands, events)
- `crates/origami-cli` — debug CLI
- `ui/` — Svelte 5 + Vite + TypeScript frontend
- `docs/` — plan and ADRs
- `packaging/arch` — AUR PKGBUILD
