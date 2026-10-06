---
title: UI state and rendering
type: architecture
status: current
updated: 2026-10-06
sources:
  - ui/src/lib/stores.svelte.ts
  - ui/src/lib/ThreadList.svelte
  - ui/src/lib/VirtualList.svelte
  - ui/src/lib/searchHighlight.ts
  - ui/src/lib/unreadList.ts
  - ui/src/lib/folderNav.ts
  - ui/src/lib/Sidebar.svelte
  - ui/src/lib/tags.ts
  - ui/src/lib/prefetch.ts
  - ui/src/lib/stores.test.ts
  - ui/scripts/check-contrast.mjs
  - ui/index.html
  - ui/src/lib/trapFocus.ts
  - ui/src/lib/AddAccount.svelte
  - ui/src/lib/OrigamiArtwork.svelte
  - ui/src/lib/messageHtml.ts
  - ui/src/lib/MessageView.svelte
  - ui/src/lib/attachmentOpen.ts
  - ui/src/lib/remoteContent.ts
  - crates/origami-core/src/store.rs
  - crates/origami-app/src/commands.rs
  - ui/package.json
  - docs/IMPROVEMENT_PLAN.md
---

# UI state and rendering

## State

`ui/src/lib/stores.svelte.ts` owns UI state: requests to the shell, optimistic actions, and
persisted UI preferences (themes, density, motion, layout widths, the unread-only list flag,
notification settings). Preferences live in `localStorage` key `origami-preferences`.
`ui/src/lib/api.ts` wraps the typed Tauri commands; `ui/src/lib/types.ts` mirrors the DTOs.
The UI reads normalized DTOs only and must not contain provider-specific logic
([[backend-seam]]).

Supporting modules: `threads.ts` (conversation projection), `tags.ts`, `navigation.ts`,
`folderNav.ts`, `searchHighlight.ts`, `unreadList.ts`, `trapFocus.ts`,
`remoteContent.ts`.

Key components: `App.svelte`, `Sidebar.svelte`, `ThreadList.svelte`, `MessageView.svelte`,
`Composer.svelte`, `Outbox.svelte`, `VirtualList.svelte`, `PaneSplitter.svelte`,
`Preferences.svelte`, `AccountSettings.svelte`, `AddAccount.svelte`,
`ActionIcon.svelte` (inline-SVG action glyphs), `OrigamiArtwork.svelte` (empty/setup art).

Mail list and rendering stay provider-neutral ([[backend-seam]]). Provider-specific copy
is confined to onboarding ([[accounts-and-secrets]]).

## Unread-only list view

`State.unreadOnly` (`boolean`, default `false`) is a sticky **current-view** filter, not a
saved search. `ThreadList.svelte` exposes it as a header **Unread** toggle (`aria-pressed`,
pressed chrome, tooltip swaps between "Show unread only" and "Show all messages"). The
header count uses the mailbox `unread` total (or the sum of Inbox `unread` under unified
Inbox), not the loaded page length.

The flag is restored in `loadPreferences` before the first folder fetch and written by
`savePreferences` / `setUnreadOnly`. Folder snapshots are keyed
`${folderId}:unread|all` so toggling does not flash the other mode's cached page.

Folder and unified-Inbox paging pass `unreadOnly` into
`list_envelopes` / `list_unified_inbox` (`unread_only: Option<bool>` at the command edge,
default false). The store filters **after** `deduplicate_envelopes` and **before** skip/take
in `list_envelopes_in_folders` (`crates/origami-core/src/store.rs`). A logical envelope is
unread when the merged flags lack `Seen` — any retained physical copy unseen, via
`merge_envelope_sources`. Filtering on SQL `flags_json` before dedupe would drop label
copies from `sources` ([[logical-vs-physical-message]]). Search does not use that argument:
`withUnreadToken` appends `is:unread` to the invoke string only, leaving `searchQuery` as
typed, and skips the token if the query already has `is:read` / `is:unread`.

Opening an unread row still flips Seen. `ui/src/lib/unreadList.ts`
(`retainForUnreadFilter`, `applyUnreadListReload`) keeps `selectedEnvelope` /
`selectedMessageIds` in the unread-only list until the selection moves or the filter is
toggled — otherwise first-open auto-Seen would make the filter look broken.

After a folder sync while that row is still selected, `mergeSelectedIntoUnreadPage` can
overwrite a patched Seen extra with a stale unseen `selectedEnvelope`. The open row may
then keep looking unread and remain in the unread-only list after leaving it. See
[[known-drift]].

Empty folders render `OrigamiArtwork` (`variant="empty"`) plus
`No unread messages` / `No messages here`. `.list-body` is a row flex container;
`.empty` grows (`flex: 1`, `width: 100%`) so the artwork and copy center in the Inbox
column rather than shrink-wrapping left.

## Unread row chrome

Unread rows keep bold sender/subject (`.row-content.unread`). `VirtualList` also takes
`isUnread` and tints `.row.unread` (accent mix on `--bg-raised`; hover/active reuse the
existing fills). `ThreadList` draws a 7 px accent **pip** at `left: 26px` (checkbox padding)
plus a screen-reader "Unread" label. The active-row marker remains `.row.active::before` at
`left: 5px` — the pip is not a second rail. Under `forced-colors`, the tint is cleared and
the pip uses `Highlight` with `forced-color-adjust: none`. Compact density only nudges pip
`top`.

