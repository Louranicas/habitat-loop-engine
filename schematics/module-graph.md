# module-graph

```text
plan.toml
  |-- ai_specs/S01..S13
  |-- ai_docs/layers/L01..L07
  |-- ai_docs/modules/M001..M004
  |-- schemas/*.json
  |-- scripts/verify-sync.sh

crates/substrate-types
  |-- neutral record shapes
  |-- no executor authority
  |-- no verifier side effects

crates/substrate-emit
  |-- future emission contracts
  |-- may depend on substrate-types
  |-- must not depend on substrate-verify

crates/substrate-verify
  |-- verifier contracts and checks
  |-- may depend on substrate-types
  |-- must not execute workflow actions

crates/hle-cli
  |-- scaffold CLI surface only
  |-- delegates verification to scripts/contracts
  |-- no live Habitat writes
```

## Review focus

The important graph property is authority separation. Adding a module is acceptable only when it preserves the direction from neutral types to either emission or verification without circular authority.
