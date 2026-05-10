# Watcher Scaffold Assessment — 2026-05-10

Reviewer: The Watcher ☤
Authority: Luke @ node 0.A
Boundary: R13 quiet; recommendations only, no PBFT proposal
Target: `/home/louranicas/claude-code-workspace/habitat-loop-engine/`
Verdict: PASS_WITH_GAPS
Score: 94 / 100
Source assessment relayed by Luke in-session.

## Scope assessed

The assessment covered the scaffold root, `plan.toml`, deployment framework, FALSE_100_TRAPS, receipts, and Weaver's scaffold verdict against actual files.

Reported delivered surface:

- 134 manifest entries
- 4 Rust crates, skeletal and correctly empty
- 7 layers
- 13 specs, S01-S13
- 12 schematics
- 9 anti-patterns
- 7 use-patterns
- 13 verifier scripts plus aggregator and 15 bin wrappers
- 1 migration
- 3 runbooks
- 3 schemas
- dedicated vault with 17 topic sections plus HOME/MASTER_INDEX
- SHA256SUMS.txt
- `scripts/quality-gate.sh --scaffold` emits the human-readable gate chain and final PASS line.
- `scripts/quality-gate.sh --scaffold --json` runs the same scaffold predicates and emits a `hle.quality_gate.v1` JSON report for CI/Watcher ingestion while streaming step logs to stderr.
- Anchored receipt fields: `^Verdict`, `^Manifest_sha256`, `^Framework_sha256`, `^Counter_evidence_locator`, `^M0_authorized=false`. `^Source_sha256` is legacy compatibility only.

## Weighted scoring

| Facet | Score | Weight | Contribution |
| --- | ---: | ---: | ---: |
| F1 Boundary integrity | 97 | 18% | 17.46 |
| F2 Plan completeness | 96 | 15% | 14.40 |
| F3 Verifier gate | 94 | 18% | 16.92 |
| F4 Receipt graph | 95 | 15% | 14.25 |
| F5 False-100 resistance | 90 | 15% | 13.50 |
| F6 Documentation | 92 | 10% | 9.20 |
| F7 Operational shape | 88 | 9% | 7.92 |
| Final | | 100% | 93.65 |

Rounded mark: 94 / 100.

## Gap clusters

The six-point delta to 100 is concentrated in:

1. false-100 trap automation: FT-03, FT-09, FT-13;
2. pre-M0 ergonomics: `.claude` hooks, `bacon.toml`, and pre-staged crates;
3. receipt/hash clarity and small CI affordances.

None were assessed as blockers. All were assessed as scaffold hardenings.

## Ranked recommendations

| ID | Action | Closes |
| --- | --- | --- |
| R1 | Add `verify-skeleton-only.sh` capping `.rs` LOC per file until `begin M0`; wire into quality gate. | Boundary auto-enforcement |
| R2 | Add framework-hash freshness verification comparing latest receipt source hash to current framework hash. | FT-03 staleness |
| R3 | Add vault parity verification comparing project vault topic sections to framework expected sections. | FT-13 vault drift |
| R4 | Promote at least 3 quality bars from file-count floors to semantic predicates. | FT-09 vanity floor |
| R5 | Pre-stage empty crates for L02/L05/L07: substrate-persistence, substrate-dispatch, substrate-runbook. | M0 readiness |
| R6 | Add `bacon.toml` and `.claude/hooks/pre-commit.sh` running scaffold verification. | Developer ergonomics |
| R7 | Add `--json` output mode to `scripts/quality-gate.sh`. | CI/Watcher ingestion |
| R8 | Split ambiguous `^Source_sha256` into explicit `^Manifest_sha256` and `^Framework_sha256`. | Receipt clarity |
| R9 | Add bin-wrapper parity verification. | Wrapper drift |
| R10 | Add `docs/SCRIPT_SPEC_PREDICATE_MAP.md`. | Auditability |

Single highest-leverage recommendation per Watcher: R5, pre-stage L02/L05/L07 crates, so M0 becomes additive code rather than structural crate creation plus runtime implementation.

## Kanban follow-through

Created scaffold-only hardening triage parent:

- `t_7ab21c50` — HLE Watcher scaffold assessment hardening parent

Created grouped scaffold-only child triage cards:

- `t_e9aa36c1` — R1/R2/R3/R9 verifier automation hardening
- `t_7a51a1ed` — R4 semantic predicate quality bars
- `t_0fa7310a` — R5/R6 pre-M0 ergonomics review
- `t_a40d7bcc` — R7/R8/R10 CI and receipt clarity docs

All cards preserve the boundary: no M0 runtime behavior, no live Habitat integrations, no cron jobs, no daemons, no services, and no deployment claims.

## Boundary status

Current state remains scaffold-only and M0 waiting. The only implementation-enabling phrase remains `begin M0`.
