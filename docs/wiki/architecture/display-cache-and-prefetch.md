---
title: Display cache and prefetch
type: architecture
status: current
updated: 2026-09-27
sources:
  - docs/superpowers/specs/2026-09-26-hardening-caching-palette-design.md
  - crates/origami-app/src/display_lru.rs
  - crates/origami-app/src/state.rs
  - crates/origami-app/src/commands.rs
  - crates/origami-core/src/prefetch_queue.rs
  - crates/origami-core/src/store.rs
  - crates/origami-core/src/sync.rs
  - ui/src/lib/prefetch.ts
  - ui/src/lib/MessageView.svelte
  - crates/origami-app/tests/open_latency.rs
---

# Display cache and prefetch

How opening an email paints as fast as possible. Landed in PR #1 (2026-09-27, merge
`7389968`); design in [[hardening-caching-palette-spec]].

## Read path (LRU → SQLite → network)

`get_cached_message` / `get_message` (`crates/origami-app/src/commands.rs`) consult, in
order:

1. **In-memory display LRU** — `crates/origami-app/src/display_lru.rs`, owned by
   `AppState.display_lru`. Key `"{folder_id}:{server_uid}"` (physical source; logical
   dedupe happens upstream). Caps **32 MiB / 300 entries**; byte estimate is the owned
   text + HTML buffers; recency-ordered with LRU eviction.
2. **SQLite parse cache** — the `message_cache` table (`message_id`, `parsed_json`,
   `cached_at`), a **pure cache**: safe to delete wholesale, rebuilt from blobs or the
   server.
3. **Network display fetch** — `ImapBackend::fetch_display_message` (display MIME only,
   no attachment bytes), cached for **every retained physical source** via
   `cache_display_message_sources` so a provider-label copy never refetches.
4. **Full-body fallback** — `ensure_body` (blob + FTS) when display data is unavailable.

Invalidation: message delete/move removes per-source entries; account removal clears the
whole LRU. Stale responses after navigation are rejected via the existing request
generation in `stores.svelte.ts`.

## Prefetch queue

`crates/origami-core/src/prefetch_queue.rs`, hosted by `SyncEngine.prefetch_queue()` and
started lazily from the first `prefetch_display` command (needs a Tokio runtime).

- **Priorities** (lower runs first): `Open` < `Predictive` < `Viewport` < `Background`.
  The UI cannot schedule `Background`.
- **Dedupe** by key (pending + in-flight); **concurrency 3**; worker records success or
  failure into a consecutive-failure streak.
- **Cancel protocol**: `cancel_all` bumps a generation counter; fetch closures compare
  `generation()` after each await and drop stale writes. The dispatcher also re-checks the
  generation after acquiring a permit, so a request popped before a cancel never starts.
- **Kill switch**: 5 consecutive fetch failures disable prefetch until re-enabled; benign
  misses (malformed key, folder/account/message gone, cached already) return `Ok` and never
  count.
- **Account health**: `prefetch_display` and the worker both skip accounts present in the
  error map. Offline is handled by the failure streak; there is no separate connectivity
  probe.
- **Worker body** (`crates/origami-app/src/state.rs::prefetch_display_key`): resolve
  folder → skip if cached → connect → fetch display MIME → generation re-check → cache for
  all physical sources. **Prefetch never writes flags or `Seen`.**

## UI scheduling

`ui/src/lib/prefetch.ts` (`createPrefetcher`, shared instance exported from
`stores.svelte.ts`):

- **hover / focusMove** — predictive prefetch, debounced 150 ms, batched into one
  `api.prefetchDisplay` call.
- **viewport** — `VirtualList` reports its `startIndex`/`endIndex` (including overscan) via
  `onVisibleRange`; ThreadList enqueues the visible window plus one neighbor viewport per
  side, debounced 200 ms.
- **cancel** — folder switch, unified-inbox switch, search, and selection change drop
  pending batches so old rows never fetch.

## Instant paint

`MessageView.svelte` renders the header (subject, sender, date, snippet) from
`selectedEnvelope` while the body loads; the skeleton is confined to the body region and
the header switches to the richer message DTO when it arrives. Message-dependent controls
(HTML/Text toggle, Details panel, reply/forward) remain gated on the loaded message — reply
clicks during load are safe no-ops via the composer's null guard.

## Budgets and eviction

| Concern | Bound |
|---|---|
| Display LRU | 32 MiB / 300 entries |
| SQLite display cache | 2,000 rows or 30 days received-age (`evict_display_cache`, run after each warm batch) |
| Cached-open latency | mean LRU hit < 50 ms — `crates/origami-app/tests/open_latency.rs` (`--ignored`) |
| Background warm | existing 7-day / 500-message / 8 MiB `PREFETCH_MAX_MESSAGE_SIZE` window stays ([[sync-engine]]) |

`message_cache` eviction touches only that table — blobs, FTS rows, and envelopes are
untouched. `evict_display_cache` orders by `COALESCE(received_at, cached_at)` and skips the
newest rows first.

## Related

- [[store-and-search]] · [[sync-engine]] · [[ui-state-and-rendering]] ·
  [[content-addressed-store]] · [[hardening-caching-palette-spec]]
