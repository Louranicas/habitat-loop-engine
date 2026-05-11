//! `M052` — `cli_taxonomy`: test taxonomy verifier CLI adapter.
//!
//! Walks a source tree, extracts `#[test]` function names from each `.rs` file,
//! classifies them into `TestKind` heuristically, and runs the module collection
//! through `TaxonomyVerifier::verify_workspace`.
//!
//! Test-kind heuristics (applied to lower-case test name):
//! - Contains `_smoke_` or ends with `_smoke` → `Smoke`
//! - Contains `_doctest` or starts with `doctest_` → `Doctest`
//! - Contains `_prop_` or `prop_test` → `Property`
//! - Anything else → `Behavioral` (default)
//!
//! Cluster role is always `Verifier` (tests verify module behavior).
//!
//! Bounds (reuses scan.rs constants):
//! - Maximum directory depth: `SCAN_MAX_DEPTH` (10)
//! - Maximum files: `SCAN_MAX_FILES` (10,000)
//! - Maximum file size: `SCAN_MAX_FILE_BYTES` (2 MB)
//! - Skipped directories: `SKIP_DIRS` (target, .git, `node_modules`)
//!
//! Error codes: 2770-2772.
//!
//! Layer: L06 | Cluster: C08

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use hle_core::testing::test_taxonomy::{ClusterRole, TestDescriptor, TestKind};
use hle_verifier::test_taxonomy_verifier::{RejectionReason, TaxonomyReport, TaxonomyVerifier};
use substrate_types::HleError;

use crate::scan::{SCAN_MAX_DEPTH, SCAN_MAX_FILES, SCAN_MAX_FILE_BYTES};

// ---------------------------------------------------------------------------
// Public constants
// ---------------------------------------------------------------------------

/// JSON schema identifier for taxonomy output.
pub const TAXONOMY_SCHEMA: &str = "hle.taxonomy.v1";

/// Directories always skipped (mirrors `scan::SKIP_DIRS`).
const SKIP_DIRS: [&str; 3] = ["target", ".git", "node_modules"];

// ---------------------------------------------------------------------------
// Internal walk result
// ---------------------------------------------------------------------------

struct WalkResult {
    files: Vec<PathBuf>,
    truncated: bool,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute `hle taxonomy --root <path> [--json]`.
///
/// Walks the source tree at `root`, extracts `#[test]` functions per `.rs`
/// file, and verifies the aggregated collection with `TaxonomyVerifier`.
///
/// # Errors
///
/// - `[2770]` when `root` does not exist.
/// - `[2771]` when `root` is not a directory.
/// - `[2772]` when any per-module call to the verifier fails unexpectedly
///   (e.g. empty module path generated internally).
pub fn taxonomy_report(root: &Path, json: bool) -> Result<String, HleError> {
    // Validate root.
    if !root.exists() {
        return Err(HleError::new(format!(
            "[2770] taxonomy root not found: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(HleError::new(format!(
            "[2771] taxonomy root is not a directory: {}",
            root.display()
        )));
    }

    // Walk .rs files.
    let walk = collect_rs_files(root);

    // For each file: extract test descriptors, group by module path.
    let mut modules: Vec<(String, Vec<TestDescriptor>)> = Vec::new();

    for file_path in &walk.files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(s) if !s.is_empty() => s,
            _ => continue,
        };

        let descriptors = extract_descriptors(file_path, &content);
        if !descriptors.is_empty() {
            let module_path = file_path.display().to_string();
            modules.push((module_path, descriptors));
        }
    }

    // Build slice for verify_workspace.
    let module_refs: Vec<(&str, Vec<TestDescriptor>)> = modules
        .iter()
        .map(|(path, descs)| (path.as_str(), descs.clone()))
        .collect();

    let verifier = TaxonomyVerifier::with_default_policy();
    let reports = verifier
        .verify_workspace(&module_refs)
        .map_err(|e| HleError::new(format!("[2772] taxonomy verifier error: {e}")))?;

    let scanned_files = walk.files.len();
    let analyzed_modules = modules.len();

    if json {
        Ok(format_json(
            root,
            scanned_files,
            analyzed_modules,
            walk.truncated,
            &reports,
        ))
    } else {
        Ok(format_human(
            root,
            scanned_files,
            analyzed_modules,
            walk.truncated,
            &reports,
        ))
    }
}

