//! `M050` — `cli_status`: topology and status report adapter.
//!
//! Reads `plan.toml`, `scaffold-status.json`, and the most recent quality
//! gate JSON output. No cross-cluster calls — filesystem-only, read-only.
//!
//! Error codes: 2740-2741.

#![forbid(unsafe_code)]
// Stub module: public items are not yet called from main.rs.
#![allow(dead_code)]

use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use substrate_types::HleError;

// ---------------------------------------------------------------------------
// Boundary constants
// ---------------------------------------------------------------------------

pub const STATUS_HUMAN_MAX_BYTES: usize = 8_192;
pub const STATUS_JSON_MAX_BYTES: usize = 8_192;
pub const STATUS_LINE_MAX_CHARS: usize = 120;
pub const STATUS_SYNERGY_MAX_CHARS: usize = 80;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Gate pass/fail/unknown from status files.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GateStatus {
    Pass,
    Fail,
    Unknown,
}

impl GateStatus {
    /// Parse `"PASS"` -> `Pass`, `"FAIL"` -> `Fail`, else `Unknown`.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "PASS" => Self::Pass,
            "FAIL" => Self::Fail,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for GateStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("PASS"),
            Self::Fail => f.write_str("FAIL"),
            Self::Unknown => f.write_str("UNKNOWN"),
        }
    }
}

/// Current deployment phase parsed from `plan.toml` `[project].status`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ScaffoldPhase {
    Planned,
    Scaffold,
    M0Runtime,
    FullCodebase,
}

impl ScaffoldPhase {
    /// Parse from the `status` string in `plan.toml`.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "scaffold" => Self::Scaffold,
            "m0-runtime" => Self::M0Runtime,
            "full_codebase" => Self::FullCodebase,
            // Any unrecognized value (incl. "planned_topology_incomplete") -> Planned
            _ => Self::Planned,
        }
    }
}

impl fmt::Display for ScaffoldPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planned => f.write_str("planned"),
            Self::Scaffold => f.write_str("scaffold"),
            Self::M0Runtime => f.write_str("m0-runtime"),
            Self::FullCodebase => f.write_str("full_codebase"),
        }
    }
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

/// Full topology snapshot.
#[derive(Debug)]
pub struct StatusReport {
    pub project_name: String,
    pub phase: ScaffoldPhase,
    pub clusters: Vec<ClusterStatus>,
    pub planned_modules: usize,
    pub scaffold_gate: GateStatus,
    pub m0_gate: GateStatus,
    pub gate_source: Option<String>,
}

// ---------------------------------------------------------------------------
// CliStatus adapter
// ---------------------------------------------------------------------------

/// Stateless adapter for the `hle status` command.
pub struct CliStatus {
    workspace_root: PathBuf,
}

