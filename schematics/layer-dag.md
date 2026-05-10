# layer-dag

```text
L01 Authority Boundary
  |
  v
L02 Workflow Definition Model
  |
  v
L03 Executor State Machine  --->  L04 Verifier and Receipt Authority
  |                                  ^
  |                                  |
  v                                  |
L05 Persistence Ledger -------------+
  |
  v
L06 Local Operation Surface
  |
  v
L07 Habitat Integration Doctrine
```

## Dependency constraints

- Lower-numbered layers may define contracts consumed by later layers.
- L03 executor concepts must not certify their own PASS; L04 owns verifier authority.
- L05 stores claims and verifier evidence without rewriting historical nodes.
- L06 offers local CLI and runbook surfaces while preserving bounded-output rules.
- L07 remains read-only doctrine until explicit live-integration authorization.

## Scaffold invariant

A layer is complete only when its contract, verifier predicate, and receipt impact are named. File presence alone is not sufficient evidence.