// ---------------------------------------------------------------------------
// Test descriptor extraction
// ---------------------------------------------------------------------------

/// Extract `TestDescriptor` objects from the text of a `.rs` file.
///
/// Algorithm:
/// 1. Scan lines for `#[test]` or `#[tokio::test]`.
/// 2. The next non-blank line that matches `fn <name>(` is the test function.
/// 3. Classify by name heuristic → `TestKind`.
/// 4. Build a `TestDescriptor` with `ClusterRole::Verifier`.
fn extract_descriptors(file_path: &Path, content: &str) -> Vec<TestDescriptor> {
    let module_path = file_path.display().to_string();
    let lines: Vec<&str> = content.lines().collect();
    let mut descriptors = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Detect test attribute on its own line.
        let is_test_attr = trimmed == "#[test]"
            || trimmed == "#[tokio::test]"
            || trimmed.starts_with("#[test(")
            || trimmed.starts_with("#[tokio::test(");

        if is_test_attr {
            // Scan forward for `fn <name>(` — skip blank lines and other attributes.
            let mut j = i + 1;
            while j < lines.len() {
                let candidate = lines[j].trim();
                if candidate.is_empty() {
                    j += 1;
                    continue;
                }
                // Allow async fn too.
                let fn_candidate = if let Some(rest) = candidate.strip_prefix("async ") {
                    rest.trim()
                } else {
                    candidate
                };
                if let Some(rest) = fn_candidate.strip_prefix("fn ") {
                    // Extract name: up to `(` or `<`.
                    let name: String = rest
                        .chars()
                        .take_while(|&c| c != '(' && c != '<' && c != ' ')
                        .collect();
                    if !name.is_empty() {
                        let kind = classify_test_kind(&name);
                        // Build the descriptor — use the builder to stay type-safe.
                        // `ClusterRole::Verifier` is the appropriate role for all tests.
                        if let Ok(desc) = TestDescriptor::builder(&name, &module_path)
                            .kind(kind)
                            .cluster_role(ClusterRole::Verifier)
                            .build()
                        {
                            descriptors.push(desc);
                        }
                    }
                    // Whether we got a name or not, stop scanning for this attr.
                    i = j + 1;
                    break;
                }
                // Another attribute or non-fn line — keep scanning forward.
                j += 1;
            }
            // If no fn was found before end-of-file, just advance.
            if j >= lines.len() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    descriptors
}

/// Heuristic classification of a test function name.
///
/// Applied to lower-case name:
/// - `_smoke_` or ends with `_smoke` → `Smoke`
/// - `_doctest` or starts with `doctest_` → `Doctest`
/// - `_prop_` or starts with `prop_` or ends with `_prop` → `Property`
/// - default → `Behavioral`
fn classify_test_kind(name: &str) -> TestKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("_smoke_") || lower.ends_with("_smoke") || lower.starts_with("smoke_") {
        TestKind::Smoke
    } else if lower.contains("_doctest") || lower.starts_with("doctest_") {
        TestKind::Doctest
    } else if lower.contains("_prop_")
        || lower.starts_with("prop_")
        || lower.ends_with("_prop")
        || lower.contains("_proptest")
    {
        TestKind::Property
    } else {
        TestKind::Behavioral
    }
}

// ---------------------------------------------------------------------------
// Directory walker (mirrors scan.rs)
// ---------------------------------------------------------------------------

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

    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        if files.len() >= SCAN_MAX_FILES {
            *truncated = true;
            return;
        }

        let path = entry.path();

        if path.is_dir() {
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&dir_name) {
                continue;
            }
            walk_dir(&path, depth + 1, files, truncated);
        } else if path.is_file() {
            let is_rs = path.extension().and_then(|e| e.to_str()) == Some("rs");
            if !is_rs {
                continue;
            }
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

fn rejection_str(r: RejectionReason) -> &'static str {
    match r {
        RejectionReason::NoBehavioralTests => "NoBehavioralTests",
        RejectionReason::TooManySmoke => "TooManySmoke",
        RejectionReason::TooManyExcluded => "TooManyExcluded",
        RejectionReason::VacuousTestInflation => "VacuousTestInflation",
        RejectionReason::SmokeOnlyModule => "SmokeOnlyModule",
    }
}

