# Habitat Loop Engine Scaffold Workflow Kanban Map

Created UTC: 2026-05-10T00:05:15Z
Status: scaffold workflow preservation map
Boundary: Kanban work is docs/review/receipt coordination only. It must not create runtime behavior, live Habitat integrations, cron jobs, daemons, services, or deployment claims.
Related workflow: [scaffold-deployment-workflow.md](scaffold-deployment-workflow.md)
Related Mermaid diagrams: [scaffold-deployment-mermaid.md](scaffold-deployment-mermaid.md)

## Task graph

```text
K0 t_d0ccf33d parent/provenance triage                    status: triage / parked
├── K1 t_1da4b7f3 narrative audit and workflow expansion   assignee: problem-solver-god-tier
├── K2 t_ae746f1a visual systems map + Mermaid validation  assignee: python-coder-godtier
├── K3 t_9d75d087 orchestration/Kanban dependency map      assignee: apex-praxis-godtier
├── K4 t_a05550be scaffold boundary technical review       assignee: rust-coder-godtier
└── K5 t_396fb09a synthesis closure receipt                assignee: problem-solver-god-tier
    active parents: K1,K2,K3,K4
```

## Observed board state

Observed from the Kanban DB and card comments during the K3 run:

| Key | Task id | Role | Assignee | Observed status at K3 start | Provenance |
| --- | --- | --- | --- | --- | --- |
| K0 | `t_d0ccf33d` | parked parent/provenance triage | problem-solver-god-tier | `triage` | source card for the workflow package; comments name executable children |
| K1 | `t_1da4b7f3` | narrative audit/expansion | problem-solver-god-tier | `running` | created with K0 as parent, then unlinked/promoted so parked triage does not gate work |
| K2 | `t_ae746f1a` | visualization validation/enrichment | python-coder-godtier | `running` | created with K0 as parent, then unlinked/promoted so parked triage does not gate work |
| K3 | `t_9d75d087` | Kanban orchestration map and dispatch audit | apex-praxis-godtier | `running` | created with K0 as parent, then unlinked/promoted so parked triage does not gate work |
| K4 | `t_a05550be` | scaffold boundary technical review | rust-coder-godtier | `running` | created with K0 as parent, then unlinked/promoted so parked triage does not gate work |
| K5 | `t_396fb09a` | synthesis closure | problem-solver-god-tier | `todo` | fan-in child gated by active task links from K1-K4 |

Active dependency edges at K3 start:

```text
K1 t_1da4b7f3 ┐
K2 t_ae746f1a ├──> K5 t_396fb09a
K3 t_9d75d087 ┤
K4 t_a05550be ┘
```

The K0-to-K1/K2/K3/K4 relationship is provenance, not an active dependency edge. It is preserved by K0 comments because K0 intentionally remains parked in `triage` and must not be closed by executable scaffold workers.

## Status policy

- K0 remains `triage` as provenance unless Luke asks for cleanup.
- K1-K4 can run in parallel after being unlinked from parked K0 and promoted.
- K5 must wait for active K1-K4 task links.
- All cards must preserve scaffold-only boundary.

## Background monitor behavior

`scripts/kanban-hle-workflow-monitor.py` is now a bounded, read-only manual helper for this workflow. It accepts task ids, polls `hermes kanban show <task_id>` every 60 seconds, prints only changed snapshots, exits once all supplied tasks are done/blocked/archived/terminal, and otherwise times out after 90 minutes. It does not dispatch, promote, claim, complete, block, or otherwise mutate Kanban state. It is not a daemon, cron job, service, or runtime Habitat integration.

Race caveats:

