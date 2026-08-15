# Origami Improvement Plan

This plan turns the current prototype into a dependable daily-driver mail client. Work is ordered by user impact and by backend dependencies.

## P0 - Correctness and speed

- [x] Open cached bodies without establishing a new IMAP connection.
- [x] Replace folder-wide envelope scans with indexed point lookups.
- [x] Reject stale folder and message responses.
- [x] Coalesce sync-driven folder refreshes.
- [x] Remove the ineffective remote-image banner and rewrite.
- [x] Open validated HTTP, HTTPS, and mailto links in the default system application.
- [x] Guard the main webview against external navigation.
- [x] Repair TipTap editor lifecycle.
- [x] Scope list keyboard shortcuts away from unrelated controls.
- [x] Fetch only display MIME sections before large attachments.
- [x] Cache normalized parsed MIME data.

## P1 - Shell and status polish

- [x] Replace raw sidebar errors with classified, actionable status cards.
- [x] Clear account errors only after a complete successful account sync.
- [x] Add Retry, Settings, and technical-detail actions.
- [x] Make account folders independently scrollable.
- [x] Expose Add Account after onboarding.
- [x] Measure the virtual-list viewport instead of assuming 800 pixels.
- [x] Add mailbox context and message counts above the list.
- [x] Add draggable pane splitters and saved widths.
- [x] Add compact two-pane and focused-reading layouts.
- [ ] Consolidate dialogs, menus, buttons, and status UI into shared primitives.

## P2 - Selection and message actions

- [x] Add exclusive, Ctrl/Cmd toggle, Shift range, keyboard, and select-all-loaded selection.
- [x] Add accessible listbox and multi-selection semantics.
- [x] Add batch read, unread, star, and unstar actions using one IMAP session per folder.
- [x] Queue failed flag changes for offline replay.
- [x] Add automatic paginated loading past the first 200 messages.
- [x] Add archive, move, trash, and permanent-delete commands with offline replay.
- [x] Add junk commands through the provider's Junk mailbox.
- [x] Add label commands with keyword preservation and offline replay.
- [x] Add a six-second optimistic undo window for destructive operations.
- [x] Add explicit select-all-folder/search behavior beyond loaded results.

## P3 - Composition

- [x] Add Reply, Reply All, and Forward entry points.
- [x] Add account-aware sender selection.
- [x] Add debounced local draft autosave and recovery.
- [x] Prevent duplicate sends and show sending state.
- [x] Persist composer drafts and attachments in SQLite.
- [x] Synchronize close-time drafts with the account Drafts mailbox using APPENDUID replacement.
- [ ] Add recipient chips and contact completion.
- [x] Add outgoing file attachments with type, size, removal, and a 25 MB guard.
- [ ] Add inline images, signatures, and plain-text mode.
- [x] Add durable offline SMTP send queue and replay.
- [x] Add reliable Sent-copy retry independent from SMTP delivery.

## P4 - Search and conversations

- [x] Add global search UI.
- [x] Search sender, recipient, and subject before a body is downloaded.
- [x] Include indexed body text when available.
- [x] Make cross-folder results safe for reading, attachments, and batch flags.
- [x] Add structured sender, recipient, subject, state, attachment, label, account, and folder filters.
- [x] Add SQLite-backed saved searches.
- [x] Add result highlighting and search pagination.
- [x] Add normalized-subject conversation grouping, expandable rows, and reading-pane navigation.
- [x] Fetch References/In-Reply-To with envelopes and persist canonical thread roots.
- [ ] Add a dedicated cross-folder thread query independent of loaded pages.
- [x] Add a paginated local unified inbox across accounts.
- [ ] Preserve view state per folder.

## P5 - Customization and motion

- [x] Add persisted System, Light, and Dark themes.
- [x] Add comfortable and compact list density.
- [x] Add System, Full, and Reduced motion preferences.
- [x] Respect the operating system reduced-motion preference.
- [x] Restore text selection and consistent focus-visible styling.
- [ ] Add pane, font scale, message zoom, and mark-read preferences.
- [x] Add notification privacy, quiet-hours, and folder-scope settings.
- [ ] Complete keyboard and screen-reader interaction tests.

## P6 - Mail-system completeness

- [x] Load current account settings before editing without exposing secrets.
- [x] Refresh OAuth access tokens before sync and every 45 minutes, rotating refresh tokens.
- [x] Add provider reauthentication when a refresh token is revoked.
- [x] Reconcile remote folder removal and selection state.
- [x] Reconcile remote message deletion and keyword changes.
- [x] Add protected custom-folder create, rename, and delete workflows.
- [ ] Add folder hierarchy rendering and subscription management.
- [ ] Add manually managed contacts, groups, and import/export.
- [x] Add learned correspondent completion from local message history.
- [x] Add explicit online/sync/error status and pending Outbox operation indicators.
- [x] Add a sanitized Outbox operation inspector and retry controls.
- [ ] Add grouped notifications, click routing, tray, and unread badge.

## P7 - Release quality

- [~] Expand UI component and interaction tests beyond the current focused suite.
- [ ] Add Playwright daily-driver workflows.
- [ ] Add accessibility and visual-regression checks.
- [ ] Add cached-open, sync, search, and 10,000-row performance budgets.
- [~] Expand the current sanitizer/iframe regressions into a hostile-message and navigation corpus.
- [ ] Run Gmail, Outlook, Fastmail, Posteo, and Dovecot release checks.
- [~] Continue reconciling legacy milestone claims with release evidence in `docs/PLAN.md`.
