# Clustered Modules — Full Codebase Synergy Map

Status: deployment-framework correction after four-module collapse review. This is now the source planning map for full-codebase deployment; current M0 source remains a subset until the Claude Code fleet implements the planned modules.

Authority roots:
- Framework: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/HABITAT_LOOP_ENGINE_DEPLOYMENT_FRAMEWORK.md` §11.
- Architecture receipt: `deployment-framework/receipts/command-primary-scaffold-architecture.md` lines 138-163 (`7 layers`, `~50 modules`).
- Active repo plan: `plan.toml` `[full_codebase]` and `[[planned_modules]]`.

## Corrected target

- Layer count: 7 (`L01` through `L07`).
- Planned module surfaces: 50 total.
- Planned Rust/source modules: 46.
- Planned operational/QI surfaces: 4 scripts/docs modules.
- Current active implementation subset: 4 legacy M0 crate surfaces. These are not framework-complete.

## Cluster synergy summary

| Cluster | Name | Layers | Count | Synergy chain |
|---|---|---:|---:|---|
| C01_EVIDENCE_INTEGRITY | Evidence Integrity | L01/L02/L04 | 5 | receipt hash -> claim store -> receipt store -> verifier recompute -> final claim evaluator |
| C02_AUTHORITY_STATE | Authority and State | L01/L03/L04 | 5 | type-state authority and transition table prevent executor self-certification |
| C03_BOUNDED_EXECUTION | Bounded Execution | L03 | 5 | local runner + phase executor + timeout/retry policies make every runtime path finite and verifier-visible |
| C04_ANTI_PATTERN_INTELLIGENCE | Anti-Pattern Intelligence | L01/L02/L04 | 5 | catalogued anti-patterns become scanner events, test taxonomy checks, and false-pass audits |
| C05_PERSISTENCE_LEDGER | Persistence Ledger | L02 | 7 | schema-first ledger ties runs, ticks, evidence, verifier results, and blockers into append-only proof |
| C06_RUNBOOK_SEMANTICS | Runbook Semantics | L07 | 8 | incident-response runbooks reuse workflow authority instead of becoming a parallel engine |
| C07_DISPATCH_BRIDGES | Dispatch Bridges | L05 | 6 | Zellij/Atuin/DevOps/STcortex/Watcher bridges share contract parity and read-only/live-write gates |
| C08_CLI_SURFACE | CLI Surface | L06 | 5 | operator commands remain thin typed adapters over executor/verifier/runbook authority |
| C09_DEVOPS_QI_OPERATIONAL_LANE | DevOps/QI Operational Lane | L06/scripts | 4 | scripts enforce docs-source-gate parity and prevent quiet topology collapse |

## Planned module map

| ID | Module | Layer | Cluster | Source path | Synergy role |
|---|---|---|---|---|---|
| M005 | `receipt_hash` | L01 | C01_EVIDENCE_INTEGRITY | `crates/hle-core/src/evidence/receipt_hash.rs` | canonical receipt hashing; source of all proof identity |
| M006 | `claims_store` | L01 | C01_EVIDENCE_INTEGRITY | `crates/hle-core/src/evidence/claims_store.rs` | claim graph store with provisional/verified/final state anchors |
| M007 | `receipts_store` | L02 | C01_EVIDENCE_INTEGRITY | `crates/hle-storage/src/receipts_store.rs` | append-only receipt persistence |
| M008 | `receipt_sha_verifier` | L04 | C01_EVIDENCE_INTEGRITY | `crates/hle-verifier/src/receipt_sha_verifier.rs` | independent verifier recomputes receipt hashes |
| M009 | `final_claim_evaluator` | L04 | C01_EVIDENCE_INTEGRITY | `crates/hle-verifier/src/final_claim_evaluator.rs` | only verifier can promote final claims |
| M010 | `claim_authority` | L01 | C02_AUTHORITY_STATE | `crates/hle-core/src/authority/claim_authority.rs` | type-state authority model |
| M011 | `workflow_state` | L01 | C02_AUTHORITY_STATE | `crates/hle-core/src/state/workflow_state.rs` | workflow state enum and invariants |
| M012 | `state_machine` | L03 | C02_AUTHORITY_STATE | `crates/hle-executor/src/state_machine.rs` | transition executor with verifier-visible events |
| M013 | `status_transitions` | L03 | C02_AUTHORITY_STATE | `crates/hle-executor/src/status_transitions.rs` | transition table and rollback affordances |
| M014 | `claim_authority_verifier` | L04 | C02_AUTHORITY_STATE | `crates/hle-verifier/src/claim_authority_verifier.rs` | adversarial check against executor self-certification |
| M015 | `bounded` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/bounded.rs` | bounded output/time/memory primitives |
| M016 | `local_runner` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/local_runner.rs` | one-shot local command runner |
| M017 | `phase_executor` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/phase_executor.rs` | phase-aware step execution |
| M018 | `timeout_policy` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/timeout_policy.rs` | TERM to KILL bounded timeout policy |
| M019 | `retry_policy` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/retry_policy.rs` | explicit bounded retry semantics |
| M020 | `anti_pattern_scanner` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/anti_pattern_scanner.rs` | turns anti-pattern catalog into executable checks |
| M021 | `anti_pattern_events` | L02 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-storage/src/anti_pattern_events.rs` | stores scanner events and evidence |
| M022 | `test_taxonomy` | L01 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-core/src/testing/test_taxonomy.rs` | behavioral test taxonomy model |
| M023 | `test_taxonomy_verifier` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/test_taxonomy_verifier.rs` | rejects inflated/vacuous tests |
| M024 | `false_pass_auditor` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/false_pass_auditor.rs` | catches PASS claims without authority evidence |
| M025 | `pool` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/pool.rs` | local database pool and connection discipline |
| M026 | `migrations` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/migrations.rs` | schema-first migration runner |
| M027 | `workflow_runs` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/workflow_runs.rs` | workflow run table abstraction |
| M028 | `workflow_ticks` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/workflow_ticks.rs` | tick ledger for freshness and causality |
| M029 | `evidence_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/evidence_store.rs` | bounded evidence blob/index store |
| M030 | `verifier_results_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/verifier_results_store.rs` | verifier result ledger |
| M031 | `blockers_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/blockers_store.rs` | blocked/awaiting-human state persistence |
| M032 | `runbook_schema` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/schema.rs` | typed runbook workflow definition schema |
| M033 | `runbook_parser` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/parser.rs` | parse and validate runbooks |
| M034 | `runbook_phase_map` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/phase_map.rs` | map runbook phases onto executor phases |
| M035 | `runbook_human_confirm` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/human_confirm.rs` | AwaitingHuman semantics |
| M036 | `runbook_manual_evidence` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/manual_evidence.rs` | manual evidence attachment model |
| M037 | `runbook_scaffold` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/scaffold.rs` | generate incident runbook scaffolds |
| M038 | `runbook_incident_replay` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/incident_replay.rs` | deterministic replay fixtures |
| M039 | `runbook_safety_policy` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/safety_policy.rs` | runbook safety gate and policy |
| M040 | `bridge_contract` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/bridge_contract.rs` | bridge schema/port/path contract model |
| M041 | `zellij_dispatch` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/zellij_dispatch.rs` | zellij pane dispatch packet adapter |
| M042 | `atuin_qi_bridge` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/atuin_qi_bridge.rs` | Atuin QI script chain integration |
| M043 | `devops_v3_probe` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/devops_v3_probe.rs` | read-only DevOps V3 probe surface |
| M044 | `stcortex_anchor_bridge` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/stcortex_anchor_bridge.rs` | future gated STcortex anchor/write adapter; read-only until authorized |
| M045 | `watcher_notice_writer` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/watcher_notice_writer.rs` | Watcher/Hermes notification receipt writer |
| M046 | `cli_args` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/args.rs` | typed CLI argument parser |
| M047 | `cli_run` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/run.rs` | hle run command |
| M048 | `cli_verify` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/verify.rs` | hle verify command |
| M049 | `cli_daemon_once` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/daemon_once.rs` | bounded daemon --once command |
| M050 | `cli_status` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/status.rs` | status and topology report command |
| M051 | `verify_sync_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-sync.sh` | operational verifier script surface |
| M052 | `quality_gate_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/quality-gate.sh` | quality gate orchestrator script |
| M053 | `module_map_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-module-map.sh` | module-map consistency verifier |
| M054 | `layer_dag_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-layer-dag.sh` | layer DAG consistency verifier |

## Claude Code fleet decomposition

Use arena-style fleet coding rather than single-agent hand edits:

1. Fleet A: C01 + C02 foundation/authority primitives.
2. Fleet B: C05 persistence ledger and migrations.
3. Fleet C: C03 bounded execution.
4. Fleet D: C04 verifier/anti-pattern authority.
5. Fleet E: C06 runbook semantics.
6. Fleet F: C07 dispatch bridges with STcortex kept read-only until explicit live-write authorization.
7. Fleet G: C08 CLI surface.
8. Fleet H: C09 QI/source-topology gates and adversarial false-pass review.

## Hard failure condition

A `verify-stack` or `review-final` claim is invalid unless `scripts/verify-source-topology.sh --strict` passes and every planned module has source and test authority. Until then, report this as `FULL_CODEBASE_TOPOLOGY_INCOMPLETE`, not deployed.
