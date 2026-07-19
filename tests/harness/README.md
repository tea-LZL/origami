# Origami test harness

Local Docker harness for live protocol tests.

- **Dovecot 2.3** — IMAP on `127.0.0.1:10143` (plaintext, testing only).
  User `origami` / password `origami`, maildir seeded with 3 messages.
- **GreenMail** — SMTP on `127.0.0.1:30025`, IMAP on `127.0.0.1:30143`
  (users are auto-created on first delivery; password = address local part).

## Usage

```sh
docker compose -f tests/harness/docker-compose.yml up -d
ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test imap_harness
docker compose -f tests/harness/docker-compose.yml down
```

The integration tests are no-ops unless `ORIGAMI_TEST_IMAP` is set, so a
plain `cargo test` (and CI without Docker) stays green.
