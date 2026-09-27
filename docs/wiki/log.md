---
title: Log
type: log
status: current
updated: 2026-09-19
---

# Log

Append-only chronology. Entries begin with `## [YYYY-MM-DD] <kind> | <title>` so the
file stays greppable: `grep "^## \[" docs/wiki/log.md | tail -5`.

## [2026-09-12] init | Wiki instantiated
Created the wiki at `docs/wiki/` with [[SCHEMA]] as the governing schema and root
`AGENTS.md` as the discovery pointer. Established categories: architecture, decisions,
concepts, entities, status, sources.

## [2026-09-12] migrate | MEMORY.md superseded
Reduced `MEMORY.md` to a pointer. Migrated its verified content into [[overview]],
[[runtime-and-layers]], [[sync-engine]], [[store-and-search]], [[message-model-and-threading]],
[[ui-state-and-rendering]], [[accounts-and-secrets]], [[release-readiness]],
[[build-and-verification]], and [[known-drift]].

## [2026-09-12] ingest | Repository docs (batch)
Ingested `README.md`, `docs/PLAN.md`, `docs/IMPROVEMENT_PLAN.md`, `docs/TEST_SUITE.md`,
`docs/GITLAB_CICD.md`, `docs/adr/0001-0003`, and both `.hermes/plans/` artifacts.
Summary pages filed under `sources/`; distilled into the architecture, decisions,
concepts, and status pages. See each source page for what it fed.

## [2026-09-12] fix | Synchronous prefetch command aborted the app on startup
`prefetch_selected_folder` was a synchronous `#[tauri::command]` while
`SyncEngine::spawn_recent_prefetch` calls `tokio::spawn`. Tauri runs synchronous commands
on the GTK main thread with no runtime, so the panic crossed the FFI callback boundary and
aborted the process (SIGABRT) whenever a folder was selected. Introduced by `59fb84b`
(2026-09-06); unrelated to the icon-first toolbar change. Fixed by making the command
`async` and by having the prefetch skip with a debug log when no runtime is present. New
regression test `crates/origami-core/tests/prefetch_runtime.rs` fails without the guard and
reproduces the exact production panic. Recorded the command-layer rule in [[origami-app]].

## [2026-09-12] change | Message action bar is icon-first
Implemented `.hermes/plans/2026-09-12_231307-message-action-bar-icon-first.md`. Reply, Reply
all, Forward, Archive, Trash, Junk, and Star are now icon-only buttons rendered by the new
`ui/src/lib/ActionIcon.svelte`; the HTML/Text format control and Details keep their text
labels (icons for verbs, text for modes). Every icon button has a constant `aria-label` and
`title`, Star uses `aria-pressed` instead of a label swap, and separators group the
clusters. Updated [[ui-state-and-rendering]] and [[ui-frontend]]. Gates: svelte-check 0
errors/0 warnings, 50 UI tests passing (MessageView 6 → 11), production build OK.

Two layout follow-ups after seeing it live: the subject and its action bar now occupy
**separate full-width rows** (`.subject-row` is a column; the `h1` lost `flex: 1 1 260px`,
which would have become a 260 px *height* basis in a column), and the message sheet width is
now `min(100%, var(--message-measure))` with `--message-measure: 1100px` on `.message`.
The old fixed 760 px cap in six places left the email frame ~700 px of content, so mail wider
than that scrolled horizontally while the pane had room to spare. The `overflow-x: auto`
fallback inside the email document (`messageHtml.ts`) is unchanged and still pinned by test.

## [2026-09-12] lint | First pass
Scripted check over 34 pages: 0 unresolved wikilinks, 0 orphans, 0 missing `sources:`
paths, 0 pages absent from [[index]]. The only flagged items were literal `[[wikilinks]]`
and `[[basename]]` mentioned inside code spans in [[SCHEMA]], which Obsidian does not
resolve as links. Structural files (SCHEMA, index, log) added to the catalog under Meta.
Open items — none blocking; content freshness is bounded by the source dates recorded in
[[build-and-verification]] and [[known-drift]].

## [2026-09-18] ingest | Unread-only list filter
Recorded the sticky thread-list Unread view (`unreadOnly`, `origami-preferences`), store-level
unread paging after logical dedupe, search `is:unread` composition, keep-selected-on-Seen, and
unread row chrome (pip + tint + bold) in [[ui-state-and-rendering]]. No new wiki page.

