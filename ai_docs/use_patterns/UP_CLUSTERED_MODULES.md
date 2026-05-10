# UP_CLUSTERED_MODULES

Status: scaffold use-pattern contract. Module clustering is a specification discipline, not a claim that every future module is implemented.

Predicate ID: `HLE-UP-006`

## Intent

The Loop Engine should prefer a small number of real, cohesive modules over many shallow descriptor files. Clusters group responsibilities by authority boundary, not by aesthetic symmetry.

## Scaffold clusters

1. Authority and planning: plan schema, authorization boundary, M0/M1/M2 cuts.
2. Substrate types: neutral records shared across emission and verification.
3. Emission contracts: future JSONL/receipt view generation without verifier authority.
4. Verification contracts: negative controls, hash checks, receipt graph validation.
5. Human operations: runbooks, Kanban orchestration, awaiting-human states.
6. Habitat integration doctrine: read-only integration maps for DevOps, Zellij, Atuin, and future live surfaces.

## Good pattern

A module is acceptable when it owns a clear invariant, has a documented dependency direction, and can be tested or reviewed independently.

## Bad pattern

Creating forty modules that only restate names without invariants is composition drift. Count-based completeness is not a scaffold PASS.

## Review checklist

- Does the cluster have a real invariant?
- Does it avoid cross-linking executor and verifier authority?
- Is the dependency direction documented in `schematics/layer-dag.md` or `schematics/module-graph.md`?
- Can a future test or verifier predicate observe drift?
