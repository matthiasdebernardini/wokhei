# d-tag Strategy Proposal for Wokhei (Issue #3)

## Scope

This document proposes a **standard strategy** for generating `d` tags for addressable list headers (`kind:39998`) and addressable list items (`kind:39999`) in Wokhei.

This is a research/design proposal only. It does **not** introduce runtime behavior changes.

## Protocol Constraints (Authoritative)

1. In Nostr, addressable events are keyed by `(kind, pubkey, d-tag)`; relays keep only the latest event for the same tuple.
2. For list headers (`39998`) and list items (`39999`), a `d` tag is therefore identity-critical:
   - Change `d` unintentionally -> you create a new logical entity.
   - Reuse `d` intentionally -> you update the existing entity.
3. For list items in the Decentralized Lists custom NIP, parent linkage is encoded in `z`, while `d` remains the item identity for `39999` events.

## Field Research Snapshot

### Observed relay data (`wss://dcosl.brainstorm.world`)

Sampled via:
- `wokhei list-headers --relay=wss://dcosl.brainstorm.world --limit=20`
- `wokhei list-items --relay=wss://dcosl.brainstorm.world --header-coordinate="39998:..." --limit=20`

Observed patterns:
- Many `39998` headers use UUID-like `d` values (example: `f4e834b6-d4b2-404c-a823-182db65b2fe7`).
- Many `39999` items also use UUID-like `d` values (example: `839bb411-c10a-429c-9a8d-6c42c4214264`).
- Prior issue examples in this repo also show human-readable slugs (example: `ai-agents-on-nostr`).

Conclusion: ecosystem usage is mixed: **UUID-like**, **human-readable slugs**, and implicit hybrids.

## Candidate Strategies

### A) Human-readable slug only
Example:
- Header: `ai-agents-on-nostr`
- Item: `nous`

Pros:
- Easy to read/debug
- Friendly for CLI and docs

Cons:
- High collision risk (same names across contexts)
- Renaming pressure (users may want to edit names, but identity should stay stable)
- Manual disambiguation burden

### B) Opaque UUID / random token only
Example:
- Header: `f4e834b6-d4b2-404c-a823-182db65b2fe7`
- Item: `839bb411-c10a-429c-9a8d-6c42c4214264`

Pros:
- Very low collision risk
- Stable identity if persisted correctly

Cons:
- Poor readability
- Harder debugging/manual operations
- Harder to reason about duplicates semantically

### C) Deterministic hash only
Example:
- `sha256(parent_z + "|" + primary_value)[0..16]`

Pros:
- Reproducible/idempotent
- Low collision risk (if enough entropy)

Cons:
- Opaque
- Sensitive to canonicalization mistakes (small normalization mismatch -> new identity)

### D) Hybrid slug + deterministic suffix
Example:
- Header: `ai-agents-on-nostr--9f31c2ab`
- Item: `nous--f4a3dce1`

Pros:
- Readable + collision-resistant
- Easier debugging than opaque IDs
- Better operational ergonomics than slug-only

Cons:
- Requires strict normalization contract
- Slightly longer identifiers

## Recommendation

Adopt **Strategy D (Hybrid slug + deterministic suffix)** as the default standard.

### Why this is the best fit

- Preserves human meaning in day-to-day usage.
- Prevents accidental collisions in real deployments.
- Supports predictable idempotent generation when input canonicalization is fixed.
- Aligns with mixed ecosystem reality while avoiding weakest properties of slug-only mode.

## Proposed Standard Algorithm

### Shared normalization rules

- Lowercase ASCII output only.
- Trim whitespace.
- Replace whitespace runs with `-`.
- Remove characters outside `[a-z0-9-]`.
- Collapse repeated `-`.
- Trim leading/trailing `-`.
- If empty after normalization, use fallback `item` or `list`.

### Header (`39998`) default d-tag

Input:
- `name_singular`
- author pubkey

Algorithm:
1. `slug = normalize(name_singular)`
2. `suffix = hex(sha256("header|" + pubkey + "|" + slug))[0..8]`
3. `d = slug + "--" + suffix`

Rationale:
- Readable list identity with deterministic anti-collision suffix scoped to author+slug.

### Item (`39999`) default d-tag

Input:
- `parent_z`
- canonical item anchor (prefer first required primary tag value; fallback: `resource`/`r`)

Algorithm:
1. `anchor_slug = normalize(anchor_value)`
2. `suffix = hex(sha256("item|" + parent_z + "|" + anchor_value))[0..8]`
3. `d = anchor_slug + "--" + suffix`

Rationale:
- Stable per parent list + item anchor.
- Prevents duplicate logical items from receiving unrelated IDs.

## Operational Rules

1. If user passes `--d-tag`, preserve it verbatim (after minimal validity checks).
2. Auto-generation must be deterministic and pure (no timestamps/randomness in default mode).
3. Once assigned, `d` must be treated as immutable identity for updates.
4. Renaming display fields (`names`, `titles`, `name`) must not force `d` rotation.

## Tradeoff Summary

| Strategy | Readability | Collision risk | Idempotence | Operational safety |
|---|---|---|---|---|
| Slug only | High | High | Medium | Medium-Low |
| UUID/random | Low | Very Low | Low (unless persisted) | High |
| Hash only | Low | Very Low | High | High |
| Hybrid (recommended) | High | Low | High | High |

## Open Questions for Vinney / Vitor / Community

1. Should Wokhei expose explicit strategy modes (`slug`, `uuid`, `hybrid`) or only support `hybrid` + manual `--d-tag` override?
2. For item default anchor, what should be canonical precedence when multiple candidate tags are present (`r`, `p`, `e`, `a`, `t`)?
3. Should we enforce a max `d` length for interoperability hardening (for example 128 chars)?
4. Should we reserve/forbid specific prefixes for future migration/versioning?

## Implementation Checklist (Future Work, Out of Scope Here)

1. Add pure helper module for d-tag generation + normalization.
2. Add unit tests for:
   - normalization edge cases
   - deterministic outputs
   - collision sanity checks
   - immutability expectations across updates
3. Add CLI behavior:
   - optional `--d-tag-strategy` (if approved)
   - clear output showing generated `d` and coordinate
4. Update SKILL/README examples to show recommended generated format.
5. Add migration note for users currently relying on random UUID-only or slug-only conventions.

## Sources

- NIP-01 (official): https://raw.githubusercontent.com/nostr-protocol/nips/master/01.md
- NIP-33 status note (moved to NIP-01): https://raw.githubusercontent.com/nostr-protocol/nips/master/33.md
- Decentralized Lists custom NIP reference used in this repo plan:
  https://raw.githubusercontent.com/wds4/brainstorm-knowledge-graph/main/docs/nips/decentralizedLists.md
