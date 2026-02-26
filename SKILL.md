# Wokhei — Agent Skill Guide

## What is Wokhei?

Wokhei is an **agent-first** Rust CLI for creating and publishing **Decentralized List** events on Nostr using the DCoSL protocol. Every command returns structured JSON with `next_actions` — no plain text ever.

## JSON Envelope

Every response has this exact shape:

```json
{
  "ok": true|false,
  "command": "command-name",
  "timestamp": 1740000000,
  "schema_version": "wokhei.v1",
  "result": { ... },       // present when ok=true
  "error": {               // present when ok=false
    "message": "...",
    "code": "ERROR_CODE",
    "retryable": false
  },
  "fix": "suggestion",     // present when ok=false
  "next_actions": [
    {
      "command": "wokhei ...",
      "description": "What this does"
    }
  ]
}
```

- `timestamp` is Unix epoch seconds (u64)
- `schema_version` is always `"wokhei.v1"`
- `retryable` indicates if the error is transient and the command can be retried

## How to Follow next_actions

After every command, read `next_actions`. Each entry is a runnable command template. Fill in any `<placeholder>` values with real data from previous results, then execute.

## Relay URLs

- **Dev (local)**: `ws://localhost:7777` (default — requires `docker compose up -d` in `strfry/`)
- **Prod**: `wss://dcosl.brainstorm.world`

Set `WOKHEI_RELAY` env var to override the default relay for all commands:
```bash
export WOKHEI_RELAY=wss://dcosl.brainstorm.world
```

Precedence: `--relay=<url>` flag > `WOKHEI_RELAY` env var > `ws://localhost:7777` default.

## Workflow

### 1. Initialize Keys

```bash
wokhei init --generate
```

Returns `pubkey` (hex) and `npub` (bech32). Keys saved to `~/.wokhei/keys`.

Import existing key from stdin (`--import=-` and `--import -` are both supported):
```bash
echo "nsec1..." | wokhei init --import=-
echo "nsec1..." | wokhei init --import -
```

Check current identity:
```bash
wokhei whoami
```

Returns `pubkey` (hex), `npub` (bech32), and the keys file path.

### 2. Create a List Header

Regular (kind 9998):
```bash
wokhei create-header --name=playlist --plural=playlists --titles="Playlist,Playlists" --tags=jazz,music
```

Addressable (kind 39998) — persists across updates, keyed by d-tag:
```bash
wokhei create-header --name=genre --plural=genres --titles="Genre,Genres" --addressable --d-tag=music-genres
```

With production relay:
```bash
wokhei create-header --relay=wss://dcosl.brainstorm.world --name=playlist --plural=playlists
```

### 3. Add Items to the List

By header event ID (fetches header to auto-detect kind):
```bash
wokhei add-item --header=<event-id> --resource="https://example.com/song" --fields="title=Kind of Blue,artist=Miles Davis"
```

By coordinate (cross-relay, no lookup needed):
```bash
wokhei add-item --header-coordinate="39998:<pubkey>:<d-tag>" --resource=jazz
```

With custom content and fields:
```bash
wokhei add-item --header=<event-id> --resource="https://example.com/song" --content='{"note":"great track"}' --fields="title=Kind of Blue,artist=Miles Davis"
```

Addressable item (kind 39999) — persists across updates, keyed by d-tag:
```bash
wokhei add-item --header-coordinate="39998:<pubkey>:<d-tag>" --resource="https://example.com" --addressable --d-tag=my-item-id
```

### 4. Query and Verify

```bash
# List all headers (default limit: 50)
wokhei list-headers

# List headers on production relay
wokhei list-headers --relay=wss://dcosl.brainstorm.world

# Filter by author
wokhei list-headers --author=<pubkey>

# Filter by topic tag
wokhei list-headers --tag=jazz

# Filter by name substring (client-side)
wokhei list-headers --name=playlist

# Combine filters with offset + limit (pagination)
wokhei list-headers --author=<pubkey> --tag=jazz --offset=20 --limit=10

# List items by header event ID (default limit: 100)
wokhei list-items <header-event-id>

# List items by header coordinate (no event ID needed)
wokhei list-items --header-coordinate="39998:<pubkey>:<d-tag>"

# Inspect a single event
wokhei inspect <event-id>
```

### 5. Count and Export