- Dispatch must be performed explicitly by an operator or the existing Hermes gateway dispatcher, not by this read-only monitor.
- The parked K0 triage card must not remain as an active parent for executable cards; otherwise children wait forever because K0 is intentionally not completed. The observed workflow handled this by unlinking K0 from K1-K4, promoting those cards, and documenting provenance in comments.
- K5 was created before all fan-in links were attached, so a dispatcher could see it briefly as ready during graph construction. Attach K1-K4 -> K5 links before running broad dispatch, or verify K5 is back in `todo`/waiting before relying on synthesis ordering.
- `hermes kanban dispatch` may run concurrently with other dispatch loops; status observations are point-in-time and should be treated as audit evidence, not a lock or completion proof.


## Acceptance criteria

K1 narrative audit:
- workflow doc is comprehensive;
- M0 boundary is explicit;
- missing phases are added as docs only.

K2 visualization:
- Mermaid diagrams are valid text artifacts;
- links resolve;
- no generated binary attachment required;
- diagram notes explain decision gates, failure loops, and scaffold-only terminal state.

K3 orchestration map:
- Kanban tasks are mapped with parent/child dependencies;
- background monitor behavior is documented;
- dispatcher race caveat is noted.

K4 boundary review:
- no runtime/live/daemon drift;
- quality gate remains PASS;
- findings are written as comments or docs only.

K5 synthesis:
- reads parent outputs;
- writes one closure note;
- reports exact paths and verification.

## Background monitor and dispatcher caveats

The workflow packet may be observed by bounded monitoring or dispatch logic, but this map does not authorize creating a new monitor, daemon, cron job, or service. If an existing dispatcher promotes child cards while a worker is starting, each worker must re-read the current card state before acting and stop if the card is no longer runnable.

K5 should treat parent handoffs as the source of truth rather than assuming the original K0 comment thread is complete. If any parent blocks, K5 remains waiting until the block is resolved.

## Boundary checklist for every card

Before completion, each card should be able to answer yes to all of these:

- Did the work stay inside the scaffold repository or read-only referenced sources?
- Were all changes limited to docs, diagrams, receipts, tests, or verifier surfaces?
- Did verification include `scripts/verify-doc-links.sh` for docs/diagram edits?
- If files were edited, did `scripts/quality-gate.sh --scaffold` remain PASS?
- Does the handoff explicitly preserve the `begin M0` boundary?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a workflow preservation artifact within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `docs/workflows/scaffold-deployment-kanban-map.md`.
- Parent directory: `docs/workflows`.
- Adjacent markdown siblings sampled: scaffold-deployment-mermaid.md, scaffold-deployment-workflow.md, scaffold-m0-broadening-review-20260510T112840Z.md, scaffold-workflow-kanban-closure.md.
- This file should be read with `plan.toml`, `ULTRAMAP.md`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, and `.deployment-work/status/scaffold-status.json` when deciding whether a change is scaffold-only, local-M0, or outside authorization.

### Verification hooks
- Baseline scaffold gate: `scripts/quality-gate.sh --scaffold --json`.
- Local-M0 gate: `scripts/quality-gate.sh --m0 --json`.
- Manifest authority: `sha256sum -c SHA256SUMS.txt` after every documentation or status edit.
- For vault/framework-only edits, refresh the appropriate vault/framework manifest before declaring closure.

### Acceptance criteria
- The document names its role, boundary, and verification surface clearly.
- Claims about PASS/FAIL are backed by verifier output or receipts, not prose alone.
- Any runtime behavior described here remains local-only unless a later authorization receipt explicitly expands scope.
- Future agents can identify which files to inspect next without guessing hidden context.

### Failure modes
- Treat vague "complete", "ready", or "deployed" wording as insufficient unless it points to gates, manifests, and receipts.
- Do not infer live integration permission from local-M0 wording.
- Do not create background services or recurring jobs from this document alone.
- If this file drifts from `plan.toml` or `ULTRAMAP.md`, update the authority files first and rerun gates.

### Next maintenance action
On the next broadening pass, re-run the markdown census, inspect files with fewer than 180 words or missing boundary/verification terms, update this section with any new authority roots, then refresh manifests and rerun both quality gates.