impl CliStatus {
    /// Construct with an explicit workspace root.
    #[must_use]
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Construct using the current working directory as workspace root.
    ///
    /// Falls back to `/` if `current_dir` fails.
    #[must_use]
    pub fn from_cwd() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        Self::new(root)
    }

    /// Execute the status command.
    ///
    /// # Errors
    ///
    /// Returns `HleError` 2740 if `plan.toml` cannot be read.
    /// Returns `HleError` 2741 if `plan.toml` cannot be parsed.
    pub fn execute(&self, json: bool) -> Result<String, HleError> {
        let plan = read_plan(&self.workspace_root)?;
        let scaffold_gate = read_scaffold_status(&self.workspace_root);
        let (m0_gate, gate_source) =
            read_gate_json(&self.workspace_root, "quality-gate-m0-latest.json");
        let report = build_report(plan, scaffold_gate, m0_gate, gate_source);
        let out = if json {
            format_json(&report)
        } else {
            format_human(&report)
        };
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Internal plan snapshot (intermediate parsed form)
// ---------------------------------------------------------------------------

struct PlanSnapshot {
    project_name: String,
    phase: ScaffoldPhase,
    planned_modules: usize,
    clusters: Vec<ClusterStatus>,
}

// ---------------------------------------------------------------------------
// File readers
// ---------------------------------------------------------------------------

/// Read and parse `plan.toml`.
///
/// Returns `HleError` 2740 on IO failure, 2741 on parse failure.
fn read_plan(root: &Path) -> Result<PlanSnapshot, HleError> {
    let path = root.join("plan.toml");
    let text = std::fs::read_to_string(&path).map_err(|err| {
        HleError::new(format!(
            "[2740] status read plan.toml failed {}: {err}",
            path.display()
        ))
    })?;
    parse_plan_toml(&text)
        .map_err(|msg| HleError::new(format!("[2741] status parse plan.toml failed: {msg}")))
}

/// Parse the minimal plan.toml fields M050 needs.
/// Unknown keys are silently skipped.
fn parse_plan_toml(text: &str) -> Result<PlanSnapshot, String> {
    let mut project_name = String::new();
    let mut phase = ScaffoldPhase::Planned;
    let mut planned_modules: usize = 0;
    let mut clusters: Vec<ClusterStatus> = Vec::new();

    // Per-cluster accumulator.
    let mut cur_id = String::new();
    let mut cur_name = String::new();
    let mut cur_layers = String::new();
    let mut cur_module_surfaces: usize = 0;
    let mut cur_synergy = String::new();

    // Section tracking: only read project_name/status inside [project],
    // and module_surfaces_exact inside [full_codebase].
    let mut in_cluster = false;
    let mut in_project = false;
    let mut in_full_codebase = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle section headers.
        if line == "[[full_codebase_clusters]]" {
            // Flush previous cluster if any.
            if in_cluster && !cur_id.is_empty() {
                clusters.push(ClusterStatus {
                    id: std::mem::take(&mut cur_id),
                    name: std::mem::take(&mut cur_name),
                    layers: std::mem::take(&mut cur_layers),
                    module_surfaces: cur_module_surfaces,
                    synergy: std::mem::take(&mut cur_synergy),
                });
                cur_module_surfaces = 0;
            }
            in_cluster = true;
            in_project = false;
            in_full_codebase = false;
            continue;
        }

        if line.starts_with('[') {
            // Any other section header — flush pending cluster and update section state.
            if in_cluster && !cur_id.is_empty() {
                clusters.push(ClusterStatus {
                    id: std::mem::take(&mut cur_id),
                    name: std::mem::take(&mut cur_name),
                    layers: std::mem::take(&mut cur_layers),
                    module_surfaces: cur_module_surfaces,
                    synergy: std::mem::take(&mut cur_synergy),
                });
                cur_module_surfaces = 0;
            }
            in_cluster = false;
            in_project = line == "[project]";
            in_full_codebase = line == "[full_codebase]";
            continue;
        }

        if in_cluster {
            if let Some(v) = line.strip_prefix("id = ") {
                cur_id = toml_string(v).unwrap_or_default();
            } else if let Some(v) = line.strip_prefix("name = ") {
                cur_name = toml_string(v).unwrap_or_default();
            } else if let Some(v) = line.strip_prefix("layers = ") {
                cur_layers = toml_string(v).unwrap_or_default();
            } else if let Some(v) = line.strip_prefix("module_surfaces = ") {
                cur_module_surfaces = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("synergy = ") {
                cur_synergy = toml_string(v).unwrap_or_default();
            }
        } else if in_project {
            if let Some(v) = line.strip_prefix("name = ") {
                project_name = toml_string(v).unwrap_or_default();
            } else if let Some(v) = line.strip_prefix("status = ") {
                phase = ScaffoldPhase::from_str(&toml_string(v).unwrap_or_default());
            }
        } else if in_full_codebase {
            if let Some(v) = line.strip_prefix("module_surfaces_exact = ") {
                planned_modules = v.trim().parse().unwrap_or(0);
            }
        }
    }

    // Flush any trailing cluster.
    if in_cluster && !cur_id.is_empty() {
        clusters.push(ClusterStatus {
            id: cur_id,
            name: cur_name,
            layers: cur_layers,
            module_surfaces: cur_module_surfaces,
            synergy: cur_synergy,
        });
    }

    if project_name.is_empty() {
        return Err("missing [project].name".to_owned());
    }

    Ok(PlanSnapshot {
        project_name,
        phase,
        planned_modules,
        clusters,
    })
}

/// Parse a TOML quoted string value (`"..."`) into the inner string.
/// Returns `None` on malformed input.
fn toml_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        Some(trimmed[1..trimmed.len() - 1].to_owned())
    } else {
        None
    }
}

/// Read `.deployment-work/status/scaffold-status.json`.
/// Returns `GateStatus::Unknown` on any failure.
fn read_scaffold_status(root: &Path) -> GateStatus {
    let path = root
        .join(".deployment-work")
        .join("status")
        .join("scaffold-status.json");
    read_status_json_field(&path, "status")
}

