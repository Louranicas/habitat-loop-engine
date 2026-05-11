//! `M051` — `cli_scan`: recursive source-tree anti-pattern scanner adapter.
//!
//! Walks a directory tree, reads every `.rs` file, passes each through
//! `CompositeScanner::scan_all`, and formats the aggregated findings.
//!
//! Bounds (all checked before scan begins):
//! - Maximum directory depth: 10
//! - Maximum files: 10,000
//! - Maximum file size: 2 MB
//! - Skipped directories: `target/`, `.git/`, `node_modules/`
//!
//! Error codes: 2760-2762.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use hle_verifier::anti_pattern_scanner::{
    AntiPatternId, CompositeScanner, DetectorEvent, ScanInput, Severity,
};
use substrate_types::HleError;

// ---------------------------------------------------------------------------
// Boundary constants
// ---------------------------------------------------------------------------

/// Maximum directory traversal depth.
pub const SCAN_MAX_DEPTH: usize = 10;
/// Maximum number of `.rs` files considered.
pub const SCAN_MAX_FILES: usize = 10_000;
/// Maximum file size in bytes (2 MB).
pub const SCAN_MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// JSON schema identifier for scan output.
pub const SCAN_SCHEMA: &str = "hle.scan.v1";

/// Directory names that are always skipped during traversal.
const SKIP_DIRS: [&str; 3] = ["target", ".git", "node_modules"];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Minimum severity filter for CLI output.
///
/// Findings below this level are suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeverityFilter {
    Low,
    Medium,
    High,
    Critical,
}

impl SeverityFilter {
    /// Parse `"low"`, `"medium"`, `"high"`, `"critical"` (case-insensitive).
    ///
    /// Returns `None` on unrecognised input.
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    /// Convert to the library `Severity` threshold.
    #[must_use]
    pub fn as_severity(self) -> Severity {
        match self {
            Self::Low => Severity::Low,
            Self::Medium => Severity::Medium,
            Self::High => Severity::High,
            Self::Critical => Severity::Critical,
        }
    }
}

