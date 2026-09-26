---
title: Known drift
type: status
status: current
updated: 2026-09-22
sources:
  - MEMORY.md
  - docs/PLAN.md
  - crates/origami-core/Cargo.toml
  - ui/package.json
---

# Known drift

Places where plans and the code disagree, or where a claim needs re-verification before it is
trusted. Precedence is defined in [[SCHEMA]]: code wins for *current*, plans win for
*intended*, and the disagreement is recorded here.

| Topic | Plan / doc says | Code / reality | Handling |
|---|---|---|---|
| Architecture prose | `docs/PLAN.md` architecture sections mix implemented behavior with target-state language | Verify symbols against source before relying | Treat plan architecture text as **target state** unless a page in `architecture/` confirms it |
| UI stack | Plan UI tree mentions Tailwind and `components/routes/stores` | Custom CSS tokens in `ui/src/app.css`; flat `ui/src/lib` structure; no Tailwind dependency | [[ui-frontend]] is authoritative |
| Secret store | Plan names `oo7`/libsecret | Implemented with the `keyring` crate v3 (`sync-secret-service`) | [[accounts-and-secrets]] records the actual dependency |
| Repo layout | Plan's layout block says `packaging/arch/PKGBUILD` is "not tracked yet" | `packaging/arch/PKGBUILD` **is** tracked | [[github-cicd-md]], [[hermes-arch-local-release]] |
| Remote CI | "Remote pipeline is not yet verified" | GitHub Actions CI exists (moved back from GitLab 2026-09-22 for larger free compute minutes); first remote pipeline/release still pending | [[release-readiness]], [[github-cicd-md]] |
| Release checkboxes | `docs/PLAN.md` and `docs/IMPROVEMENT_PLAN.md` checkboxes | Checkbox state is **not** release evidence | Only real build/provider/package/native-smoke evidence updates release status |
| File permissions | Audit found `0755` dirs and `0644` files | Worktree enforces `0700`/`0600` on next open/write; existing-data smoke outstanding | [[release-readiness]], [[store-and-search]] |
| SMTP E2E | Send/receive E2E expected | GreenMail round-trip tolerates an `io-smtp` greeting bug via diagnostic skip | [[test-suite-md]], [[backend-seam]] |
| Version surfaces | Tag version must match four files | `Cargo.toml`, `tauri.conf.json`, `ui/package.json`, `PKGBUILD` all report `0.1.0` today | [[github-cicd-md]] |
| Gmail onboarding | Overview previously listed "Gmail/Microsoft OAuth"; `provider_hints` still maps Gmail to `Xoauth2`; config still has `oauth.google_client_id` | The add-account wizard uses a Gmail **app password** and does not offer Google OAuth. Microsoft OAuth remains the wizard's OAuth path. | [[accounts-and-secrets]], [[sidebar-folders-tags-onboarding]] |

## Open follow-ups

- `README.md` still reads "See `MEMORY.md` for the verified project snapshot"; `MEMORY.md`
  is now a pointer into this wiki. Update the README link when convenient — a human decision,
  since README is a source file.
- The "~17,700 lines across 66 files" figure in [[memory-md]] is dated 2026-08-24; re-count
  before quoting it.
- Verify the Sent-projection and tray behavior against current source before relying on
  [[hermes-sent-tray-background]] as a description of *current* behavior.
- After a folder sync while a just-opened message is selected, `mergeSelectedIntoUnreadPage`
  (`ui/src/lib/unreadList.ts`) can overwrite a patched Seen previous-page envelope with a
  stale unseen `selectedEnvelope`. The open row may keep looking unread and stay in the
  unread-only list after the selection moves. Intended fix: do not overwrite `extrasById`
  with `selectedEnvelope` if previous already has the id; also patch `selectedEnvelope`
  flags on first-open Seen. Product code, not a wiki edit — [[unread-only-list-filter]].

## Standing instruction

When code and a plan disagree, do not "fix" the code to match the plan silently. Record the
conflict here and in the affected page, then decide with the human which side is intended.

## Related

- [[SCHEMA]] · [[release-readiness]] · [[overview]]
