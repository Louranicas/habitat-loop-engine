# Habitat Loop Engine Scaffold Deployment Workflow

Created UTC: 2026-05-10T00:05:15Z
Status: workflow preservation packet
Boundary: scaffold is created and verified; M0 remains blocked until explicit `begin M0`.

## Purpose

This document preserves the exact high-integrity scaffold workflow used to create `/home/louranicas/claude-code-workspace/habitat-loop-engine` from the Habitat Loop Engine deployment framework without crossing into runtime implementation.

## Core principle

Scaffold authorization creates substrate, not behavior. The workflow must preserve executor/verifier separation, durable receipts, manifest verification, and independent review while refusing M0/runtime/live-integration work until separately authorized.

## Phase graph

1. Read-only assimilation of framework and dedicated review vault.
2. Command-lane review and 100-score guardrail addition.
3. Command-2 Cycle 3 gate: real receipt or exact waiver.
4. Exact scaffold phrase: `begin scaffold`.
5. Scaffold root creation after confirming root absence and scaffold-only authorization.
6. Scaffold-only artifact generation: Rust skeletons, docs, specs, AI docs, schematics, runbooks, scripts, tests, manifests, and receipts.
7. Status packet creation under `.deployment-work/status/` with `m0_authorized=false` and `live_integrations_authorized=false`.
8. Quality gate execution through the full `scripts/quality-gate.sh --scaffold` chain.
9. Independent scaffold review.
10. Review-driven scaffold-only verifier improvements, limited to checks and docs.
11. Manifest refresh and receipt closure.
12. Workflow preservation, Kanban triage, visualizations, and bounded background dispatch.
13. M0 parking state: leave implementation blocked until the exact `begin M0` phrase.

## Authorization state machine

- `pre_scaffold`: framework and vault exist; future root absent.
- `blocked_on_command_2`: no Command-2 receipt and no waiver.
- `waived_for_scaffold_only`: Luke gave exact waiver phrase.
- `scaffold_authorized`: Luke gave exact `begin scaffold` phrase after waiver.
- `scaffold_created`: root exists with scaffold-only substrate.
- `scaffold_verified`: quality gate and independent review passed.
- `m0_waiting`: M0 implementation blocked until `begin M0`.

## Scaffold artifact categories

- Rust skeleton crates: compile-safe, marker-only.
- Docs: root architecture, quality bar, harness contract, quickstart.
- AI specs: S01-S13.
- AI docs: layers, modules, anti-patterns, use-patterns.
- Schematics: text-first visual maps.
- Runbooks: scaffold verification and M0 boundary.
- Scripts: structural, safety, receipt, and quality gates.
- Tests: Python scaffold checks and Rust workspace compile checks.
- Receipts: scaffold authorization and framework closure.
- Manifests: scaffold, framework, vault, project.

## Verification gate

Canonical command:

```bash
cd /home/louranicas/claude-code-workspace/habitat-loop-engine
scripts/quality-gate.sh --scaffold
```

The gate is intentionally composite. As of this scaffold packet it runs sync, doc-link, `.claude`, anti-pattern, module-map, layer-DAG, receipt-schema, negative-control, runbook-schema, receipt-graph, test-taxonomy, bounded-log, and script-safety verifiers, then `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and the Python scaffold tests. This makes the verifier, not the narrative, the sole PASS/FAIL authority.

Expected terminal line:

```text
quality-gate --scaffold PASS
```

## Scaffold status packet

`.deployment-work/status/scaffold-status.json` is the machine-readable closure packet for the scaffold state. It must keep `m0_authorized=false`, `live_integrations_authorized=false`, the scaffold root, manifest count/hash, and the latest quality gate result aligned with receipts and manifests. The current status string is `scaffold-created`; this is compatible with the narrative's verified parking state only when `quality_gate` remains `PASS` and M0/live integration flags remain false.

## Review closure

Independent review must confirm:

- no M0 runtime behavior;
- no live integrations;
- no cron/daemon creation;
- scripts are bounded and safe;
- receipts/status preserve M0 blocked state.

## Kanban operating model

The parent Kanban triage card is provenance. It should remain parked unless Luke asks for active-board cleanup. Executable child tasks do the work:

1. workflow narrative audit;
2. visualization expansion;
3. Kanban graph and orchestration map;
4. scaffold boundary review;
5. synthesis/closure receipt.

Dependencies fan out to review/visualization/audit tasks, then fan in to a final synthesis card.

Any monitor used for this workflow must remain bounded, read-only, and non-daemonized. `scripts/kanban-hle-workflow-monitor.py` is a foreground helper with a finite timeout and no dispatch/promotion/claim/completion behavior; it is not authorization for cron, services, live Habitat writes, live Kanban mutation, or permanent orchestration.

## Audit additions captured

This narrative audit expands the original packet with scaffold-only phases that are present in adjacent artifacts but were implicit here:

- root-absence and authorization confirmation before root creation;
- explicit status-packet creation and reconciliation with `.deployment-work/status/scaffold-status.json`;
- the full quality-gate chain from `scripts/quality-gate.sh` rather than a single opaque PASS line;
- bounded-monitor semantics for Kanban follow-through;
- final M0 parking as a named state after preservation work.

No runtime executor behavior, live integration, cron job, daemon, service, or deployment claim is added by these narrative changes.

## Local M0 transition addendum — 2026-05-10T11:28:40Z

The original workflow packet above is preserved as historical scaffold provenance. The active repository state has since broadened from scaffold-only into bounded local M0: `plan.toml` now carries `m0_runtime = true`, `scripts/quality-gate.sh --m0` runs verifier-authorized local CLI round trips, and local receipts are emitted only under `.deployment-work/runtime/`.

The transition does not authorize live Habitat writes, cron jobs, systemd services, unbounded daemons, or final deployment claims. The current authority gates are:

```bash
scripts/quality-gate.sh --scaffold
scripts/quality-gate.sh --m0
```

For the directory/file review that reconciles the deployment framework, dedicated review vault, and active scaffold/M0 repository, see:

```text
docs/workflows/scaffold-m0-broadening-review-20260510T112840Z.md
```

## Next boundary

The next implementation-enabling phrase is no longer scaffold/M0; local M0 is already active. The next forbidden boundary requires separate explicit authorization:

```text
authorize live Habitat integration / authorize service deployment
```

Until then, continue local-only runtime work, docs, diagrams, reviews, planning, tests, receipts, and bounded foreground helpers only.
