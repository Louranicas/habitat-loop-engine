# M050 — cli_status

> **File:** `crates/hle-cli/src/status.rs` | **Layer:** L06 | **Cluster:** C08_CLI_SURFACE
> **Error codes:** 2740-2741 | **Role:** topology and status report adapter

---

## Purpose

M050 is the adapter for the `hle status` operator command. It reads `plan.toml`, `scaffold-status.json`, and the most recent quality gate JSON output to produce a topology and health report of the current codebase deployment state. It has no dependency on M047, M048, or M049 — it reads static configuration and status files only. It does not execute workflows, call verifiers, or write to any store.

The primary output surfaces are a human-readable multi-line report and a machine-readable `hle.status.v1` JSON object controlled by the `--json` flag.

---

## Types at a Glance

| Type | Kind | Role |
|---|---|---|
| `CliStatus` | struct | Stateless status reader; no injected dependencies |
| `StatusArgs` | struct (re-export from M046) | Validated status command arguments |
| `StatusReport` | struct | Full topology snapshot built from plan + status files |
| `ClusterStatus` | struct | Per-cluster row: id, name, module count, gate status |
| `GateStatus` | enum | `Pass`, `Fail`, `Unknown` |
| `ScaffoldPhase` | enum | `Planned`, `Scaffold`, `M0Runtime`, `FullCodebase` |

---

## Rust Signatures

```rust
use crate::args::StatusArgs;
use substrate_types::HleError;

/// Stateless adapter for the `hle status` command.
/// Reads plan.toml, scaffold-status.json, and the most recent gate JSON.
pub struct CliStatus {
    /// Workspace root used to locate plan.toml and status files.
    /// Defaults to the current working directory.
    workspace_root: std::path::PathBuf,
}

impl CliStatus {
    /// Construct with an explicit workspace root.
    #[must_use]
    pub fn new(workspace_root: std::path::PathBuf) -> Self;

    /// Construct using the current working directory as workspace root.
    #[must_use]
    pub fn from_cwd() -> Self;

    /// Execute the status command.
    ///
    /// Reads plan.toml and optional status/gate JSON files.
    /// Missing optional files produce `GateStatus::Unknown` for affected fields.
    /// Returns `Err(HleError)` only if plan.toml cannot be read (code 2740)
    /// or cannot be parsed (code 2741).
    ///
    /// Returns a bounded string: human-readable (default) or
    /// hle.status.v1 JSON (when `args.json` is true).
    pub fn execute(&self, args: &StatusArgs) -> Result<String, HleError>;
}

/// Full topology snapshot.
#[derive(Debug)]
pub struct StatusReport {
    /// Project name from plan.toml.
    pub project_name: String,
    /// Current deployment phase.
    pub phase: ScaffoldPhase,
    /// Cluster rows from plan.toml `[[full_codebase_clusters]]`.
    pub clusters: Vec<ClusterStatus>,
    /// Total planned module surfaces.
    pub planned_modules: usize,
    /// Scaffold gate status from scaffold-status.json.
    pub scaffold_gate: GateStatus,
    /// M0 runtime gate status from quality-gate-m0-latest.json.
    pub m0_gate: GateStatus,
    /// Timestamp or path of the most recent gate JSON, if present.
    pub gate_source: Option<String>,
}

/// Per-cluster row in the topology report.
#[derive(Debug)]
pub struct ClusterStatus {
    pub id: String,
    pub name: String,
    pub layers: String,
    pub module_surfaces: usize,
    pub synergy: String,
}

/// Gate pass/fail/unknown from status files.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GateStatus {
    Pass,
    Fail,
    Unknown,
}

/// Current deployment phase.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ScaffoldPhase {
    /// plan.toml `status = "planned_topology_incomplete"`.
    Planned,
    /// plan.toml `status = "scaffold"`.
    Scaffold,
    /// plan.toml `status = "m0-runtime"`.
    M0Runtime,
    /// plan.toml `status = "full_codebase"`.
    FullCodebase,
}
```

---

## Method / Trait Table

