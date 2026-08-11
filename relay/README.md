# Nooki relay

The relay exposes two listeners:

- `127.0.0.1:7000` for the HTTPS/WebSocket API behind Caddy.
- `127.0.0.1:7001` for the local-only activation administration API.
- `0.0.0.0:25565` for public Minecraft Java traffic.

Nooki installations authenticate with a local Ed25519 key. A single-use activation code binds relay access to that installation public key. The relay challenges the client, verifies its signature and active entitlement, and permits one relayed Minecraft server per installation at a time. Other local servers continue running without a public relay address.

Temporary public route labels are derived from a fresh per-start token, so they remain stable during reconnects but change whenever the Minecraft server restarts. Optional vanity labels are reserved to the installation/server identity and persisted in `data/vanity.json`.

The base and wildcard DNS records must be DNS-only. Caddy terminates HTTPS for the control and data WebSockets on port 443, while Minecraft Java connects directly to TCP 25565.

`relay.env` contains the server-side route-label and administration secrets and must never be committed or included in desktop builds. Desktop builds contain only the relay URL; every Nooki installation creates its own signing key under the application-local data directory.

Generate one or more activation codes directly on the VPS:

```sh
set -a
. /opt/nooki-relay/relay.env
set +a
curl --fail-with-body \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  --data '{"count":1}' \
  http://127.0.0.1:7001/v1/activation-codes
```

The raw activation codes are returned only once. `data/access.json` stores only keyed hashes of unused codes plus the public installation identities and revocable entitlement records.
