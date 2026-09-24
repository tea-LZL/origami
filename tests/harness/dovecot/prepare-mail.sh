#!/bin/sh
# Maildir flag suffixes use ":", which Windows cannot store. The git tree
# keeps those names with "__c__" in place of ":". This runs inside the
# Dovecot container and restores the real names on a writable copy.
set -eu

rm -rf /mail
mkdir -p /mail
cp -a /mail-src/. /mail/

for src in /mail/origami/cur/*__c__*; do
  [ -e "$src" ] || continue
  dest=$(printf '%s' "$src" | sed 's/__c__/:/g')
  mv "$src" "$dest"
done

if command -v chown >/dev/null 2>&1; then
  chown -R 1000:1000 /mail || true
fi

if [ -x /usr/local/bin/docker-entrypoint.sh ]; then
  exec /usr/local/bin/docker-entrypoint.sh dovecot -F
fi
exec dovecot -F
