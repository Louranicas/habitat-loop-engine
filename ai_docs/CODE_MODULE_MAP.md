# Code Module Map — Full Framework Target

The active repository previously mapped only four M0 substrate crates. That was a local-M0 subset, not the deployment-framework target. This map now records the full ~50-module target and marks implementation status explicitly.

## Legacy M0 substrate crates (M001-M004) — currently implemented

These four crates are anchored under `[[modules]]` in `plan.toml` and remain operational. The framework canonical planned topology (M005-M054 below) will eventually absorb their surfaces.

| ID | Module | Layer | Crate | Current status |
|---|---|---|---|---|
| M001 | substrate-types | L01 | `crates/substrate-types` | implemented-legacy-substrate |
| M002 | substrate-verify | L04 | `crates/substrate-verify` | implemented-legacy-substrate |
| M003 | substrate-emit | L02/L03/L07 | `crates/substrate-emit` | implemented-legacy-substrate |
| M004 | hle-cli (main + 5 new sub-modules) | L06 | `crates/hle-cli` | implemented-legacy-substrate |

## Planned topology (M005-M054, 46 Rust modules + 4 ops scripts) — compile-safe stubs landed 2026-05-11

| ID | Module | Layer | Cluster | Source path | Current status |
|---|---|---|---|---|---|
| M005 | `receipt_hash` | L01 | C01_EVIDENCE_INTEGRITY | `crates/hle-core/src/evidence/receipt_hash.rs` | planned-missing-source |
| M006 | `claims_store` | L01 | C01_EVIDENCE_INTEGRITY | `crates/hle-core/src/evidence/claims_store.rs` | planned-missing-source |
| M007 | `receipts_store` | L02 | C01_EVIDENCE_INTEGRITY | `crates/hle-storage/src/receipts_store.rs` | planned-missing-source |
| M008 | `receipt_sha_verifier` | L04 | C01_EVIDENCE_INTEGRITY | `crates/hle-verifier/src/receipt_sha_verifier.rs` | planned-missing-source |
| M009 | `final_claim_evaluator` | L04 | C01_EVIDENCE_INTEGRITY | `crates/hle-verifier/src/final_claim_evaluator.rs` | planned-missing-source |
| M010 | `claim_authority` | L01 | C02_AUTHORITY_STATE | `crates/hle-core/src/authority/claim_authority.rs` | planned-missing-source |
| M011 | `workflow_state` | L01 | C02_AUTHORITY_STATE | `crates/hle-core/src/state/workflow_state.rs` | planned-missing-source |
| M012 | `state_machine` | L03 | C02_AUTHORITY_STATE | `crates/hle-executor/src/state_machine.rs` | planned-missing-source |
| M013 | `status_transitions` | L03 | C02_AUTHORITY_STATE | `crates/hle-executor/src/status_transitions.rs` | planned-missing-source |
| M014 | `claim_authority_verifier` | L04 | C02_AUTHORITY_STATE | `crates/hle-verifier/src/claim_authority_verifier.rs` | planned-missing-source |
| M015 | `bounded` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/bounded.rs` | planned-missing-source |
| M016 | `local_runner` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/local_runner.rs` | planned-missing-source |
| M017 | `phase_executor` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/phase_executor.rs` | planned-missing-source |
| M018 | `timeout_policy` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/timeout_policy.rs` | planned-missing-source |
| M019 | `retry_policy` | L03 | C03_BOUNDED_EXECUTION | `crates/hle-executor/src/retry_policy.rs` | planned-missing-source |
| M020 | `anti_pattern_scanner` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/anti_pattern_scanner.rs` | planned-missing-source |
| M021 | `anti_pattern_events` | L02 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-storage/src/anti_pattern_events.rs` | planned-missing-source |
| M022 | `test_taxonomy` | L01 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-core/src/testing/test_taxonomy.rs` | planned-missing-source |
| M023 | `test_taxonomy_verifier` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/test_taxonomy_verifier.rs` | planned-missing-source |
| M024 | `false_pass_auditor` | L04 | C04_ANTI_PATTERN_INTELLIGENCE | `crates/hle-verifier/src/false_pass_auditor.rs` | planned-missing-source |
| M025 | `pool` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/pool.rs` | planned-missing-source |
| M026 | `migrations` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/migrations.rs` | planned-missing-source |
| M027 | `workflow_runs` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/workflow_runs.rs` | planned-missing-source |
| M028 | `workflow_ticks` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/workflow_ticks.rs` | planned-missing-source |
| M029 | `evidence_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/evidence_store.rs` | planned-missing-source |
| M030 | `verifier_results_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/verifier_results_store.rs` | planned-missing-source |
| M031 | `blockers_store` | L02 | C05_PERSISTENCE_LEDGER | `crates/hle-storage/src/blockers_store.rs` | planned-missing-source |
| M032 | `runbook_schema` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/schema.rs` | planned-missing-source |
| M033 | `runbook_parser` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/parser.rs` | planned-missing-source |
| M034 | `runbook_phase_map` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/phase_map.rs` | planned-missing-source |
| M035 | `runbook_human_confirm` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/human_confirm.rs` | planned-missing-source |
| M036 | `runbook_manual_evidence` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/manual_evidence.rs` | planned-missing-source |
| M037 | `runbook_scaffold` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/scaffold.rs` | planned-missing-source |
| M038 | `runbook_incident_replay` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/incident_replay.rs` | planned-missing-source |
| M039 | `runbook_safety_policy` | L07 | C06_RUNBOOK_SEMANTICS | `crates/hle-runbook/src/safety_policy.rs` | planned-missing-source |
| M040 | `bridge_contract` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/bridge_contract.rs` | planned-missing-source |
| M041 | `zellij_dispatch` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/zellij_dispatch.rs` | planned-missing-source |
| M042 | `atuin_qi_bridge` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/atuin_qi_bridge.rs` | planned-missing-source |
| M043 | `devops_v3_probe` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/devops_v3_probe.rs` | planned-missing-source |
| M044 | `stcortex_anchor_bridge` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/stcortex_anchor_bridge.rs` | planned-missing-source |
| M045 | `watcher_notice_writer` | L05 | C07_DISPATCH_BRIDGES | `crates/hle-bridge/src/watcher_notice_writer.rs` | planned-missing-source |
| M046 | `cli_args` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/args.rs` | planned-missing-source |
| M047 | `cli_run` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/run.rs` | planned-missing-source |
| M048 | `cli_verify` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/verify.rs` | planned-missing-source |
| M049 | `cli_daemon_once` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/daemon_once.rs` | planned-missing-source |
| M050 | `cli_status` | L06 | C08_CLI_SURFACE | `crates/hle-cli/src/status.rs` | planned-missing-source |
| M051 | `verify_sync_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-sync.sh` | implemented-operational-surface |
| M052 | `quality_gate_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/quality-gate.sh` | implemented-operational-surface |
| M053 | `module_map_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-module-map.sh` | implemented-operational-surface |
| M054 | `layer_dag_script` | L06 | C09_DEVOPS_QI_OPERATIONAL_LANE | `scripts/verify-layer-dag.sh` | implemented-operational-surface |

Legacy M0 crate surfaces currently implemented: `crates/substrate-types`, `crates/substrate-verify`, `crates/substrate-emit`, `crates/hle-cli`. They must be split or expanded into the planned topology before full-codebase deployment is claimed.
