---
title: ui frontend
type: entity
status: current
updated: 2026-09-19
sources:
  - ui/package.json
  - ui/src/App.svelte
  - ui/src/lib/stores.svelte.ts
---

# ui frontend

Svelte 5 (runes) + TypeScript + Vite, under `ui/`. No Tailwind: styling is a custom CSS token
system in `ui/src/app.css` (this contradicts an older plan tree — see [[known-drift]]).

## Dependencies

Runtime: `@tauri-apps/api`, `@tauri-apps/plugin-opener`, `dompurify`.
Rich text: TipTap (`@tiptap/core`, `@tiptap/pm`, `@tiptap/starter-kit`, currently declared
under devDependencies).
Tooling: `vite`, `svelte-check`, `typescript`, `vitest` + Testing Library + jsdom.

## Structure

State and helpers in `ui/src/lib/` (`stores.svelte.ts`, `api.ts`, `types.ts`, `threads.ts`,
`tags.ts`, `navigation.ts`, `folderNav.ts`, `searchHighlight.ts`, `unreadList.ts`,
`trapFocus.ts`, `remoteContent.ts`, `messageHtml.ts`); components in the same folder
(`App.svelte` lives at `ui/src/App.svelte`), including `OrigamiArtwork.svelte` for empty
and setup art. Tests are colocated as `*.test.ts` with harnesses in `ui/src/test/`.

## Gates

`npm --prefix ui run check`, `npm --prefix ui test -- --run`, `npm --prefix ui run build`
([[build-and-verification]]).

## Related

- [[ui-state-and-rendering]] · [[message-model-and-threading]] · [[unread-only-list-filter]] · [[sidebar-folders-tags-onboarding]] · [[origami-app]]
