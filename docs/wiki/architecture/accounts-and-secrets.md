---
title: Accounts and secrets
type: architecture
status: current
updated: 2026-09-12
sources:
  - crates/origami-core/src/config.rs
  - crates/origami-core/src/oauth.rs
  - crates/origami-app/src/oauth_flow.rs
  - crates/origami-core/Cargo.toml
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

## OAuth2

`crates/origami-core/src/oauth.rs` plus `crates/origami-app/src/oauth_flow.rs` implement the
PKCE flow for Google and Microsoft:

- a dedicated local-host redirect listener captures the authorization code;
- token exchange uses `reqwest` (rustls) with JSON;
- a token manager refreshes **before sync and every 45 minutes**, rotating
  provider-issued refresh tokens in the keyring;
- when a refresh token is revoked, the account surfaces provider reauthentication.

Password auth (PLAIN/LOGIN/SCRAM) is equally supported; SASL uses XOAUTH2/OAUTHBEARER via
`io-imap` ([[backend-seam]]).

## Invariants

Never log, document, cache in UI state, or expose credentials or tokens. Loading account
settings for editing must not reveal secrets. TLS uses rustls with the platform verifier.

## Related

- [[origami-app]] · [[backend-seam]] · [[known-drift]]