## Sidebar folders and tags

`folderNav.ts` builds the account folder tree: special-role folders first (Inbox, Drafts,
Sent, Archive, Junk, Trash), then nested `Other` folders using the IMAP delimiter.
Collapsed parents roll unread/total up from children. Expand state is per-account in
`localStorage` key `origami-sidebar-expanded`. Other-role leaves named All Mail,
Important, or Starred (Gmail namespace stripped) are hidden from the tree; they remain
in the store.

Account context menu: Sync now, Create folder, Settings, Remove account. Folder context
menu applies to **Other-role** folders only (Rename, Delete). Dialogs use `trapFocus`.

The tag catalog lists `app.value.keywords` with hashed palette colors (`tagColor`) and
counts; a click runs search `tag:<name>`. Thread-list chips share the same colors.
Details: [[sidebar-folders-tags-onboarding]].

## Rendering and the trust boundary

`ui/src/lib/messageHtml.ts` builds the isolated message document. Email HTML is sanitized
with **DOMPurify** (a production dependency) and rendered under a restrictive CSP
(`default-src 'none'; img-src data: cid:`). The rules:

- keep HTML email script-free, sandboxed, sanitized, and CSP-restricted;
- block remote images, external CSS, tracking resources, and attachments from automatic
  download; expose per-origin controls via `remoteContent.ts`;
- open a listed attachment on request: images and plain text (`text/plain`, `text/csv`)
  preview in the reading pane from a `data:` URL built only from a MIME token
  (`attachmentOpen.ts`). Other types, and oversized previews, are written under the app
  cache and opened with the system handler (`open_attachment`). The webview download
  attribute is not used. Filenames are reduced to one path segment before the write;
- intercept links, validate external URLs, open them through the **system** handler, and
  never navigate the app webview;
- serve `cid:` through a Tauri custom protocol scoped per message.

## Composer

`Composer.svelte` uses **TipTap** (`@tiptap/core`, `@tiptap/pm`, `@tiptap/starter-kit`),
serialized by `origami-core::compose` into a multipart message. Drafts autosave locally,
are persisted in SQLite, and are replaced in the account Drafts mailbox via APPENDUID.

## Themes and the palette system

Theme preference (`State.theme`) is one of `system | light | dark | ember`; unknown saved
values normalize to `system`. Resolution moved into JS (`resolveTheme`,
`applyThemePreference` in `stores.svelte.ts`): the app always sets a **concrete**
`data-theme` attribute and re-resolves live on OS `prefers-color-scheme` flips, which let
the duplicated `@media (prefers-color-scheme: dark)` CSS block be deleted. A pre-paint
bootstrap in `ui/index.html` resolves the saved preference before first paint so dark-OS
users get no light flash. **Ember** is a warm charcoal/amber dark theme alongside the
paper-light and midnight-blue defaults.

Tokens live in `ui/src/app.css`: surfaces, text, borders, accent scales, status hues
(`--success` / `--warning` / `--info` / `--danger` with `-fg` pairs), and twelve
`--tag-*` hues used by `tags.ts` `TAG_PALETTE` (hash-distributed, stable). Folder roles get
a decorative hue dot (`folderRoleHue` in `folderNav.ts` — Inbox blue, Drafts amber, Sent
teal, Junk rose, Archive violet, Trash muted) beside the always-present text label. Outbox
rows carry text chips: **Pending** (info) and **Failed** (danger), never color alone.

`ui/scripts/check-contrast.mjs` (`npm run check:contrast`) is a WCAG AA gate over every
theme block: body/raised/muted text at 4.5:1, accent/danger/success/warning/info label
pairs and all tag hues at 3:1, with tag hues discovered dynamically so new ones cannot
escape. It caught a real `--fg-muted` failure during the palette work. `forced-colors`
blocks are untouched by theming.

## Predictive and viewport prefetch

`ui/src/lib/prefetch.ts` schedules display prefetch (see
[[display-cache-and-prefetch]] for the Rust side): row **hover** and selection movement
enqueue predictive fetches (150 ms debounce, batched), and `VirtualList` reports its visible
index range via `onVisibleRange` so the viewport plus one neighbor viewport per side is
enqueued (200 ms debounce). Folder switches, unified-inbox switches, searches, and selection
changes cancel pending batches. The prefetcher is a singleton exported from
`stores.svelte.ts`; it only ever calls `api.prefetchDisplay` — the UI contains no
provider logic.

`MessageView.svelte` paints the header (subject, sender, date, snippet) from
`selectedEnvelope` immediately while the body loads — the skeleton is confined to the body
region — and switches to the message DTO when it arrives. Message-dependent controls stay
gated on the loaded message; reply clicks during load are safe no-ops.

## Motion and accessibility

From the project's UI rules ([[memory-md]]): theme tokens define light, dark, ember, and
system resolution together; interaction motion ~120 ms, surface entry ~220 ms; animate opacity,
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

- [[ui-frontend]] · [[store-and-search]] · [[logical-vs-physical-message]] · [[accounts-and-secrets]] · [[unread-only-list-filter]] · [[sidebar-folders-tags-onboarding]] · [[release-readiness]]