| Item | Signature | Notes |
|---|---|---|
| `CliStatus::new` | `pub fn(workspace_root: PathBuf) -> Self` | Explicit root |
| `CliStatus::from_cwd` | `pub fn() -> Self` | Convenience; uses `std::env::current_dir().unwrap_or_default()` in main; must be called with `?` propagation context |
| `CliStatus::execute` | `pub fn(&self, args: &StatusArgs) -> Result<String, HleError>` | Main entry point |
| `read_plan` | `fn(root: &Path) -> Result<PlanSnapshot, HleError>` | Private; reads and parses plan.toml; 2740/2741 |
| `read_scaffold_status` | `fn(root: &Path) -> GateStatus` | Private; returns Unknown if file absent |
| `read_gate_json` | `fn(root: &Path, filename: &str) -> GateStatus` | Private; reads `.deployment-work/status/{filename}` |
| `build_report` | `fn(plan, scaffold_gate, m0_gate, gate_source) -> StatusReport` | Private; assembles `StatusReport` |
| `format_human` | `fn(report: &StatusReport) -> String` | Private; bounded 8 KB multi-line |
| `format_json` | `fn(report: &StatusReport) -> String` | Private; bounded 8 KB single-object |
| `GateStatus::from_str` | `fn(s: &str) -> Self` | Parses `"PASS"` -> Pass, `"FAIL"` -> Fail, else Unknown |
| `ScaffoldPhase::from_str` | `fn(s: &str) -> Self` | Parses plan.toml `status` field |
| `Display for GateStatus` | impl | `"PASS"` / `"FAIL"` / `"UNKNOWN"` |
| `Display for ScaffoldPhase` | impl | `"planned"` / `"scaffold"` / `"m0-runtime"` / `"full_codebase"` |

---

## Status File Locations

All paths are resolved relative to `self.workspace_root`:

| File | Path | Required |
|---|---|---|
| Plan | `plan.toml` | Yes — `Err(2740/2741)` if absent/invalid |
| Scaffold status | `.deployment-work/status/scaffold-status.json` | No — `GateStatus::Unknown` if absent |
| M0 gate latest | `.deployment-work/status/quality-gate-m0-latest.json` | No — `GateStatus::Unknown` if absent |

The scaffold-status.json is expected to contain a `"status"` field with value `"PASS"` or `"FAIL"`. The quality-gate JSON is expected to contain a top-level `"status"` field. Both parsers are lenient: any format that does not match expected structure returns `GateStatus::Unknown` rather than propagating an error.

---

## Design Notes

### Filesystem-only, no cross-cluster calls

M050 has no trait-injected dependencies and makes no calls into C01-C07 modules. It reads only static files. This is intentional: `hle status` must work even when the executor, verifier, and stores are not compiled yet (e.g., during scaffold-only phases). The `CliStatus` struct takes no generic parameters.

### Lenient optional file handling

`read_scaffold_status` and `read_gate_json` never return `Err`. A missing file, unreadable file, or unrecognized JSON structure all produce `GateStatus::Unknown`. Only `read_plan` propagates errors, because `plan.toml` is the canonical source of truth: without it, there is nothing to report.

### plan.toml minimal parser

M050 contains a minimal hand-rolled TOML reader that extracts:

- `[project].name` (string)
- `[project].status` (string)
- `[full_codebase].module_surfaces_exact` (integer)
- All `[[full_codebase_clusters]]` tables: `id`, `name`, `layers`, `module_surfaces`, `synergy`

It does not attempt to parse the full TOML grammar. Unknown keys are silently skipped. This avoids adding a TOML parsing dep to `hle-cli`.

### Human-readable output format

```
hle status
  project:       habitat-loop-engine
  phase:         m0-runtime
  scaffold gate: PASS
  m0 gate:       PASS
  gate source:   .deployment-work/status/quality-gate-m0-latest.json

  Clusters (9):
    C01_EVIDENCE_INTEGRITY   L01/L02/L04  5 modules  receipt hash -> claim store -> ...
    C02_AUTHORITY_STATE      L01/L03/L04  5 modules  type-state authority ...
    ...
    C08_CLI_SURFACE          L06          5 modules  operator commands remain thin adapters
    C09_DEVOPS_QI_OPERATIONAL_LANE  L06/scripts  4 modules  scripts enforce docs-source-gate parity
```

