# Wokhei

Agent-first CLI for creating and publishing Decentralized List events on Nostr using the [DCoSL protocol](https://github.com/wds4/brainstorm-knowledge-graph).

Every command returns structured JSON with `next_actions` — no plain text ever.

## Install

```bash
cargo install wokhei
```

## Quick Start

```bash
# Generate a keypair
wokhei init --generate

# Create a list header
wokhei create-header --name=playlist --title="Jazz Favorites" --tags=jazz,music

# Add an item
wokhei add-item --header=<event-id> --resource="https://example.com/song"

# Query
wokhei list-headers
wokhei list-headers --name=playlist
wokhei list-items <header-id>
wokhei list-items --header-coordinate="39998:<pubkey>:<d-tag>"
wokhei inspect <event-id>

# Delete (NIP-09 request)
wokhei delete <event-id>
```

## Relay Configuration

Default relay: `ws://localhost:7777`

Override with `--relay=<url>` flag or `WOKHEI_RELAY` env var:

```bash
export WOKHEI_RELAY=wss://dcosl.brainstorm.world
wokhei list-headers
```

## JSON Response Envelope

```json
{
  "ok": true,
  "command": "command-name",
  "timestamp": 1740000000,
  "schema_version": "wokhei.v1",
  "result": { },
  "next_actions": [
    { "command": "wokhei ...", "description": "What this does" }
  ]
}
```

## Event Kinds

| Kind | Type | Usage |
|------|------|-------|
| 9998 | Regular | One-off list header |
| 9999 | Regular | One-off list item |
| 39998 | Addressable | Persistent list header (keyed by d-tag) |
| 39999 | Addressable | Persistent list item (keyed by d-tag) |

## Agent Integration

See [SKILL.md](SKILL.md) for the full agent skill guide — error codes, tag schema, workflow patterns, and relay URLs.

## License

MIT
