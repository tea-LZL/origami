---
title: "Source: sidebar folders, tags, and onboarding"
type: source
status: current
updated: 2026-09-19
sources:
  - ui/src/lib/folderNav.ts
  - ui/src/lib/Sidebar.svelte
  - ui/src/lib/tags.ts
  - ui/src/lib/trapFocus.ts
  - ui/src/lib/AddAccount.svelte
  - ui/src/lib/AddAccount.test.ts
  - crates/origami-core/src/provider_hints.rs
  - crates/origami-app/src/commands.rs
---

# Source — sidebar folders, tags, and onboarding

**Ingested:** 2026-09-19 · Work from 2026-09-06 through 2026-09-13 that the
wiki named as modules but did not distill: nested folders, Gmail All Mail
hiding, tag catalog, account/folder menus, trap-focus dialogs, and the
app-password onboarding wizard.

## Folder tree

`ui/src/lib/folderNav.ts` builds the sidebar tree:

- Special-role folders first, in order Inbox, Drafts, Sent, Archive, Junk,
  Trash (one row per role).
- Remaining `Other` folders nest by IMAP delimiter (`/` vs `.`, inferred from
  names). Virtual parent rows exist when a path segment has no mailbox.
- Collapsed parents roll unread/total up from children. Expand state is
  per-account in `localStorage` key `origami-sidebar-expanded`.
- Hidden other-role leaves matching `all mail|important|starred` (Gmail
  namespace stripped). Those mailboxes still exist in the store; they are not
  listed.

`Sidebar.svelte` renders the tree (`role="tree"` / `treeitem`), with twist
buttons on expandable rows.

## Context menus

Account menu (header `•••` or right-click): Sync now, Create folder…,
Settings…, Remove account. Folder menu is **Other-role only**: Rename…,
Delete folder. Create/rename/delete talk to `api.createFolder` /
`renameFolder` / `deleteFolder`. Delete warns that the server folder and its
messages are permanently removed. Remove-account deletes saved credentials
and does not clear the local database.

Dialogs use `trapFocus` (`ui/src/lib/trapFocus.ts`): Tab cycles inside the
dialog; destroy restores the previously focused element.

## Tag catalog

When `app.value.keywords` is non-empty, the sidebar lists tags with a hashed
palette color (`tagColor` in `ui/src/lib/tags.ts`) and a count. Clicking a
tag runs `selectKeyword` → search `tag:<name>`. Thread-list chips use the
same colors. Creating a tag from the thread list is a `New tag` field there,
not in the catalog.

## Onboarding wizard

`AddAccount.svelte` is a three-step dialog: email → configure → done.

1. `provider_hints` (`crates/origami-core/src/provider_hints.rs`) fills IMAP/
   SMTP defaults from the domain (Gmail, Microsoft, Yahoo, iCloud, Fastmail,
   Proton Bridge).
2. **Gmail, iCloud, Yahoo, Fastmail** use app passwords (`auth: login`).
   Gmail does **not** offer "Sign in with Google"; the test
   `uses an app password for Gmail and does not offer Google OAuth` pins that.
   Failed Gmail app-password logins get a specific command-layer message
   (`classify_login_error` in `crates/origami-app/src/commands.rs`).
3. **Microsoft** offers OAuth PKCE as the primary path, with password as
   alternate.
4. On success, the wizard selects that account's Inbox and leaves sync
   running in the background.

Google OAuth client IDs still exist in config/shell. The wizard simply does
not use them. See [[accounts-and-secrets]] and [[known-drift]].

Provider-specific copy lives in onboarding only. Mail list and rendering stay
provider-neutral ([[backend-seam]]).

## Fed into

[[ui-state-and-rendering]] · [[accounts-and-secrets]] · [[ui-frontend]] ·
[[overview]] · [[known-drift]]