fn format_human(
    root: &Path,
    scanned_files: usize,
    analyzed_modules: usize,
    truncated: bool,
    reports: &[TaxonomyReport],
) -> String {
    let passing = reports.iter().filter(|r| r.is_passing()).count();
    let failing = reports.len() - passing;
    let total_tests: usize = reports.iter().map(|r| r.profile.total).sum();

    let mut out = String::new();
    let _ = writeln!(
        out,
        "hle taxonomy root={} scanned_files={} analyzed_modules={} total_tests={} passing_modules={} failing_modules={}",
        root.display(),
        scanned_files,
        analyzed_modules,
        total_tests,
        passing,
        failing,
    );
    if truncated {
        let _ = writeln!(
            out,
            "  [warning] file limit ({SCAN_MAX_FILES}) reached — scan truncated"
        );
    }

    for report in reports {
        let status = if report.is_passing() { "PASS" } else { "FAIL" };
        let _ = writeln!(
            out,
            "  {status} {module} — total={total} behavioral={behavioral} smoke={smoke} property={prop} vacuous={vacuous}",
            module = report.module_path,
            total = report.profile.total,
            behavioral = report.profile.behavioral_count,
            smoke = report.profile.smoke_count,
            prop = report.profile.property_count,
            vacuous = report.profile.vacuous_count,
        );
        if let Some(reason) = &report.rejection {
            let _ = writeln!(out, "    rejection: {reason}");
        }
        // Print rejected test names.
        for entry in report.rejected_tests() {
            let _ = writeln!(
                out,
                "    rejected: {} [{}]",
                entry.descriptor.test_name, entry.rationale
            );
        }
    }

    if reports.is_empty() {
        let _ = writeln!(out, "  (no test modules found)");
    }

    out
}

