# ADR-0001: Origami owns all primary keys

- Status: Accepted
- Date: 2026-07-19
- Context: Local storage needs primary keys for messages. IMAP provides UIDs,
  but they are only unique within (folder, UIDVALIDITY), can be renumbered after
  a UIDVALIDITY flip, and the same message can exist in multiple folders (Gmail
  labels, RFC 8474 OBJECTID). Thunderbird's Panorama ADR-0002 documents how
  using server UIDs as local keys corrupted their whole data layer.
- Decision: Every local entity (account, mailbox, message, thread, attachment)
  gets an app-owned UUIDv7 primary key. Server identifiers (IMAP UID,
  UIDVALIDITY, Message-ID header, RFC 8474 OBJECTID) are stored as plain
  attributes with uniqueness enforced only where the protocol guarantees it
  ((mailbox_id, server_uid) is unique per sync_state generation).
- Consequences: Sync maps server→local keys on every pull via
  `sync_state`/`messages(server_uid)` lookups; UIDVALIDITY flips become a
  folder-local resync instead of a database-wide crisis; cross-folder
  deduplication by Message-ID/OBJECTID is possible later without key surgery.