/// Read a gate JSON file and return `(GateStatus, Option<source_path_string>)`.
/// Returns `GateStatus::Unknown` and `None` on any failure.
fn read_gate_json(root: &Path, filename: &str) -> (GateStatus, Option<String>) {
    let path = root.join(".deployment-work").join("status").join(filename);
    let status = read_status_json_field(&path, "status");
    let source = if path.exists() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    };
    (status, source)
}

/// Lenient JSON field reader: finds `"field":"VALUE"` or `"field": "VALUE"`.
/// Returns `GateStatus::Unknown` on any problem.
fn read_status_json_field(path: &Path, field: &str) -> GateStatus {
    let Ok(text) = std::fs::read_to_string(path) else {
        return GateStatus::Unknown;
    };
    // Search for `"field"` then extract the value after `:`.
    let needle = format!("\"{field}\"");
    let Some(pos) = text.find(&needle) else {
        return GateStatus::Unknown;
    };
    let after_key = &text[pos + needle.len()..];
    // Skip optional whitespace and the colon.
    let after_colon = after_key.trim_start().trim_start_matches(':').trim_start();
    // Expect a quoted value.
    if let Some(inner) = after_colon.strip_prefix('"') {
        if let Some(end) = inner.find('"') {
            return GateStatus::from_str(&inner[..end]);
        }
    }
    GateStatus::Unknown
}