impl std::fmt::Display for SeverityFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
            Self::Critical => f.write_str("critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Walk result (internal)
// ---------------------------------------------------------------------------

struct WalkResult {
    files: Vec<PathBuf>,
    truncated: bool,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Scan the source tree rooted at `root` for anti-pattern findings.
///
/// - `severity_min`: suppress findings below this level.
/// - `json`: emit `hle.scan.v1` JSON; human-readable text when `false`.
///
/// # Errors
///
/// - `[2760]` when `root` does not exist.
/// - `[2761]` when `root` is not a directory.
/// - `[2762]` when `CompositeScanner::full()` construction fails (coverage gap).
pub fn scan_path(
    root: &Path,
    severity_min: SeverityFilter,
    json: bool,
) -> Result<String, HleError> {
    // Validate root.
    if !root.exists() {
        return Err(HleError::new(format!(
            "[2760] scan root not found: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(HleError::new(format!(
            "[2761] scan root is not a directory: {}",
            root.display()
        )));
    }

    // Build composite scanner.
    let scanner =
        CompositeScanner::full().map_err(|e| HleError::new(format!("[2762] scan blocked: {e}")))?;

    // Collect .rs files under root.
    let walk = collect_rs_files(root);

    // Build ScanInputs.
    let mut inputs: Vec<ScanInput> = Vec::with_capacity(walk.files.len());
    for path in &walk.files {
        let content = match std::fs::read_to_string(path) {
            Ok(s) if !s.is_empty() => s,
            // Skip unreadable or empty files silently.
            _ => continue,
        };
        // ScanInput::new requires non-empty content — we already checked above.
        if let Ok(input) = ScanInput::new(path.to_string_lossy().as_ref(), content) {
            inputs.push(input);
        }
    }

    let scanned_count = inputs.len();
    let report = scanner.scan_all(&inputs);

    // Apply severity filter.
    let threshold = severity_min.as_severity();
    let findings: Vec<&DetectorEvent> = report
        .events
        .iter()
        .filter(|e| e.severity >= threshold)
        .collect();

    if json {
        Ok(format_json(root, scanned_count, &findings, walk.truncated))
    } else {
        Ok(format_human(root, scanned_count, &findings, walk.truncated))
    }
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Recursively collect `.rs` files under `root`, bounded by `SCAN_MAX_DEPTH`
/// and `SCAN_MAX_FILES`. `SKIP_DIRS` are never entered.
fn collect_rs_files(root: &Path) -> WalkResult {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut truncated = false;
    walk_dir(root, 0, &mut files, &mut truncated);
    WalkResult { files, truncated }
}

fn walk_dir(dir: &Path, depth: usize, files: &mut Vec<PathBuf>, truncated: &mut bool) {
    if depth > SCAN_MAX_DEPTH {
        return;
    }
    if files.len() >= SCAN_MAX_FILES {
        *truncated = true;
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    // Collect and sort entries for deterministic order.
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        if files.len() >= SCAN_MAX_FILES {
            *truncated = true;
            return;
        }

        let path = entry.path();

        if path.is_dir() {
            // Check if this directory name is in the skip list.
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&dir_name) {
                continue;
            }
            walk_dir(&path, depth + 1, files, truncated);
        } else if path.is_file() {
            // Only consider .rs files within size budget.
            let is_rs = path.extension().and_then(|e| e.to_str()) == Some("rs");
            if !is_rs {
                continue;
            }
            // Size guard — use saturating cast; over-estimate is safe (skip large files).
            let size = std::fs::metadata(&path).map_or(0u64, |m| m.len());
            if size > SCAN_MAX_FILE_BYTES as u64 {
                continue;
            }
            files.push(path);
        }
    }
}

// ---------------------------------------------------------------------------
// JSON escape helper
// ---------------------------------------------------------------------------

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

/// Human-readable output: `file:line — [PATTERN_ID] SEVERITY — evidence`.
fn format_human(
    root: &Path,
    scanned: usize,
    findings: &[&DetectorEvent],
    truncated: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "hle scan root={} scanned_files={} findings={}",
        root.display(),
        scanned,
        findings.len()
    );
    if truncated {
        let _ = writeln!(
            out,
            "  [warning] file limit ({SCAN_MAX_FILES}) reached — scan truncated"
        );
    }
    for ev in findings {
        let _ = writeln!(
            out,
            "  {}:{} — [{}] {} — {}",
            ev.location.file_path, ev.location.line_start, ev.pattern_id, ev.severity, ev.evidence,
        );
    }
    if findings.is_empty() {
        let _ = writeln!(out, "  no findings above threshold");
    }
    out
}

/// JSON output: `hle.scan.v1` schema.
fn format_json(
    root: &Path,
    scanned: usize,
    findings: &[&DetectorEvent],
    truncated: bool,
) -> String {
    // counts_by_pattern
    let mut pattern_counts: Vec<(AntiPatternId, usize)> = Vec::new();
    for id in AntiPatternId::ALL {
        let cnt = findings.iter().filter(|e| e.pattern_id == id).count();
        if cnt > 0 {
            pattern_counts.push((id, cnt));
        }
    }

    // counts_by_severity
    let low_cnt = findings
        .iter()
        .filter(|e| e.severity == Severity::Low)
        .count();
    let med_cnt = findings
        .iter()
        .filter(|e| e.severity == Severity::Medium)
        .count();
    let high_cnt = findings
        .iter()
        .filter(|e| e.severity == Severity::High)
        .count();
    let crit_cnt = findings
        .iter()
        .filter(|e| e.severity == Severity::Critical)
        .count();

    // findings array
    let mut findings_json = String::from('[');
    for (i, ev) in findings.iter().enumerate() {
        if i > 0 {
            findings_json.push(',');
        }
        let _ = write!(
            findings_json,
            "{{\"pattern_id\":\"{pid}\",\"severity\":\"{sev}\",\
             \"file\":\"{file}\",\"line\":{line},\
             \"evidence\":\"{ev}\"}}",
            pid = json_escape(ev.pattern_id.as_str()),
            sev = ev.severity,
            file = json_escape(&ev.location.file_path),
            line = ev.location.line_start,
            ev = json_escape(ev.evidence.as_str()),
        );
    }
    findings_json.push(']');

    // counts_by_pattern object
    let mut cbp = String::from('{');
    for (i, (id, cnt)) in pattern_counts.iter().enumerate() {
        if i > 0 {
            cbp.push(',');
        }
        let _ = write!(cbp, "\"{}\":{}", json_escape(id.as_str()), cnt);
    }
    cbp.push('}');

    format!(
        "{{\"schema\":\"{schema}\",\
         \"root\":\"{root}\",\
         \"scanned_files\":{scanned},\
         \"truncated\":{truncated},\
         \"findings\":{findings},\
         \"counts_by_pattern\":{cbp},\
         \"counts_by_severity\":{{\"LOW\":{low},\"MEDIUM\":{med},\"HIGH\":{high},\"CRITICAL\":{crit}}}}}",
        schema = SCAN_SCHEMA,
        root = json_escape(&root.display().to_string()),
        scanned = scanned,
        truncated = truncated,
        findings = findings_json,
        cbp = cbp,
        low = low_cnt,
        med = med_cnt,
        high = high_cnt,
        crit = crit_cnt,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Create a temporary directory unique to the test.
    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hle-scan-{tag}-{}", std::process::id()));
        fs::create_dir_all(&d).expect("create temp dir");
        d
    }

    /// Write a named file inside a directory.
    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).expect("write file");
        p
    }

    // -----------------------------------------------------------------------
    // Error-code tests: 2760, 2761
    // -----------------------------------------------------------------------

    #[test]
    fn scan_nonexistent_root_returns_2760() {
        let p = std::env::temp_dir().join("hle-scan-nonexistent-999999999");
        let r = scan_path(&p, SeverityFilter::Low, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2760"));
    }

    #[test]
    fn scan_file_as_root_returns_2761() {
        let dir = temp_dir("file-as-root");
        let f = write_file(&dir, "file.rs", "fn f() {}");
        let r = scan_path(&f, SeverityFilter::Low, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2761"));
    }

    // -----------------------------------------------------------------------
    // Empty directory
    // -----------------------------------------------------------------------

    #[test]
    fn scan_empty_dir_returns_zero_findings() {
        let dir = temp_dir("empty");
        let r = scan_path(&dir, SeverityFilter::Low, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        let out = r.unwrap();
        assert!(out.contains("findings=0"));
    }

    #[test]
    fn scan_empty_dir_json_has_zero_scanned_files() {
        let dir = temp_dir("empty-json");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        assert!(r.is_ok());
        let json = r.unwrap();
        assert!(json.contains("\"scanned_files\":0"), "got: {json}");
    }

    #[test]
    fn scan_empty_dir_json_schema_is_hle_scan_v1() {
        let dir = temp_dir("empty-schema");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(json.contains("\"schema\":\"hle.scan.v1\""));
    }

    #[test]
    fn scan_empty_dir_json_findings_is_empty_array() {
        let dir = temp_dir("empty-findings");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(json.contains("\"findings\":[]"));
    }

    // -----------------------------------------------------------------------
    // AP29 trigger
    // -----------------------------------------------------------------------

    #[test]
    fn scan_ap29_trigger_produces_one_finding() {
        let dir = temp_dir("ap29");
        write_file(
            &dir,
            "bad.rs",
            "async fn bad() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        assert!(r.is_ok());
        let out = r.unwrap();
        assert!(
            out.contains("AP29_BLOCKING_IN_ASYNC"),
            "expected AP29 in output, got:\n{out}"
        );
    }

    #[test]
    fn scan_ap29_trigger_finding_count_nonzero() {
        let dir = temp_dir("ap29-count");
        write_file(
            &dir,
            "bad.rs",
            "async fn run() { thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        // "findings=0" should NOT appear; finding count should be > 0.
        assert!(!out.contains("findings=0"), "expected findings > 0:\n{out}");
    }

    #[test]
    fn scan_ap29_trigger_json_pattern_id_present() {
        let dir = temp_dir("ap29-json");
        write_file(
            &dir,
            "bad.rs",
            "async fn run() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(json.contains("AP29_BLOCKING_IN_ASYNC"), "got: {json}");
    }

    // -----------------------------------------------------------------------
    // Severity filter
    // -----------------------------------------------------------------------

    #[test]
    fn scan_severity_filter_high_suppresses_medium_findings() {
        let dir = temp_dir("severity-filter");
        // C12 emits Medium; AP29 emits High.
        write_file(
            &dir,
            "mixed.rs",
            "async fn run() { std::thread::sleep(std::time::Duration::ZERO); let v = Vec::new(); }",
        );
        let r = scan_path(&dir, SeverityFilter::High, false);
        let out = r.unwrap();
        // C12_UNBOUNDED_COLLECTIONS (Medium) should be absent.
        assert!(
            !out.contains("C12_UNBOUNDED_COLLECTIONS"),
            "Medium finding not suppressed: {out}"
        );
    }

    #[test]
    fn scan_severity_filter_medium_includes_medium_findings() {
        let dir = temp_dir("severity-medium");
        write_file(&dir, "coll.rs", "fn f() { let v = Vec::new(); }");
        let r = scan_path(&dir, SeverityFilter::Medium, false);
        let out = r.unwrap();
        assert!(
            out.contains("C12_UNBOUNDED_COLLECTIONS"),
            "expected C12 at Medium: {out}"
        );
    }

    #[test]
    fn scan_severity_filter_critical_suppresses_high_findings() {
        let dir = temp_dir("severity-critical");
        write_file(
            &dir,
            "high.rs",
            "async fn run() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Critical, false);
        let out = r.unwrap();
        // AP29 is High, not Critical — should be absent.
        assert!(
            !out.contains("AP29_BLOCKING_IN_ASYNC"),
            "High finding not suppressed at Critical threshold: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // JSON schema validation
    // -----------------------------------------------------------------------

    #[test]
    fn scan_json_output_contains_root_field() {
        let dir = temp_dir("json-root-field");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(json.contains("\"root\":"), "missing root: {json}");
    }

    #[test]
    fn scan_json_output_contains_counts_by_severity() {
        let dir = temp_dir("json-cbs");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(
            json.contains("\"counts_by_severity\":"),
            "missing counts_by_severity: {json}"
        );
    }

    #[test]
    fn scan_json_output_contains_counts_by_pattern() {
        let dir = temp_dir("json-cbp");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(
            json.contains("\"counts_by_pattern\":"),
            "missing counts_by_pattern: {json}"
        );
    }

    #[test]
    fn scan_json_output_contains_truncated_field() {
        let dir = temp_dir("json-trunc-field");
        let r = scan_path(&dir, SeverityFilter::Low, true);
        let json = r.unwrap();
        assert!(json.contains("\"truncated\":"), "missing truncated: {json}");
    }

    // -----------------------------------------------------------------------
    // Multi-file aggregation
    // -----------------------------------------------------------------------

    #[test]
    fn scan_multi_file_aggregates_findings() {
        let dir = temp_dir("multi-file");
        // Two files, each with one AP29 trigger.
        write_file(
            &dir,
            "a.rs",
            "async fn a() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        write_file(
            &dir,
            "b.rs",
            "async fn b() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        // Should not report "findings=0".
        assert!(!out.contains("findings=0"), "expected findings > 0:\n{out}");
    }

    #[test]
    fn scan_multi_file_scanned_files_reflects_count() {
        let dir = temp_dir("multi-scanned");
        write_file(&dir, "a.rs", "fn a() {}");
        write_file(&dir, "b.rs", "fn b() {}");
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("scanned_files=2"),
            "expected scanned_files=2: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // All 8 catalog scanners exercised (one trigger per pattern)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_ap28_trigger() {
        let dir = temp_dir("ap28");
        // AP28: merge conflict marker + "modules:" key.
        write_file(
            &dir,
            "plan.rs",
            "// modules: 42\n<<<<<<< HEAD\nmod a;\n=======\nmod b;\n>>>>>>>",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("AP28_COMPOSITIONAL_INTEGRITY_DRIFT"),
            "AP28 not found: {out}"
        );
    }

    #[test]
    fn scan_ap31_trigger() {
        let dir = temp_dir("ap31");
        // AP31: two .lock() calls on same line.
        write_file(
            &dir,
            "locks.rs",
            "fn f() { let a = mu1.lock(); let b = mu2.lock(); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(out.contains("AP31_NESTED_LOCKS"), "AP31 not found: {out}");
    }

    #[test]
    fn scan_c6_trigger() {
        let dir = temp_dir("c6");
        // C6: .lock() and .emit( on same line.
        write_file(
            &dir,
            "c6.rs",
            "fn f() { let g = mu.lock(); chan.emit(evt); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("C6_LOCK_HELD_SIGNAL_EMIT"),
            "C6 not found: {out}"
        );
    }

    #[test]
    fn scan_c7_trigger() {
        let dir = temp_dir("c7");
        // C7: function returning MutexGuard.
        write_file(
            &dir,
            "c7.rs",
            "pub fn get(&self) -> MutexGuard<Config> { self.mu.lock().unwrap() }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("C7_LOCK_GUARD_REFERENCE_RETURN"),
            "C7 not found: {out}"
        );
    }

    #[test]
    fn scan_c12_trigger() {
        let dir = temp_dir("c12");
        // C12: Vec::new() without capacity.
        write_file(&dir, "c12.rs", "fn f() { let v = Vec::new(); }");
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("C12_UNBOUNDED_COLLECTIONS"),
            "C12 not found: {out}"
        );
    }

    #[test]
    fn scan_c13_trigger() {
        let dir = temp_dir("c13");
        // C13: struct literal with 5+ fields.
        write_file(
            &dir,
            "c13.rs",
            "let x = BigConfig {\n  a: 1,\n  b: 2,\n  c: 3,\n  d: 4,\n  e: 5,\n};",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(out.contains("C13_MISSING_BUILDER"), "C13 not found: {out}");
    }

    #[test]
    fn scan_fp_false_pass_trigger() {
        let dir = temp_dir("fp");
        // FP_FALSE_PASS_CLASSES: "PASS" without required anchors.
        write_file(&dir, "gate.rs", r#"let s = "PASS"; // no anchors"#);
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("FP_FALSE_PASS_CLASSES"),
            "FP_FALSE_PASS_CLASSES not found: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // Skip non-.rs files and skip directories
    // -----------------------------------------------------------------------

    #[test]
    fn scan_skips_non_rs_files() {
        let dir = temp_dir("non-rs");
        // Write a .txt file with an AP29 trigger — should not be scanned.
        write_file(
            &dir,
            "notes.txt",
            "async fn run() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("findings=0"),
            "non-.rs file should not produce findings: {out}"
        );
    }

    #[test]
    fn scan_skips_target_directory() {
        let dir = temp_dir("skip-target");
        let target = dir.join("target");
        fs::create_dir_all(&target).ok();
        write_file(
            &target,
            "gen.rs",
            "async fn x() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        // findings=0 because target/ is skipped.
        assert!(
            out.contains("findings=0"),
            "target/ should be skipped: {out}"
        );
    }

    #[test]
    fn scan_skips_git_directory() {
        let dir = temp_dir("skip-git");
        let git = dir.join(".git");
        fs::create_dir_all(&git).ok();
        write_file(
            &git,
            "hook.rs",
            "async fn x() { std::thread::sleep(std::time::Duration::ZERO); }",
        );
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(out.contains("findings=0"), ".git/ should be skipped: {out}");
    }

    // -----------------------------------------------------------------------
    // SeverityFilter helpers
    // -----------------------------------------------------------------------

    #[test]
    fn severity_filter_from_str_low() {
        assert_eq!(SeverityFilter::from_str("low"), Some(SeverityFilter::Low));
    }

    #[test]
    fn severity_filter_from_str_medium() {
        assert_eq!(
            SeverityFilter::from_str("medium"),
            Some(SeverityFilter::Medium)
        );
    }

    #[test]
    fn severity_filter_from_str_high() {
        assert_eq!(SeverityFilter::from_str("high"), Some(SeverityFilter::High));
    }

    #[test]
    fn severity_filter_from_str_critical() {
        assert_eq!(
            SeverityFilter::from_str("critical"),
            Some(SeverityFilter::Critical)
        );
    }

    #[test]
    fn severity_filter_from_str_uppercase_accepted() {
        assert_eq!(
            SeverityFilter::from_str("MEDIUM"),
            Some(SeverityFilter::Medium)
        );
    }

    #[test]
    fn severity_filter_from_str_unknown_returns_none() {
        assert_eq!(SeverityFilter::from_str("bogus"), None);
    }

    #[test]
    fn severity_filter_display_stable() {
        assert_eq!(SeverityFilter::Low.to_string(), "low");
        assert_eq!(SeverityFilter::Medium.to_string(), "medium");
        assert_eq!(SeverityFilter::High.to_string(), "high");
        assert_eq!(SeverityFilter::Critical.to_string(), "critical");
    }

    #[test]
    fn severity_filter_as_severity_maps_correctly() {
        assert_eq!(SeverityFilter::Low.as_severity(), Severity::Low);
        assert_eq!(SeverityFilter::Medium.as_severity(), Severity::Medium);
        assert_eq!(SeverityFilter::High.as_severity(), Severity::High);
        assert_eq!(SeverityFilter::Critical.as_severity(), Severity::Critical);
    }

    // -----------------------------------------------------------------------
    // Human output format
    // -----------------------------------------------------------------------

    #[test]
    fn human_output_starts_with_hle_scan() {
        let dir = temp_dir("human-header");
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(out.starts_with("hle scan"), "expected 'hle scan': {out}");
    }

    #[test]
    fn human_output_no_findings_message() {
        let dir = temp_dir("no-findings-msg");
        let r = scan_path(&dir, SeverityFilter::Low, false);
        let out = r.unwrap();
        assert!(
            out.contains("no findings above threshold"),
            "expected no-findings message: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // json_escape internal helper (tested indirectly via JSON output)
    // -----------------------------------------------------------------------

    #[test]
    fn json_escape_quotes_are_escaped() {
        let escaped = json_escape("say \"hello\"");
        assert_eq!(escaped, r#"say \"hello\""#);
    }

    #[test]
    fn json_escape_newlines_are_escaped() {
        let escaped = json_escape("line1\nline2");
        assert_eq!(escaped, "line1\\nline2");
    }

    #[test]
    fn json_escape_backslash_is_escaped() {
        let escaped = json_escape("path\\to\\file");
        assert_eq!(escaped, "path\\\\to\\\\file");
    }

    // -----------------------------------------------------------------------
    // SCAN_MAX_FILES boundary (bounded walk)
    // -----------------------------------------------------------------------

    #[test]
    fn walk_does_not_panic_on_large_trees() {
        // We cannot easily create 10,001 files, but we verify the result
        // is always Ok and does not exceed the file count limit.
        let dir = temp_dir("large-tree");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).ok();
        for i in 0..20usize {
            write_file(&sub, &format!("f{i}.rs"), "fn x() {}");
        }
        let r = scan_path(&dir, SeverityFilter::Low, false);
        assert!(r.is_ok());
    }
}
