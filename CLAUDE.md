# Wokhei Project Guidelines

## Rust Style

- Prefer functional/iterator style over imperative loops (`iter`, `map`, `filter`, `fold`, `collect`)
- Avoid mutable variables where possible; favor expression-oriented code
- Minimize side effects; keep functions pure
- Use combinator chains over `for` loops with `mut` accumulators
