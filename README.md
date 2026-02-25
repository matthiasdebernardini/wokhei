# Wokhei

Agent-first CLI for creating and publishing Decentralized List events on Nostr using the [DCoSL protocol](https://github.com/nickmccoy/brainstorm-knowledge-graph).

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
wokhei create-header --relay ws://localhost:7777 --name playlist --title "Jazz Favorites" --tags jazz,music

# Add an item
wokhei add-item --relay ws://localhost:7777 --header <event-id> --resource "https://example.com/song"

# Query
wokhei list-headers --relay ws://localhost:7777
wokhei list-items --relay ws://localhost:7777 <header-id>
wokhei inspect --relay ws://localhost:7777 <event-id>

# Delete (NIP-09 request)
wokhei delete --relay ws://localhost:7777 <event-id>
```

## JSON Response Envelope

```json
{
  "ok": true,
  "schema_version": "wokhei.v1",
  "command": "command-name",
  "timestamp": "ISO-8601 UTC",
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