fn format_json(
    root: &Path,
    scanned_files: usize,
    analyzed_modules: usize,
    truncated: bool,
    reports: &[TaxonomyReport],
) -> String {
    let passing = reports.iter().filter(|r| r.is_passing()).count();
    let failing = reports.len() - passing;
    let total_tests: usize = reports.iter().map(|r| r.profile.total).sum();

    // Build modules array.
    let mut modules_json = String::from('[');
    for (i, report) in reports.iter().enumerate() {
        if i > 0 {
            modules_json.push(',');
        }
        let rejection_str_val = report.rejection.map_or(String::from("null"), |r| {
            format!("\"{}\"", rejection_str(r))
        });

        // Build rejected_tests array.
        let mut rejected_arr = String::from('[');
        let rejected = report.rejected_tests();
        for (j, entry) in rejected.iter().enumerate() {
            if j > 0 {
                rejected_arr.push(',');
            }
            let _ = write!(
                rejected_arr,
                "{{\"name\":\"{name}\",\"rationale\":\"{rationale}\"}}",
                name = json_escape(&entry.descriptor.test_name),
                rationale = json_escape(&entry.rationale),
            );
        }
        rejected_arr.push(']');

        let _ = write!(
            modules_json,
            "{{\"module\":\"{module}\",\"verdict\":\"{verdict}\",\
             \"total\":{total},\"behavioral\":{behavioral},\
             \"smoke\":{smoke},\"property\":{prop},\
             \"doctest\":{doctest},\"vacuous\":{vacuous},\
             \"rejection\":{rejection},\
             \"rejected_tests\":{rejected}}}",
            module = json_escape(&report.module_path),
            verdict = if report.is_passing() { "PASS" } else { "FAIL" },
            total = report.profile.total,
            behavioral = report.profile.behavioral_count,
            smoke = report.profile.smoke_count,
            prop = report.profile.property_count,
            doctest = report.profile.doctest_count,
            vacuous = report.profile.vacuous_count,
            rejection = rejection_str_val,
            rejected = rejected_arr,
        );
    }
    modules_json.push(']');

    format!(
        "{{\"schema\":\"{schema}\",\
         \"root\":\"{root}\",\
         \"scanned_files\":{scanned},\
         \"analyzed_modules\":{analyzed},\
         \"total_tests\":{total},\
         \"truncated\":{truncated},\
         \"passing_modules\":{passing},\
         \"failing_modules\":{failing},\
         \"modules\":{modules}}}",
        schema = TAXONOMY_SCHEMA,
        root = json_escape(&root.display().to_string()),
        scanned = scanned_files,
        analyzed = analyzed_modules,
        total = total_tests,
        truncated = truncated,
        passing = passing,
        failing = failing,
        modules = modules_json,
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

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hle-taxonomy-{tag}-{}", std::process::id()));
        fs::create_dir_all(&d).expect("create temp dir");
        d
    }

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).expect("write file");
        p
    }

    // -----------------------------------------------------------------------
    // Error code tests: 2770, 2771
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_nonexistent_root_returns_2770() {
        let p = std::env::temp_dir().join("hle-taxonomy-nonexistent-999999999");
        let r = taxonomy_report(&p, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2770"));
    }

    #[test]
    fn taxonomy_file_as_root_returns_2771() {
        let dir = temp_dir("file-root");
        let f = write_file(&dir, "lib.rs", "fn foo() {}");
        let r = taxonomy_report(&f, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2771"));
    }

    // -----------------------------------------------------------------------
    // Empty directory
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_empty_dir_returns_ok() {
        let dir = temp_dir("empty");
        let r = taxonomy_report(&dir, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn taxonomy_empty_dir_zero_modules() {
        let dir = temp_dir("empty-modules");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules=0"), "got: {out}");
    }

    #[test]
    fn taxonomy_empty_dir_json_schema_correct() {
        let dir = temp_dir("empty-schema");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"schema\":\"hle.taxonomy.v1\""), "got: {out}");
    }

    #[test]
    fn taxonomy_empty_dir_json_modules_empty_array() {
        let dir = temp_dir("empty-arr");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"modules\":[]"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // File with no tests → excluded from modules list
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_file_with_no_tests_excluded() {
        let dir = temp_dir("notests");
        write_file(
            &dir,
            "lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules=0"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // extract_descriptors: basic test detection
    // -----------------------------------------------------------------------

    #[test]
    fn extract_descriptors_finds_one_test() {
        let path = PathBuf::from("fake/module.rs");
        let content = "\
#[test]
fn my_behavioral_test() {
    assert_eq!(1 + 1, 2);
}
";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1, "expected 1 descriptor, got {}", descs.len());
        assert_eq!(descs[0].test_name, "my_behavioral_test");
    }

    #[test]
    fn extract_descriptors_classifies_behavioral_default() {
        let path = PathBuf::from("mod.rs");
        let content = "#[test]\nfn verify_output() {}\n";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].kind, TestKind::Behavioral);
    }

    #[test]
    fn extract_descriptors_classifies_smoke_by_suffix() {
        let path = PathBuf::from("mod.rs");
        let content = "#[test]\nfn health_check_smoke() {}\n";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].kind, TestKind::Smoke);
    }

    #[test]
    fn extract_descriptors_classifies_smoke_by_infix() {
        let path = PathBuf::from("mod.rs");
        let content = "#[test]\nfn check_smoke_endpoint() {}\n";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].kind, TestKind::Smoke);
    }

    #[test]
    fn extract_descriptors_classifies_property_by_prefix() {
        let path = PathBuf::from("mod.rs");
        let content = "#[test]\nfn prop_addition_commutative() {}\n";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].kind, TestKind::Property);
    }

    #[test]
    fn extract_descriptors_classifies_doctest_infix() {
        let path = PathBuf::from("mod.rs");
        let content = "#[test]\nfn example_doctest_format() {}\n";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].kind, TestKind::Doctest);
    }

    #[test]
    fn extract_descriptors_finds_multiple_tests() {
        let path = PathBuf::from("mod.rs");
        let content = "\
#[test]
fn test_a() {}
#[test]
fn test_b() {}
#[test]
fn test_c() {}
";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 3);
    }

    #[test]
    fn extract_descriptors_handles_async_test() {
        let path = PathBuf::from("mod.rs");
        let content = "\
#[tokio::test]
async fn async_test_one() {}
";
        let descs = extract_descriptors(&path, content);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].test_name, "async_test_one");
    }

    #[test]
    fn extract_descriptors_no_false_positive_from_regular_fn() {
        let path = PathBuf::from("mod.rs");
        // Non-test function after test attribute is guarded by not having #[test]
        let content = "pub fn regular_fn() {}\n";
        let descs = extract_descriptors(&path, content);
        assert!(descs.is_empty());
    }

    // -----------------------------------------------------------------------
    // classify_test_kind
    // -----------------------------------------------------------------------

    #[test]
    fn classify_behavioral_default_name() {
        assert_eq!(
            classify_test_kind("verify_output_format"),
            TestKind::Behavioral
        );
    }

    #[test]
    fn classify_smoke_suffix() {
        assert_eq!(classify_test_kind("service_health_smoke"), TestKind::Smoke);
    }

    #[test]
    fn classify_smoke_prefix() {
        assert_eq!(classify_test_kind("smoke_startup_check"), TestKind::Smoke);
    }

    #[test]
    fn classify_smoke_infix() {
        assert_eq!(classify_test_kind("check_smoke_endpoint"), TestKind::Smoke);
    }

    #[test]
    fn classify_property_prefix() {
        assert_eq!(
            classify_test_kind("prop_length_preserved"),
            TestKind::Property
        );
    }

    #[test]
    fn classify_property_infix() {
        assert_eq!(
            classify_test_kind("add_prop_commutative"),
            TestKind::Property
        );
    }

    #[test]
    fn classify_property_proptest_infix() {
        assert_eq!(
            classify_test_kind("encode_decode_proptest"),
            TestKind::Property
        );
    }

    #[test]
    fn classify_doctest_infix() {
        assert_eq!(classify_test_kind("my_doctest_runs"), TestKind::Doctest);
    }

    #[test]
    fn classify_doctest_prefix() {
        assert_eq!(
            classify_test_kind("doctest_example_format"),
            TestKind::Doctest
        );
    }

    // -----------------------------------------------------------------------
    // Module with tests: taxonomy_report returns data
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_single_module_with_tests_scanned() {
        let dir = temp_dir("single-module");
        write_file(
            &dir,
            "lib.rs",
            "\
#[cfg(test)]
mod tests {
    #[test]
    fn verify_add_two_numbers() {
        assert_eq!(1 + 1, 2);
    }
    #[test]
    fn verify_subtract() {
        assert_eq!(3 - 1, 2);
    }
}
",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules=1"), "got: {out}");
    }

    #[test]
    fn taxonomy_single_module_total_tests_counted() {
        let dir = temp_dir("test-count");
        write_file(
            &dir,
            "mod.rs",
            "\
#[test]
fn first_behavioral_test() {}
#[test]
fn second_behavioral_test() {}
",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("total_tests=2"), "got: {out}");
    }

    #[test]
    fn taxonomy_module_with_behavioral_tests_passes() {
        let dir = temp_dir("passing-module");
        write_file(
            &dir,
            "lib.rs",
            "\
#[test]
fn verify_something_real() { assert!(true); }
",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("PASS"), "expected PASS in: {out}");
    }

    // -----------------------------------------------------------------------
    // JSON output fields
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_json_scanned_files_field() {
        let dir = temp_dir("json-scan");
        write_file(&dir, "lib.rs", "fn x() {}");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"scanned_files\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_analyzed_modules_field() {
        let dir = temp_dir("json-analyzed");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"analyzed_modules\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_total_tests_field() {
        let dir = temp_dir("json-total");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"total_tests\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_passing_modules_field() {
        let dir = temp_dir("json-pass");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"passing_modules\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_failing_modules_field() {
        let dir = temp_dir("json-fail-field");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"failing_modules\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_truncated_field() {
        let dir = temp_dir("json-trunc");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"truncated\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_root_field_contains_path() {
        let dir = temp_dir("json-root");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"root\":"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // Human output format: header fields
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_human_output_contains_root() {
        let dir = temp_dir("human-root");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("hle taxonomy root="), "got: {out}");
    }

    #[test]
    fn taxonomy_human_output_contains_scanned_files() {
        let dir = temp_dir("human-scanned");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("scanned_files="), "got: {out}");
    }

    #[test]
    fn taxonomy_human_output_contains_analyzed_modules() {
        let dir = temp_dir("human-analyzed");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules="), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // Skip directories
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_skip_target_dir() {
        let dir = temp_dir("skip-target");
        let target_dir = dir.join("target");
        fs::create_dir_all(&target_dir).unwrap();
        write_file(
            &target_dir,
            "generated.rs",
            "#[test]\nfn test_generated() {}\n",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        // target/ is skipped — no module from it should be analyzed.
        assert!(out.contains("analyzed_modules=0"), "got: {out}");
    }

    #[test]
    fn taxonomy_skip_git_dir() {
        let dir = temp_dir("skip-git");
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        write_file(&git_dir, "hook.rs", "#[test]\nfn hook_test() {}\n");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules=0"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // Vacuous test detection
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_vacuous_name_triggers_inflation() {
        let dir = temp_dir("vacuous");
        // Names containing `_assert_true` are flagged as vacuous by TaxonomyVerifier.
        write_file(
            &dir,
            "lib.rs",
            "#[test]\nfn always_passes_assert_true() {}\n",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        // Module-level policy: vacuous_count > 0 → VacuousTestInflation rejection.
        // Human output should show FAIL for that module.
        assert!(
            out.contains("FAIL"),
            "expected FAIL for vacuous module, got: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // Multiple files in subdirectory
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_walks_subdirectory() {
        let dir = temp_dir("subdir-walk");
        let sub = dir.join("src");
        fs::create_dir_all(&sub).unwrap();
        write_file(&sub, "a.rs", "#[test]\nfn test_a_behavior() {}\n");
        write_file(&sub, "b.rs", "#[test]\nfn test_b_behavior() {}\n");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("analyzed_modules=2"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // Non-.rs files are ignored
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_ignores_non_rs_files() {
        let dir = temp_dir("non-rs");
        write_file(&dir, "notes.md", "#[test]\nfn fake_test() {}\n");
        write_file(&dir, "config.toml", "[dependencies]\n");
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("scanned_files=0"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // JSON module entry fields
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_json_module_entry_has_verdict_field() {
        let dir = temp_dir("json-module-verdict");
        write_file(&dir, "lib.rs", "#[test]\nfn verify_works() {}\n");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"verdict\":"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_module_entry_has_total_field() {
        let dir = temp_dir("json-module-total");
        write_file(&dir, "lib.rs", "#[test]\nfn verify_works() {}\n");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"total\":1"), "got: {out}");
    }

    #[test]
    fn taxonomy_json_module_entry_has_rejection_null_for_passing() {
        let dir = temp_dir("json-rejection-null");
        write_file(&dir, "lib.rs", "#[test]\nfn verify_output() {}\n");
        let out = taxonomy_report(&dir, true).unwrap();
        assert!(out.contains("\"rejection\":null"), "got: {out}");
    }

    // -----------------------------------------------------------------------
    // json_escape helper
    // -----------------------------------------------------------------------

    #[test]
    fn json_escape_quote_escaped() {
        assert_eq!(json_escape("a\"b"), "a\\\"b");
    }

    #[test]
    fn json_escape_backslash_escaped() {
        assert_eq!(json_escape("a\\b"), "a\\\\b");
    }

    #[test]
    fn json_escape_newline_escaped() {
        assert_eq!(json_escape("a\nb"), "a\\nb");
    }

    #[test]
    fn json_escape_plain_unchanged() {
        assert_eq!(json_escape("hello world"), "hello world");
    }

    // -----------------------------------------------------------------------
    // taxonomy_report with async test
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_async_tokio_test_counted() {
        let dir = temp_dir("async-test");
        write_file(
            &dir,
            "lib.rs",
            "\
#[tokio::test]
async fn verify_async_behavior() {}
",
        );
        let out = taxonomy_report(&dir, false).unwrap();
        assert!(out.contains("total_tests=1"), "got: {out}");
    }
}
