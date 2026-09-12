---
title: UI state and rendering
type: architecture
status: current
updated: 2026-09-12
sources:
  - ui/src/lib/stores.svelte.ts
  - ui/src/lib/messageHtml.ts
  - ui/src/lib/remoteContent.ts
  - ui/package.json
  - docs/IMPROVEMENT_PLAN.md
---

# UI state and rendering

## State

`ui/src/lib/stores.svelte.ts` owns UI state: requests to the shell, optimistic actions, and
persisted UI preferences (themes, density, motion, layout widths, notification settings).
`ui/src/lib/api.ts` wraps the typed Tauri commands; `ui/src/lib/types.ts` mirrors the DTOs.
The UI reads normalized DTOs only and must not contain provider-specific logic
([[backend-seam]]).

Supporting modules: `threads.ts` (conversation projection), `tags.ts`, `navigation.ts`,
`folderNav.ts`, `searchHighlight.ts`, `trapFocus.ts`, `remoteContent.ts`.

Key components: `App.svelte`, `Sidebar.svelte`, `ThreadList.svelte`, `MessageView.svelte`,
`Composer.svelte`, `Outbox.svelte`, `VirtualList.svelte`, `PaneSplitter.svelte`,
`Preferences.svelte`, `AccountSettings.svelte`, `AddAccount.svelte`,
`ActionIcon.svelte` (inline-SVG action glyphs).

## Rendering and the trust boundary

`ui/src/lib/messageHtml.ts` builds the isolated message document. Email HTML is sanitized
with **DOMPurify** (a production dependency) and rendered under a restrictive CSP
(`default-src 'none'; img-src data: cid:`). The rules:

- keep HTML email script-free, sandboxed, sanitized, and CSP-restricted;
- block remote images, external CSS, tracking resources, and attachments from automatic
  download; expose per-origin controls via `remoteContent.ts`;
- intercept links, validate external URLs, open them through the **system** handler, and
  never navigate the app webview;
- serve `cid:` through a Tauri custom protocol scoped per message.

## Composer

`Composer.svelte` uses **TipTap** (`@tiptap/core`, `@tiptap/pm`, `@tiptap/starter-kit`),
serialized by `origami-core::compose` into a multipart message. Drafts autosave locally,
are persisted in SQLite, and are replaced in the account Drafts mailbox via APPENDUID.

## Motion and accessibility

From the project's UI rules ([[memory-md]]): theme tokens define light, dark, and
system-dark together; interaction motion ~120 ms, surface entry ~220 ms; animate opacity,
color, and small transforms — not list geometry or email layout; every animation respects
both OS reduced motion and Origami's motion preference; keep focus-visible and
forced-colors behavior intact; plain text labels over decorative emoji.

## Message action bar

The message toolbar (`MessageView.svelte`) is **icon-first** and occupies its **own
full-width row** beneath the subject — the subject takes the whole row above it, so
neither wraps into the other. Reply, Reply all, Forward, Archive, Trash, Junk, and Star
render as 30 px icon buttons drawn by
`ui/src/lib/ActionIcon.svelte` (inline SVG, no icon dependency). The rule applied is
**icons for verbs, text for modes** — the HTML/Text format control and the Details toggle keep
their short text labels, because no glyph conveys them unambiguously.

Density is not bought with discoverability or state:

- every icon button carries a constant `aria-label` plus a matching `title` tooltip;
- Star's accessible name is always "Star"; its state is exposed through `aria-pressed` and a
  filled glyph instead of a label swap;
- Details and the format buttons also expose `aria-pressed`;
- thin separators group view controls, respond, file, and state;
- `flex-wrap: wrap` remains the narrow-width fallback.

Command verbs are unchanged: the icons call the same `openReplyComposer`,
`moveSelectedToRole`, and `setSelectedFlag` handlers as before. Coverage lives in
`ui/src/lib/MessageView.test.ts` — accessible name and tooltip presence, handler routing,
`aria-pressed` transitions, and that Details and the format control remain operable.

The thread-list bulk bar (`ThreadList.svelte`) still uses text chips; adopting
`ActionIcon` there is a known follow-up, not part of this change.

## Related

- [[ui-frontend]] · [[accounts-and-secrets]] · [[release-readiness]]
