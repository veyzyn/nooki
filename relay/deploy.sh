#!/bin/sh
set -eu

SOURCE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
INSTALL_DIR=/opt/nooki-relay

sudo install -d -m 0755 "$INSTALL_DIR"
sudo install -d -o 65532 -g 65532 -m 0700 "$INSTALL_DIR/data"
sudo install -m 0644 \
  "$SOURCE_DIR/go.mod" \
  "$SOURCE_DIR/go.sum" \
  "$SOURCE_DIR/access.go" \
  "$SOURCE_DIR/main.go" \
  "$SOURCE_DIR/Dockerfile" \
  "$SOURCE_DIR/compose.yaml" \
  "$INSTALL_DIR/"

if [ ! -f "$INSTALL_DIR/relay.env" ]; then
  secret="$(openssl rand -base64 48 | tr -d '\n=' | tr '+/' '-_')"
  admin_token="$(openssl rand -base64 48 | tr -d '\n=' | tr '+/' '-_')"
  umask 077
  {
    printf '%s\n' 'PUBLIC_BASE=nooki-64f85d08d9.mints.wtf'
    printf 'LABEL_SECRET=%s\n' "$secret"
    printf 'ADMIN_TOKEN=%s\n' "$admin_token"
    printf '%s\n' 'HTTP_ADDRESS=127.0.0.1:7000'
    printf '%s\n' 'ADMIN_ADDRESS=127.0.0.1:7001'
    printf '%s\n' 'MINECRAFT_ADDRESS=0.0.0.0:25565'
    printf '%s\n' 'VANITY_FILE=/data/vanity.json'
    printf '%s\n' 'ACCESS_FILE=/data/access.json'
  } > /tmp/nooki-relay.env
  sudo install -m 0600 /tmp/nooki-relay.env "$INSTALL_DIR/relay.env"
  rm -f /tmp/nooki-relay.env
fi

if ! sudo grep -q '^ADMIN_TOKEN=' "$INSTALL_DIR/relay.env"; then
  admin_token="$(openssl rand -base64 48 | tr -d '\n=' | tr '+/' '-_')"
  printf 'ADMIN_TOKEN=%s\n' "$admin_token" | sudo tee -a "$INSTALL_DIR/relay.env" >/dev/null
fi
if ! sudo grep -q '^ADMIN_ADDRESS=' "$INSTALL_DIR/relay.env"; then
  printf '%s\n' 'ADMIN_ADDRESS=127.0.0.1:7001' | sudo tee -a "$INSTALL_DIR/relay.env" >/dev/null
fi
if ! sudo grep -q '^ACCESS_FILE=' "$INSTALL_DIR/relay.env"; then
  printf '%s\n' 'ACCESS_FILE=/data/access.json' | sudo tee -a "$INSTALL_DIR/relay.env" >/dev/null
fi

sudo install -m 0644 "$SOURCE_DIR/Caddyfile" /etc/caddy/Caddyfile.d/nooki-relay.caddy
cd "$INSTALL_DIR"
sudo docker compose build
sudo docker compose up -d
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
