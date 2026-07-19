# ADR-0002: Backend trait isolates protocol crates

- Status: Accepted
- Date: 2026-07-19
- Context: pimalaya's `io-imap`/`io-smtp` (0.2.x, git-pinned, alpha) give us
  correct, extension-rich protocol clients (IDLE, CONDSTORE/QRESYNC, UIDPLUS,
  MOVE, SORT/THREAD, OAUTHBEARER/XOAUTH2, SCRAM). But they are pre-1.0: APIs
  will break between revisions. Himalaya itself is binary-only and cannot be
  depended on (its `src/shared/client.rs` is the model for this seam).
- Decision: `origami-core` defines `MailBackend` (read) and `SmtpSender`
  (send) async traits over Origami domain types. Exactly one module,
  `origami_core::imap`, may name `io-imap`/`io-smtp` types; the rest of the
  codebase (sync engine, store, UI) only sees the trait and domain types.
  Dependencies are pinned to git revisions; upgrades happen only via
  deliberate bump commits with the Dovecot test harness green.
- Consequences: Upstream churn touches one module. JMAP/Gmail-API/Graph
  backends can be added as new trait implementations without sync/UI changes.
  Sans-IO internals of io-imap stay an implementation detail (today: blocking
  std clients on `spawn_blocking`; a fully-async adapter can replace them
  later behind the same trait).
