---
title: Accounts and secrets
type: architecture
status: current
updated: 2026-10-03
sources:
  - crates/origami-core/src/config.rs
  - crates/origami-core/src/oauth.rs
  - crates/origami-core/src/provider_hints.rs
  - crates/origami-app/src/oauth_flow.rs
  - crates/origami-app/src/commands.rs
  - crates/origami-core/Cargo.toml
  - ui/src/lib/AddAccount.svelte
  - docs/IMPROVEMENT_PLAN.md
---

# Accounts and secrets

## Configuration

Account configuration is TOML (`crates/origami-core/src/config.rs`). Secrets are never
written into the TOML: the config stores a **secret indirection** with a keyring-backed
variant (`Secret::Keyring { entry }`) and supports an external-command secret for
`pass`-style setups.

## Keyring

Implemented with the `keyring` crate v3 (`sync-secret-service`, plus the native macOS and
Windows backends), which talks to the Secret Service (GNOME Keyring / KWallet). Passwords and
OAuth refresh tokens live there.

> **Drift note:** `docs/PLAN.md` names `oo7`/libsecret as the intended secret store. The
> implemented dependency is the `keyring` crate. See [[known-drift]].

## Onboarding wizard

`AddAccount.svelte` is a three-step dialog (email → configure → done). Domain lookup goes
through `provider_hints` (`crates/origami-core/src/provider_hints.rs`): Gmail, Microsoft,
Yahoo, iCloud, Fastmail, Purelymail, and Proton Bridge defaults.

**Current wizard paths:**

- **Microsoft** — OAuth PKCE is the primary button; password is an alternate.
- **Gmail, iCloud, Yahoo, Fastmail** — app password (`auth: login`). Gmail does not offer
  "Sign in with Google". A rejected Gmail app password gets a dedicated command-layer
  explanation (`classify_login_error`).
- **Purelymail** — account password (`auth: login`, `imap.purelymail.com:993` /
  `smtp.purelymail.com:465`); the wizard shows a hint that an app password is required only
  when Two-Factor Authentication is enabled (regression test
  `provider_hints::tests::purelymail_hints_fill_imap_smtp_login`).
- Unknown domains — manual IMAP/SMTP fields.

On success the wizard selects that account's Inbox and leaves Inbox sync running.
Dialogs use `trapFocus`. Details: [[sidebar-folders-tags-onboarding]].

## OAuth2

`crates/origami-core/src/oauth.rs` plus `crates/origami-app/src/oauth_flow.rs` implement the
PKCE flow for **Google and Microsoft** in the shell:

- a dedicated local-host redirect listener captures the authorization code;
- token exchange uses `reqwest` (rustls) with JSON;
- a token manager refreshes **before sync and every 45 minutes**, rotating
  provider-issued refresh tokens in the keyring;
- when a refresh token is revoked, the account surfaces provider reauthentication.

The wizard currently **uses** Microsoft OAuth and **does not offer** Google OAuth, even
though `oauth.google_client_id` remains in config. See [[known-drift]].

Password auth (PLAIN/LOGIN/SCRAM) is equally supported; SASL uses XOAUTH2/OAUTHBEARER via
`io-imap` ([[backend-seam]]).

## Invariants

Never log, document, cache in UI state, or expose credentials or tokens. Loading account
settings for editing must not reveal secrets. TLS uses rustls with the platform verifier.

## Related

- [[origami-app]] · [[backend-seam]] · [[ui-state-and-rendering]] · [[sidebar-folders-tags-onboarding]] · [[known-drift]]
