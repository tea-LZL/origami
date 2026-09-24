# Origami test harness

Local Docker harness for live protocol tests.

- **Dovecot 2.3** — IMAP on `127.0.0.1:10143` (plaintext, testing only).
  User `origami` / password `origami`, maildir seeded with 3 messages.
  Committed filenames use `__c__` instead of `:` so the tree checks out on
  Windows. The container restores real Maildir names (`:2,flags`) at start.
- **GreenMail** — SMTP on `127.0.0.1:30025`, IMAP on `127.0.0.1:30143`
  (users are auto-created on first delivery; password = address local part).

## Usage

```sh
docker compose -f tests/harness/docker-compose.yml up -d

# IMAP tests against Dovecot
ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test imap_harness
ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test sync_harness

# SMTP + IMAP receive against GreenMail
ORIGAMI_TEST_SMTP=1 cargo test -p origami-core --test send_e2e

# All live tests (separate --test flags — Cargo doesn't accept pipe patterns)
ORIGAMI_TEST_IMAP=1 ORIGAMI_TEST_SMTP=1 \
  cargo test -p origami-core \
    --test imap_harness \
    --test sync_harness \
    --test send_e2e

docker compose -f tests/harness/docker-compose.yml down
```

The integration tests are no-ops unless `ORIGAMI_TEST_IMAP` (or `ORIGAMI_TEST_SMTP`) is set, so a
plain `cargo test` (and CI without Docker) stays green.
