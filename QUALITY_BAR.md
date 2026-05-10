# Quality Bar

MEv2 L1 is the gold standard: trait-first boundaries, explicit state, typed errors, bounded output, clustered modules, and behavior-bearing tests.

## Scaffold bar

- `plan.toml` and `ULTRAMAP.md` must agree.
- Seven layer docs must exist.
- S01-S13 specs must exist.
- Anti/use-pattern registries must exist.
- `.claude` must contain context, local rules, commands, rules, and agents.
- Cargo workspace must compile as skeleton-only.
- No `unwrap`, `expect`, `panic!`, `todo!`, `dbg!`, or `unsafe` in Rust sources.

## Semantic predicate bars

The detailed receipt and PASS/FAIL examples live in `docs/quality/semantic-predicates.md`.

- `HLE-SP-001`: anti-pattern docs require detector semantics, negative controls, and remediation expectations instead of file-count-only registry presence.
- `HLE-SP-002`: S01-S13 specs require acceptance gates that reject premature execution PASS claims instead of file-count-only inventory presence.
- `HLE-SP-003`: verifier scripts must map checklist bars to explicit predicate IDs and evaluator paths instead of relying on a script-name checklist.