```bash
# Fast counts for headers/items on relay
wokhei count

# Full backup: all headers + linked items (JSON to stdout)
wokhei export --relay=wss://dcosl.brainstorm.world
```

### 6. Delete (NIP-09)

```bash
# Delete a single event
wokhei delete <event-id>

# Delete multiple events at once
wokhei delete <event-id-1> <event-id-2> <event-id-3>
```

**Caveat**: Deletion is a NIP-09 REQUEST — relays may or may not honor it.

## Tag Schema Reference

### Header Tags (kinds 9998/39998)

| Tag | Description | Example |
|-----|-------------|---------|
| `names` | Required singular + plural list names | `["names", "playlist", "playlists"]` |
| `titles` | Optional singular + plural display titles | `["titles", "Playlist", "Playlists"]` |
| `description` | Long description | `["description", "My curated jazz list"]` |
| `required` | Required item fields | `["required", "url", "title"]` |
| `recommended` | Optional item field (repeat tag per field) | `["recommended", "artist"]` |
| `t` | Topic hashtag | `["t", "jazz"]` |
| `alt` | Alt text | `["alt", "DCoSL list: playlist — Jazz Favorites"]` |
| `d` | Identifier (addressable) | `["d", "music-genres"]` |
| `client` | Client identifier | `["client", "wokhei"]` |

### Item Tags (kinds 9999/39999)

| Tag | Description | Example |
|-----|-------------|---------|
| `z` | Parent list pointer (required) | `["z", "<header-id>"]` or `["z", "39998:<pubkey>:<d-tag>"]` |
| `r` | Resource URL/ID | `["r", "https://example.com/song"]` |
| `p` / `e` / `t` / `a` | Item payload tags as required/allowed by parent header | `["p", "<pubkey>"]` |
| custom | Field key=value | `["title", "Kind of Blue"]` |

### z-tag Parent Pointer Rules

- For parent header kind `9998`: use the header event id in `z`
- For parent header kind `39998`: use coordinate `39998:<pubkey>:<d-tag>` in `z`
- `wokhei add-item` derives `z` automatically from `--header` or `--header-coordinate`
- `--z-tag` is intentionally unsupported

## Error Handling

1. Check `ok` field — `true` means success
2. If `false`, read `error.code` for machine-readable classification
3. Check `error.retryable` — if `true`, the command can be retried (e.g., relay timeout)
4. Read `fix` for a human-readable suggestion
5. Follow `next_actions` to recover

### Error Codes

| Code | Meaning | Retryable |
|------|---------|-----------|
| `KEYS_NOT_FOUND` | No keypair at ~/.wokhei/keys | No |
| `RELAY_UNREACHABLE` | Can't connect to relay | Yes |
| `RELAY_REJECTED` | Relay rejected event | No |
| `HEADER_NOT_FOUND` | Header event ID not on relay | No |
| `HEADER_MISSING_D_TAG` | Addressable header has no d tag | No |
| `INVALID_EVENT_ID` | Bad event ID format | No |
| `NO_RESULTS` | Query returned 0 events | No |
| `INVALID_NSEC` | Bad nsec format on import | No |
| `INVALID_COORDINATE` | Bad coordinate format | No |
| `INVALID_ARGS` | Bad CLI arguments / help / version | No |
| `INTERNAL_ERROR` | Panic / unexpected error | No |

## When to Use --header vs --header-coordinate

- **`--header=<event-id>`**: Default mode. Fetches the header from the relay to auto-detect its kind and derive the correct `z` parent pointer. Use when the header is on the same relay.
- **`--header-coordinate=<kind:pubkey:d-tag>`**: Detached mode. No relay lookup. Use for cross-relay references or when you already know the coordinate from a previous `create-header` result.

## Event Kinds

| Kind | Type | Usage |
|------|------|-------|
| 9998 | Regular | One-off list header |
| 9999 | Regular | One-off list item |
| 39998 | Addressable | Persistent list header (keyed by d-tag) |
| 39999 | Addressable | Persistent list item (keyed by d-tag) |

## Raw Event Publishing

For custom events not covered by built-in commands:

```bash
echo '{"kind": 9998, "content": "", "tags": [["names", "test", "tests"], ["titles", "Test", "Tests"]]}' | wokhei publish --relay=wss://dcosl.brainstorm.world -
```