Lines are bounded to 120 characters each; long synergy strings are truncated with `...`.

### JSON output schema (hle.status.v1)

```json
{
  "schema": "hle.status.v1",
  "project_name": "habitat-loop-engine",
  "phase": "m0-runtime",
  "planned_modules": 50,
  "scaffold_gate": "PASS",
  "m0_gate": "PASS",
  "gate_source": ".deployment-work/status/quality-gate-m0-latest.json",
  "clusters": [
    {
      "id": "C01_EVIDENCE_INTEGRITY",
      "name": "Evidence Integrity",
      "layers": "L01/L02/L04",
      "module_surfaces": 5,
      "synergy": "receipt hash -> claim store -> ..."
    }
  ]
}
```

The JSON is bounded to 8 KB. The cluster array preserves the order from `plan.toml`.

### Bounded output constants

```rust
pub const STATUS_HUMAN_MAX_BYTES: usize = 8_192;
pub const STATUS_JSON_MAX_BYTES: usize = 8_192;
pub const STATUS_LINE_MAX_CHARS: usize = 120;
pub const STATUS_SYNERGY_MAX_CHARS: usize = 80;
```

If the full report exceeds 8 KB, a truncation notice is appended: `"\n[status output truncated at 8 KB]"`.

### Test surface (minimum 50 tests)

- `execute_returns_ok_with_valid_plan_toml`
- `execute_errors_2740_on_missing_plan_toml`
- `execute_errors_2741_on_unparseable_plan_toml`
- `execute_json_flag_emits_status_v1_schema`
- `execute_human_output_contains_project_name`
- `execute_human_output_contains_phase`
- `execute_human_output_contains_scaffold_gate`
- `execute_human_output_contains_m0_gate`
- `execute_human_output_lists_clusters`
- `read_plan_extracts_project_name`
- `read_plan_extracts_status_field`
- `read_plan_extracts_module_surfaces_exact`
- `read_plan_extracts_cluster_ids`
- `read_plan_extracts_cluster_names`
- `read_plan_extracts_cluster_layers`
- `read_plan_extracts_cluster_module_surfaces`
- `read_plan_extracts_cluster_synergy`
- `read_scaffold_status_pass_on_pass_json`
- `read_scaffold_status_fail_on_fail_json`
- `read_scaffold_status_unknown_on_missing_file`
- `read_scaffold_status_unknown_on_malformed_json`
- `read_gate_json_pass_on_pass_status`
- `read_gate_json_fail_on_fail_status`
- `read_gate_json_unknown_on_missing_file`
- `read_gate_json_unknown_on_unexpected_format`
- `build_report_cluster_count_matches_plan`
- `build_report_phase_from_project_status`
- `scaffold_phase_from_str_m0_runtime`
- `scaffold_phase_from_str_scaffold`
- `scaffold_phase_from_str_planned`
- `scaffold_phase_from_str_full_codebase`
- `scaffold_phase_from_str_unknown_defaults_to_planned`
- `gate_status_from_str_pass`
- `gate_status_from_str_fail`
- `gate_status_from_str_unknown_on_other`
- `gate_status_display_pass`
- `gate_status_display_fail`
- `gate_status_display_unknown`
- `format_human_bounded_under_8kb`
- `format_json_bounded_under_8kb`
- `format_json_single_json_object`
- `format_json_clusters_array_present`
- `format_json_cluster_id_present`
- `format_human_synergy_truncated_at_limit`
- `format_human_line_not_exceeding_120_chars`
- `execute_gate_source_field_present_when_file_exists`
- `execute_gate_source_none_when_file_absent`
- `execute_from_cwd_resolves_plan_toml`
- ... (additional edge cases to meet 50-test minimum)

---

*M050 cli_status Spec v1.0 | C08_CLI_SURFACE | 2026-05-10*