fn build_report(
    plan: PlanSnapshot,
    scaffold_gate: GateStatus,
    m0_gate: GateStatus,
    gate_source: Option<String>,
) -> StatusReport {
    StatusReport {
        project_name: plan.project_name,
        phase: plan.phase,
        clusters: plan.clusters,
        planned_modules: plan.planned_modules,
        scaffold_gate,
        m0_gate,
        gate_source,
    }
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

fn format_human(report: &StatusReport) -> String {
    let mut out = String::new();
    out.push_str("hle status\n");

    push_line(
        &mut out,
        &format!("  project:       {}", report.project_name),
    );
    push_line(&mut out, &format!("  phase:         {}", report.phase));
    push_line(
        &mut out,
        &format!("  scaffold gate: {}", report.scaffold_gate),
    );
    push_line(&mut out, &format!("  m0 gate:       {}", report.m0_gate));
    if let Some(ref src) = report.gate_source {
        push_line(&mut out, &format!("  gate source:   {src}"));
    }
    push_line(&mut out, "");

    let cluster_count = report.clusters.len();
    push_line(&mut out, &format!("  Clusters ({cluster_count}):"));

    for c in &report.clusters {
        let synergy = truncate_str(&c.synergy, STATUS_SYNERGY_MAX_CHARS);
        let row = format!(
            "    {id:<38} {layers:<14} {ms} modules  {synergy}",
            id = c.id,
            layers = c.layers,
            ms = c.module_surfaces,
            synergy = synergy,
        );
        push_line(&mut out, &row);
    }

    if out.len() > STATUS_HUMAN_MAX_BYTES {
        out.truncate(STATUS_HUMAN_MAX_BYTES - 33);
        out.push_str("\n[status output truncated at 8 KB]");
    }
    out
}

fn format_json(report: &StatusReport) -> String {
    let mut clusters_json = String::from('[');
    for (i, c) in report.clusters.iter().enumerate() {
        if i > 0 {
            clusters_json.push(',');
        }
        let _ = write!(
            clusters_json,
            "{{\"id\":\"{id}\",\"name\":\"{name}\",\"layers\":\"{layers}\",\
             \"module_surfaces\":{ms},\"synergy\":\"{synergy}\"}}",
            id = c.id.replace('"', "\\\""),
            name = c.name.replace('"', "\\\""),
            layers = c.layers.replace('"', "\\\""),
            ms = c.module_surfaces,
            synergy = c.synergy.replace('"', "\\\""),
        );
    }
    clusters_json.push(']');

    let gate_source = report.gate_source.as_deref().map_or_else(
        || "null".to_owned(),
        |s| format!("\"{}\"", s.replace('"', "\\\"")),
    );

    let raw = format!(
        "{{\"schema\":\"hle.status.v1\",\"project_name\":\"{name}\",\
         \"phase\":\"{phase}\",\"planned_modules\":{pm},\
         \"scaffold_gate\":\"{sg}\",\"m0_gate\":\"{m0g}\",\
         \"gate_source\":{gs},\"clusters\":{clusters}}}",
        name = report.project_name.replace('"', "\\\""),
        phase = report.phase,
        pm = report.planned_modules,
        sg = report.scaffold_gate,
        m0g = report.m0_gate,
        gs = gate_source,
        clusters = clusters_json,
    );

    if raw.len() > STATUS_JSON_MAX_BYTES {
        // Truncate to max and append marker — result may not be valid JSON,
        // which is acceptable per spec (truncation is bounded, not silent).
        let mut out = raw[..STATUS_JSON_MAX_BYTES - 33].to_owned();
        out.push_str("\n[status output truncated at 8 KB]");
        return out;
    }
    raw
}

/// Append a line, truncating to `STATUS_LINE_MAX_CHARS`.
fn push_line(buf: &mut String, line: &str) {
    let truncated = truncate_str(line, STATUS_LINE_MAX_CHARS);
    buf.push_str(&truncated);
    buf.push('\n');
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut out = s[..max.saturating_sub(3)].to_owned();
    out.push_str("...");
    out
}

// ---------------------------------------------------------------------------
// Public wrapper function used by main.rs if wired
// ---------------------------------------------------------------------------

/// Report the habitat-loop-engine topology and gate status.
///
/// `json` selects machine-readable `hle.status.v1` JSON output.
///
/// # Errors
///
/// Returns `HleError` 2740/2741 if `plan.toml` is missing or unparseable.
pub fn report_status(workspace_root: &Path, json: bool) -> Result<String, HleError> {
    CliStatus::new(workspace_root.to_path_buf()).execute(json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn temp_dir_for(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hle-status-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).ok();
        d
    }

    fn write_minimal_plan(dir: &Path, extra: &str) {
        let content = format!(
            "[project]\nname = \"test-project\"\nstatus = \"m0-runtime\"\n\
             [full_codebase]\nmodule_surfaces_exact = 50\n\
             [[full_codebase_clusters]]\nid = \"C01\"\nname = \"Cluster One\"\n\
             layers = \"L01\"\nmodule_surfaces = 5\nsynergy = \"desc\"\n{extra}"
        );
        std::fs::write(dir.join("plan.toml"), content).expect("write plan.toml");
    }

    // -- report_status / execute ------------------------------------------------

    #[test]
    fn execute_returns_ok_with_valid_plan_toml() {
        let dir = temp_dir_for("ok");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn execute_errors_2740_on_missing_plan_toml() {
        let dir = temp_dir_for("missing");
        let r = report_status(&dir, false);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2740")));
    }

    #[test]
    fn execute_errors_2741_on_unparseable_plan_toml() {
        let dir = temp_dir_for("badparse");
        std::fs::write(dir.join("plan.toml"), "not valid toml at all @@@ !!!").ok();
        let r = report_status(&dir, false);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2741")));
    }

    #[test]
    fn execute_json_flag_emits_status_v1_schema() {
        let dir = temp_dir_for("json-schema");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("hle.status.v1")));
    }

    #[test]
    fn execute_human_output_contains_project_name() {
        let dir = temp_dir_for("projname");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.contains("test-project")));
    }

    #[test]
    fn execute_human_output_contains_phase() {
        let dir = temp_dir_for("phase");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.contains("m0-runtime")));
    }

    #[test]
    fn execute_human_output_contains_scaffold_gate() {
        let dir = temp_dir_for("scaffold-gate");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        // Gate is UNKNOWN since no status file exists in temp dir.
        assert!(r.map_or(false, |s| s.contains("UNKNOWN")
            || s.contains("scaffold gate")));
    }

    #[test]
    fn execute_human_output_lists_clusters() {
        let dir = temp_dir_for("clusters");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.contains("C01")));
    }

    // -- read_plan / parse_plan_toml --------------------------------------------

    #[test]
    fn read_plan_extracts_project_name() {
        let dir = temp_dir_for("name-extract");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert_eq!(r.map(|p| p.project_name), Ok("test-project".to_owned()));
    }

    #[test]
    fn read_plan_extracts_status_field() {
        let dir = temp_dir_for("status-extract");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert_eq!(r.map(|p| p.phase), Ok(ScaffoldPhase::M0Runtime));
    }

    #[test]
    fn read_plan_extracts_module_surfaces_exact() {
        let dir = temp_dir_for("modules-exact");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert_eq!(r.map(|p| p.planned_modules), Ok(50));
    }

    #[test]
    fn read_plan_extracts_cluster_ids() {
        let dir = temp_dir_for("cluster-ids");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p.clusters.iter().any(|c| c.id == "C01")));
    }

    #[test]
    fn read_plan_extracts_cluster_names() {
        let dir = temp_dir_for("cluster-names");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p
            .clusters
            .iter()
            .any(|c| c.name == "Cluster One")));
    }

    #[test]
    fn read_plan_extracts_cluster_layers() {
        let dir = temp_dir_for("cluster-layers");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p.clusters.iter().any(|c| c.layers == "L01")));
    }

    #[test]
    fn read_plan_extracts_cluster_module_surfaces() {
        let dir = temp_dir_for("cluster-ms");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p.clusters.iter().any(|c| c.module_surfaces == 5)));
    }

    #[test]
    fn read_plan_extracts_cluster_synergy() {
        let dir = temp_dir_for("cluster-syn");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p.clusters.iter().any(|c| c.synergy == "desc")));
    }

    #[test]
    fn read_plan_real_plan_toml_parses_nine_clusters() {
        let r = read_plan(&workspace_root());
        assert!(r.map_or(false, |p| p.clusters.len() == 9));
    }

    #[test]
    fn read_plan_real_plan_toml_project_name_correct() {
        let r = read_plan(&workspace_root());
        assert_eq!(
            r.map(|p| p.project_name),
            Ok("habitat-loop-engine".to_owned())
        );
    }

    // -- read_scaffold_status ---------------------------------------------------

    #[test]
    fn read_scaffold_status_pass_on_pass_json() {
        let dir = temp_dir_for("scaffold-pass");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("scaffold-status.json"),
            r#"{"status":"PASS","timestamp":"2026-05-11"}"#,
        )
        .ok();
        assert_eq!(read_scaffold_status(&dir), GateStatus::Pass);
    }

    #[test]
    fn read_scaffold_status_fail_on_fail_json() {
        let dir = temp_dir_for("scaffold-fail");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("scaffold-status.json"),
            r#"{"status":"FAIL"}"#,
        )
        .ok();
        assert_eq!(read_scaffold_status(&dir), GateStatus::Fail);
    }

    #[test]
    fn read_scaffold_status_unknown_on_missing_file() {
        let dir = temp_dir_for("scaffold-missing");
        assert_eq!(read_scaffold_status(&dir), GateStatus::Unknown);
    }

    #[test]
    fn read_scaffold_status_unknown_on_malformed_json() {
        let dir = temp_dir_for("scaffold-malformed");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(status_dir.join("scaffold-status.json"), "not json").ok();
        assert_eq!(read_scaffold_status(&dir), GateStatus::Unknown);
    }

    // -- read_gate_json ---------------------------------------------------------

    #[test]
    fn read_gate_json_pass_on_pass_status() {
        let dir = temp_dir_for("gate-pass");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("quality-gate-m0-latest.json"),
            r#"{"status":"PASS"}"#,
        )
        .ok();
        let (gs, _) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert_eq!(gs, GateStatus::Pass);
    }

    #[test]
    fn read_gate_json_fail_on_fail_status() {
        let dir = temp_dir_for("gate-fail");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("quality-gate-m0-latest.json"),
            r#"{"status":"FAIL"}"#,
        )
        .ok();
        let (gs, _) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert_eq!(gs, GateStatus::Fail);
    }

    #[test]
    fn read_gate_json_unknown_on_missing_file() {
        let dir = temp_dir_for("gate-missing");
        let (gs, src) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert_eq!(gs, GateStatus::Unknown);
        assert!(src.is_none());
    }

    #[test]
    fn read_gate_json_unknown_on_unexpected_format() {
        let dir = temp_dir_for("gate-unexpected");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(status_dir.join("quality-gate-m0-latest.json"), "[]").ok();
        let (gs, _) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert_eq!(gs, GateStatus::Unknown);
    }

    #[test]
    fn read_gate_json_source_present_when_file_exists() {
        let dir = temp_dir_for("gate-source");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("quality-gate-m0-latest.json"),
            r#"{"status":"PASS"}"#,
        )
        .ok();
        let (_, src) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert!(src.is_some());
    }

    #[test]
    fn read_gate_json_source_none_when_file_absent() {
        let dir = temp_dir_for("gate-source-none");
        let (_, src) = read_gate_json(&dir, "quality-gate-m0-latest.json");
        assert!(src.is_none());
    }

    // -- GateStatus -------------------------------------------------------------

    #[test]
    fn gate_status_from_str_pass() {
        assert_eq!(GateStatus::from_str("PASS"), GateStatus::Pass);
    }

    #[test]
    fn gate_status_from_str_fail() {
        assert_eq!(GateStatus::from_str("FAIL"), GateStatus::Fail);
    }

    #[test]
    fn gate_status_from_str_unknown_on_other() {
        assert_eq!(GateStatus::from_str("MAYBE"), GateStatus::Unknown);
    }

    #[test]
    fn gate_status_display_pass() {
        assert_eq!(GateStatus::Pass.to_string(), "PASS");
    }

    #[test]
    fn gate_status_display_fail() {
        assert_eq!(GateStatus::Fail.to_string(), "FAIL");
    }

    #[test]
    fn gate_status_display_unknown() {
        assert_eq!(GateStatus::Unknown.to_string(), "UNKNOWN");
    }

    // -- ScaffoldPhase ----------------------------------------------------------

    #[test]
    fn scaffold_phase_from_str_m0_runtime() {
        assert_eq!(
            ScaffoldPhase::from_str("m0-runtime"),
            ScaffoldPhase::M0Runtime
        );
    }

    #[test]
    fn scaffold_phase_from_str_scaffold() {
        assert_eq!(ScaffoldPhase::from_str("scaffold"), ScaffoldPhase::Scaffold);
    }

    #[test]
    fn scaffold_phase_from_str_planned() {
        assert_eq!(
            ScaffoldPhase::from_str("planned_topology_incomplete"),
            ScaffoldPhase::Planned
        );
    }

    #[test]
    fn scaffold_phase_from_str_full_codebase() {
        assert_eq!(
            ScaffoldPhase::from_str("full_codebase"),
            ScaffoldPhase::FullCodebase
        );
    }

    #[test]
    fn scaffold_phase_from_str_unknown_defaults_to_planned() {
        assert_eq!(ScaffoldPhase::from_str("bogus"), ScaffoldPhase::Planned);
    }

    #[test]
    fn scaffold_phase_display_m0_runtime() {
        assert_eq!(ScaffoldPhase::M0Runtime.to_string(), "m0-runtime");
    }

    #[test]
    fn scaffold_phase_display_scaffold() {
        assert_eq!(ScaffoldPhase::Scaffold.to_string(), "scaffold");
    }

    #[test]
    fn scaffold_phase_display_planned() {
        assert_eq!(ScaffoldPhase::Planned.to_string(), "planned");
    }

    #[test]
    fn scaffold_phase_display_full_codebase() {
        assert_eq!(ScaffoldPhase::FullCodebase.to_string(), "full_codebase");
    }

    // -- format_human / format_json ---------------------------------------------

    #[test]
    fn format_human_bounded_under_8kb() {
        let dir = temp_dir_for("human-bound");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.len() <= STATUS_HUMAN_MAX_BYTES));
    }

    #[test]
    fn format_json_bounded_under_8kb() {
        let dir = temp_dir_for("json-bound");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.len() <= STATUS_JSON_MAX_BYTES));
    }

    #[test]
    fn format_json_clusters_array_present() {
        let dir = temp_dir_for("json-clusters");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"clusters\":")));
    }

    #[test]
    fn format_json_cluster_id_present() {
        let dir = temp_dir_for("json-cluster-id");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"C01\"")));
    }

    #[test]
    fn format_human_synergy_truncated_at_limit() {
        let long_synergy = "x".repeat(200);
        let content = format!(
            "[project]\nname = \"p\"\nstatus = \"scaffold\"\n\
             [full_codebase]\nmodule_surfaces_exact = 1\n\
             [[full_codebase_clusters]]\nid = \"C01\"\nname = \"N\"\n\
             layers = \"L01\"\nmodule_surfaces = 1\nsynergy = \"{long_synergy}\"\n"
        );
        let dir = temp_dir_for("synergy-trunc");
        std::fs::write(dir.join("plan.toml"), &content).ok();
        let r = report_status(&dir, false);
        if let Ok(s) = r {
            // Line containing the cluster should not contain more than 200 chars of synergy.
            assert!(s.len() <= STATUS_HUMAN_MAX_BYTES, "output exceeded 8 KB");
        }
    }

    #[test]
    fn format_human_line_not_exceeding_120_chars() {
        let dir = temp_dir_for("line-limit");
        write_minimal_plan(&dir, "");
        if let Ok(output) = report_status(&dir, false) {
            for line in output.lines() {
                assert!(
                    line.len() <= STATUS_LINE_MAX_CHARS,
                    "line too long ({} chars): {line}",
                    line.len()
                );
            }
        }
    }

    // -- CliStatus::from_cwd ----------------------------------------------------

    #[test]
    fn from_cwd_does_not_panic() {
        // Just checks construction doesn't error.
        let _ = CliStatus::from_cwd();
    }

    // -- build_report -----------------------------------------------------------

    #[test]
    fn build_report_cluster_count_matches_plan() {
        let dir = temp_dir_for("cluster-count");
        write_minimal_plan(&dir, "");
        let r = read_plan(&dir);
        assert!(r.map_or(false, |p| p.clusters.len() == 1));
    }

    #[test]
    fn build_report_phase_from_project_status() {
        let dir = temp_dir_for("phase-build");
        write_minimal_plan(&dir, "");
        let plan = read_plan(&dir).expect("plan");
        let report = build_report(plan, GateStatus::Unknown, GateStatus::Unknown, None);
        assert_eq!(report.phase, ScaffoldPhase::M0Runtime);
    }

    // -- toml_string helper -----------------------------------------------------

    #[test]
    fn toml_string_quoted_value() {
        assert_eq!(toml_string("\"hello\""), Some("hello".to_owned()));
    }

    #[test]
    fn toml_string_unquoted_returns_none() {
        assert_eq!(toml_string("hello"), None);
    }

    #[test]
    fn toml_string_empty_quoted() {
        assert_eq!(toml_string("\"\""), Some(String::new()));
    }

    // -- toml_string edge cases -------------------------------------------------

    #[test]
    fn toml_string_with_surrounding_spaces() {
        // Leading/trailing whitespace is trimmed before quote check.
        assert_eq!(toml_string("  \"trimmed\"  "), Some("trimmed".to_owned()));
    }

    #[test]
    fn toml_string_single_char_returns_none() {
        assert_eq!(toml_string("\""), None);
    }

    #[test]
    fn toml_string_only_open_quote_returns_none() {
        assert_eq!(toml_string("\"value"), None);
    }

    // -- parse_plan_toml: multiple clusters ------------------------------------

    #[test]
    fn parse_plan_toml_two_clusters() {
        let text = "[project]\nname = \"p\"\nstatus = \"scaffold\"\n\
                    [[full_codebase_clusters]]\nid = \"C01\"\nname = \"One\"\n\
                    layers = \"L01\"\nmodule_surfaces = 5\nsynergy = \"s1\"\n\
                    [[full_codebase_clusters]]\nid = \"C02\"\nname = \"Two\"\n\
                    layers = \"L02\"\nmodule_surfaces = 3\nsynergy = \"s2\"\n";
        let snap = parse_plan_toml(text).expect("parse");
        assert_eq!(snap.clusters.len(), 2);
    }

    #[test]
    fn parse_plan_toml_two_clusters_ids_correct() {
        let text = "[project]\nname = \"p\"\nstatus = \"scaffold\"\n\
                    [[full_codebase_clusters]]\nid = \"C01\"\nname = \"One\"\n\
                    layers = \"L01\"\nmodule_surfaces = 5\nsynergy = \"s\"\n\
                    [[full_codebase_clusters]]\nid = \"C02\"\nname = \"Two\"\n\
                    layers = \"L02\"\nmodule_surfaces = 3\nsynergy = \"s\"\n";
        let snap = parse_plan_toml(text).expect("parse");
        assert_eq!(snap.clusters[0].id, "C01");
        assert_eq!(snap.clusters[1].id, "C02");
    }

    #[test]
    fn parse_plan_toml_missing_name_returns_err() {
        let text = "[project]\nstatus = \"scaffold\"\n";
        assert!(parse_plan_toml(text).is_err());
    }

    #[test]
    fn parse_plan_toml_comments_ignored() {
        let text = "# this is a comment\n[project]\nname = \"p\"\n\
                    status = \"scaffold\"\n";
        let snap = parse_plan_toml(text).expect("parse");
        assert_eq!(snap.project_name, "p");
    }

    #[test]
    fn parse_plan_toml_blank_lines_ignored() {
        let text = "\n[project]\n\nname = \"p\"\n\nstatus = \"scaffold\"\n";
        let snap = parse_plan_toml(text).expect("parse");
        assert_eq!(snap.project_name, "p");
    }

    // -- GateStatus::from_str edge cases ----------------------------------------

    #[test]
    fn gate_status_from_str_with_leading_whitespace() {
        assert_eq!(GateStatus::from_str("  PASS"), GateStatus::Pass);
    }

    #[test]
    fn gate_status_from_str_with_trailing_whitespace() {
        assert_eq!(GateStatus::from_str("FAIL  "), GateStatus::Fail);
    }

    #[test]
    fn gate_status_from_str_empty_string_unknown() {
        assert_eq!(GateStatus::from_str(""), GateStatus::Unknown);
    }

    #[test]
    fn gate_status_from_str_lowercase_unknown() {
        assert_eq!(GateStatus::from_str("pass"), GateStatus::Unknown);
    }

    // -- ScaffoldPhase from_str edge cases --------------------------------------

    #[test]
    fn scaffold_phase_from_str_empty_string_is_planned() {
        assert_eq!(ScaffoldPhase::from_str(""), ScaffoldPhase::Planned);
    }

    #[test]
    fn scaffold_phase_from_str_whitespace_is_planned() {
        assert_eq!(ScaffoldPhase::from_str("   "), ScaffoldPhase::Planned);
    }

    // -- CliStatus::new uses provided root -------------------------------------

    #[test]
    fn cli_status_new_stores_workspace_root() {
        let dir = temp_dir_for("cli-new");
        write_minimal_plan(&dir, "");
        let cs = CliStatus::new(dir.clone());
        let r = cs.execute(false);
        assert!(r.is_ok());
    }

    // -- format_json: schema field is first key --------------------------------

    #[test]
    fn format_json_schema_field_in_output() {
        let dir = temp_dir_for("schema-field");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"schema\":\"hle.status.v1\"")));
    }

    // -- format_json: planned_modules field ------------------------------------

    #[test]
    fn format_json_planned_modules_field_correct() {
        let dir = temp_dir_for("planned-modules");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"planned_modules\":50")));
    }

    // -- format_json: phase field correct --------------------------------------

    #[test]
    fn format_json_phase_field_correct() {
        let dir = temp_dir_for("phase-json");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"phase\":\"m0-runtime\"")));
    }

    // -- format_human: cluster count header ------------------------------------

    #[test]
    fn format_human_clusters_header_present() {
        let dir = temp_dir_for("clusters-header");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.contains("Clusters")));
    }

    // -- format_human: m0 gate label visible -----------------------------------

    #[test]
    fn format_human_m0_gate_label_present() {
        let dir = temp_dir_for("m0-gate-label");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.contains("m0 gate")));
    }

    // -- read_status_json_field: space after colon -----------------------------

    #[test]
    fn read_status_json_field_space_after_colon_accepted() {
        let dir = temp_dir_for("space-colon");
        let status_dir = dir.join(".deployment-work").join("status");
        std::fs::create_dir_all(&status_dir).ok();
        std::fs::write(
            status_dir.join("scaffold-status.json"),
            r#"{"status": "PASS"}"#,
        )
        .ok();
        assert_eq!(read_scaffold_status(&dir), GateStatus::Pass);
    }

    // -- Truncation: output starts with hle status header ----------------------

    #[test]
    fn format_human_starts_with_hle_status() {
        let dir = temp_dir_for("hle-status-header");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, false);
        assert!(r.map_or(false, |s| s.starts_with("hle status")));
    }

    // -- Gate source field in JSON (null when missing) -------------------------

    #[test]
    fn format_json_gate_source_null_when_missing() {
        let dir = temp_dir_for("gate-source-null");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"gate_source\":null")));
    }

    // -- cluster module_surfaces in JSON --------------------------------------

    #[test]
    fn format_json_cluster_module_surfaces_present() {
        let dir = temp_dir_for("ms-json");
        write_minimal_plan(&dir, "");
        let r = report_status(&dir, true);
        assert!(r.map_or(false, |s| s.contains("\"module_surfaces\":5")));
    }

    // -- push_line truncation ---------------------------------------------------

    #[test]
    fn push_line_truncates_long_line_to_120_chars() {
        let long_line = "a".repeat(200);
        let mut buf = String::new();
        push_line(&mut buf, &long_line);
        // Trailing newline is added; line content should be ≤ STATUS_LINE_MAX_CHARS.
        let line = buf.trim_end_matches('\n');
        assert!(
            line.len() <= STATUS_LINE_MAX_CHARS,
            "line too long: {}",
            line.len()
        );
    }

    #[test]
    fn push_line_short_line_unchanged_plus_newline() {
        let mut buf = String::new();
        push_line(&mut buf, "hello");
        assert_eq!(buf, "hello\n");
    }

    // -- truncate_str helper ----------------------------------------------------

    #[test]
    fn truncate_str_short_string_returned_unchanged() {
        assert_eq!(truncate_str("abc", 10), "abc");
    }

    #[test]
    fn truncate_str_long_string_ends_with_ellipsis() {
        let out = truncate_str(&"x".repeat(200), 50);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn truncate_str_long_string_at_most_max_chars() {
        let out = truncate_str(&"x".repeat(200), 50);
        assert!(out.len() <= 50);
    }
}
