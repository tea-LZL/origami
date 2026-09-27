---
title: "Source: unread-only list filter"
type: source
status: current
updated: 2026-09-27
sources:
  - crates/origami-core/src/store.rs
  - crates/origami-core/tests/store.rs
  - crates/origami-app/src/commands.rs
  - ui/src/lib/stores.svelte.ts
  - ui/src/lib/unreadList.ts
  - ui/src/lib/searchHighlight.ts
  - ui/src/lib/ThreadList.svelte
  - ui/src/lib/VirtualList.svelte
---

# Source — unread-only list filter

**Ingested:** 2026-09-19 · Feature landed 2026-09-18 (`621fe99`–`4ec0ea8`,
`776f52e`); empty-state centering `18e8363` on 2026-09-19. A 2026-09-18 wiki
note recorded the first slice without a source page.

## What it is

A sticky **current-view** unread filter on the thread list, not a saved search.
`State.unreadOnly` (default `false`) lives in `localStorage` key
`origami-preferences` and is restored before the first folder fetch.

## Store contract

`list_envelopes_in_folders` / `list_unified_inbox` take `unread_only: bool`.
Commands expose `unread_only: Option<bool>` (default false). The store filters
**after** `deduplicate_envelopes` and **before** skip/take. A logical envelope
is unread when merged flags lack `Seen` — any retained physical copy unseen
(`merge_envelope_sources`). Filtering on SQL `flags_json` before dedupe would
drop label copies from `sources` ([[logical-vs-physical-message]]).

Regression: `crates/origami-core/tests/store.rs`
(`unread_only_list_skips_seen_and_paginates_unread`,
`unread_only_keeps_logical_unread_when_one_copy_is_seen`,
`unread_only_unified_inbox_skips_seen`).

## UI contract

- `ThreadList.svelte` header **Unread** toggle (`aria-pressed`; tooltip
  "Show unread only" / "Show all messages").
- Empty copy: `No unread messages` vs `No messages here`. Artwork is
  `OrigamiArtwork` variant `empty`. `.list-body` is a row flex container;
  `.empty` grows (`flex: 1`, `width: 100%`) so the empty state centers in the
  Inbox column.
- Header count uses the mailbox `unread` total (sum of Inbox `unread` under
  unified Inbox), not the loaded page length.
- Folder snapshots key `${folderId}:unread|all` (`folderViewKey`).
- Search does not pass `unread_only`: `withUnreadToken` appends `is:unread`
  unless the query already has `is:read` / `is:unread`.
- Unread chrome: bold sender/subject, VirtualList `.row.unread` tint, 7 px
  accent pip at `left: 26px`, screen-reader "Unread" label.

## Keep-selected-on-Seen

Opening an unread row still flips Seen. `retainForUnreadFilter` /
`applyUnreadListReload` in `ui/src/lib/unreadList.ts` keep
`selectedEnvelope` / `selectedMessageIds` in the unread-only list until the
selection moves or the filter is toggled.

> **Resolved 2026-09-27.** The stale-selection overwrite above is fixed: the
> optimistic first-open Seen flip now refreshes `selectedEnvelope`, and
> `mergeSelectedIntoUnreadPage` keeps the `previous`-page extra when a (possibly stale)
> selection points at the same id. Regression:
> `keeps_the_patched_seen_row_when_selection_state_is_stale` in
> `ui/src/lib/unreadList.test.ts`.

## Fed into

[[ui-state-and-rendering]] · [[store-and-search]] ·
[[logical-vs-physical-message]] · [[ui-frontend]] · [[overview]] ·
[[known-drift]]
