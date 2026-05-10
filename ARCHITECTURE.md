# Architecture

The Habitat Loop Engine is scaffolded as a seven-layer local workflow substrate.

## Layer DAG

```text
L01 Foundation -> L02 Persistence -> L03 Workflow Executor -> L04 Verification
L01 Foundation -> L05 Dispatch Bridges
L01 Foundation -> L06 CLI
L03 Workflow Executor -> L07 Runbook Semantics
L04 Verification -> L07 Runbook Semantics
```

Forbidden edges:
- verifier must not call executor mutation paths;
- dispatch bridges must not write live Habitat services during scaffold;
- CLI must not bypass verifier authority;
- runbook semantics must not become a second workflow engine.

## Executor / verifier split

Executor crates may emit draft receipts and state transitions after M0 authorization. Verifier crates own PASS/FAIL authority. During scaffold, both are skeleton-only.

## L7 runbook semantics

Runbooks are typed workflow definitions with `AwaitingHuman` / `human_confirm` semantics. They are not a separate engine.

## Clustered module strategy

Cluster IDs use `HLE-C01` through `HLE-C07` to avoid collision with Habitat anti-pattern cluster labels.