## [2026-09-19] ingest | Unread-only list filter (complete)
Filed [[unread-only-list-filter]]. Distilled keep-selected (`unreadList.ts`), empty-state
centering (`OrigamiArtwork` + `.empty` grow in a row-flex `.list-body`), store unread paging,
and logical-unread (`merge_envelope_sources`) into [[ui-state-and-rendering]],
[[store-and-search]], [[logical-vs-physical-message]], [[ui-frontend]], and [[overview]].
Recorded the parked Seen-merge residual in [[known-drift]]. Completes the 2026-09-18 note.

## [2026-09-19] ingest | Sidebar folders, tags, and onboarding
Filed [[sidebar-folders-tags-onboarding]]. Distilled nested folder tree and Gmail All Mail
hiding, tag catalog, account/folder menus, trap-focus dialogs, and the app-password wizard
(Gmail/iCloud/Yahoo/Fastmail app passwords; Microsoft OAuth; Google OAuth not offered in the
wizard) into [[ui-state-and-rendering]], [[accounts-and-secrets]], [[ui-frontend]], and
[[overview]]. Gmail OAuth-vs-app-password split recorded in [[known-drift]].

## [2026-09-19] lint | After catch-up ingest
Scripted check over 36 pages: 0 unresolved wikilinks (same SCHEMA/log code-span false
positives as 2026-09-12), 0 orphans, 0 missing `sources:` paths, 0 pages absent from
[[index]].

## [2026-09-22] ingest | CI moved back to GitHub
Repo moved back to GitHub (`github.com/tea-LZL/origami`) from GitLab for larger free CI
compute minutes. `.gitlab-ci.yml` removed; `.github/workflows/ci.yml` now carries the full
pipeline (rust, ui, package_arch in an Arch container; publish_arch uploads the Arch
package + checksum as a GitHub Release for `vX.Y.Z` tags). Source page renamed
[[github-cicd-md]] (was `gitlab-cicd-md`, superseded); claims distilled into
[[build-and-verification]], [[release-readiness]], [[known-drift]], [[overview]], and
[[readme]]. `docs/GITLAB_CICD.md` replaced by `docs/GITHUB_CICD.md`; `PKGBUILD` url and
README install links point at GitHub Releases. Historical excerpts (e.g. the gitlab remote
in [[hermes-arch-local-release]]) left verbatim as record.

## [2026-09-26] update | GitHub move executed, first pipeline green
Repo created and pushed: `github.com/tea-LZL/origami` (public), master + `v0.1.0`.
Commit `cca0328` ("ci(CICD): move CI/CD back to GitHub Actions") carries the migration.
First GitHub Actions run 36269969749 green: ui (1m), rust (6m), package_arch (13m,
artifact `origami-master` 9.2 MB); publish_arch correctly skipped on branch push.
Updated [[known-drift]] Remote CI row, [[release-readiness]], and [[build-and-verification]]
to record the verified run. Tag publish path still unproven. GitLab repo left standing
(archive/delete is a human call).

## [2026-09-27] ingest | Windows NSIS packaging + release asset fix
Release bug root cause: `v0.1.0` tag pointed at pre-workflow commit, so publish never
ran and no GitHub Release existed. Fix approved: force-move `v0.1.0` to new HEAD after
CI green. Ported `ci/windows-nsis` branch work onto master: `@tauri-apps/cli` pin
(cherry-pick 42c0179), Windows-safe harness mail paths (cherry-pick 1a99128 — `:` in
filenames breaks checkout on NTFS), and `package_windows`/`publish` jobs in
`.github/workflows/ci.yml` (unsigned NSIS `*-setup.exe` on `windows-latest`; single
publish job emits 4 release assets). Distilled into [[github-cicd-md]], [[overview]],
[[build-and-verification]]. Windows installer stays unsigned/experimental per the
`ci/windows-nsis` docs stance.

## [2026-09-27] update | v0.1.0 released with 4 assets
Tag `v0.1.0` force-moved to `47b92a2` (was pre-workflow `f892ff9`, never delivered
anything — user approved option A). Tag pipeline 36278995728 green end-to-end:
rust, ui, package_arch, package_windows, publish. GitHub Release v0.1.0 carries
`origami-0.1.0-8-x86_64.pkg.tar.zst` (9.2 MB) + sha256 and
`Origami_0.1.0_x64-setup.exe` (7.4 MB) + sha256. One mid-flight fix: Windows job
pinned rustc 1.85.0 but current lockfile needs >=1.89 (notify-rust) — switched to
stable, matching Arch jobs. [[known-drift]] Remote CI row and [[release-readiness]]
now record the proven tag path.
