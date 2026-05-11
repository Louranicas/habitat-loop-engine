#![forbid(unsafe_code)]
// New M046-M050 modules + their tests use ergonomic patterns; legacy main.rs above stays strict.
#![cfg_attr(
    test,
    allow(
        warnings,
        clippy::all,
        clippy::pedantic,
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::todo,
        clippy::dbg_macro
    )
)]
// End-to-end stack cross-reference: this source file is the terminal implementation node for M004_HLE_CLI.md / L06_CLI.md.
// Keep reciprocal alignment with CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> ai_docs/layers -> ai_docs/modules -> this source file while deploying the full codebase stack.

// C08 CLI Surface modules (M046-M051) — typed adapters; see ai_specs/modules/c08-cli-surface/.
//
// Legacy substrate-emit::execute_local_workflow remains the in-process bounded executor
// for any path that bypasses the planned-topology pipeline. New code routes through
// C03 phase_executor via run::run_workflow; execute_local_workflow stays available
// for substrate-emit consumers that have not yet migrated.
mod args;
mod audit;
mod daemon_once;
mod run;
mod scan;
mod status;
mod taxonomy;
mod verify;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use substrate_types::HleError;
use substrate_verify::verify_report;

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    match run(&raw_args) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("hle error: {err}");
            ExitCode::from(1)
        }
    }
}

/// Top-level dispatcher: parses argv through `args::parse` and delegates to
/// the appropriate M047-M050 handler.
fn run(argv: &[String]) -> Result<String, HleError> {
    use args::ParsedCommand;
    let cmd = args::parse(argv).map_err(HleError::from)?;
    match cmd {
        ParsedCommand::Help => Ok(help()),
        ParsedCommand::Version => Ok(String::from("hle m0-runtime 0.1.0")),
        ParsedCommand::Run(run_args) => {
            run_workflow_cmd(&run_args.workflow_path, &run_args.ledger_path)
        }
        ParsedCommand::Verify(verify_args) => {
            verify_ledger_cmd(&verify_args.ledger_path, verify_args.strict_sha)
        }
        ParsedCommand::DaemonOnce(daemon_args) => daemon_once::daemon_once(
            daemon_args.once,
            &daemon_args.workflow_path,
            &daemon_args.ledger_path,
            daemon_args.json,
        ),
        ParsedCommand::Status(status_args) => {
            let workspace_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            status::report_status(&workspace_root, status_args.json)
        }
        ParsedCommand::Scan(scan_args) => {
            scan::scan_path(&scan_args.root, scan_args.severity_min, scan_args.json)
        }
        ParsedCommand::Audit(audit_args) => {
            audit::audit_ledger(&audit_args.source, audit_args.strict, audit_args.json)
        }
        ParsedCommand::Taxonomy(taxonomy_args) => {
            taxonomy::taxonomy_report(&taxonomy_args.root, taxonomy_args.json)
        }
    }
}

fn help() -> String {
    String::from(
        "hle commands:\n  hle run --workflow <path> --ledger <path>\n  hle verify --ledger <path>\n  hle daemon --once --workflow <path> --ledger <path>\n  hle scan --root <path> [--severity low|medium|high|critical] [--json]\n  hle audit --source <path> [--strict] [--json]\n  hle taxonomy --root <path> [--json]\n\nThis codebase needs to be 'one shotted': every runtime command is bounded, foreground, and finite.\n",
    )
}

// ---------------------------------------------------------------------------
// Thin command wrappers — delegate to modules; preserve legacy output format
// for test compatibility.
// ---------------------------------------------------------------------------

/// Dispatch `hle run`: delegates to `run::run_workflow`, formats via the
/// legacy `"hle run verdict={} ledger={}"` template.
fn run_workflow_cmd(workflow: &Path, ledger: &Path) -> Result<String, HleError> {
    let report = run::run_workflow(workflow, ledger)?;
    Ok(format!(
        "hle run verdict={} ledger={}",
        report.verdict,
        ledger.display()
    ))
}

/// Dispatch `hle verify`: invokes `verify::verify_ledger_with_opts` which
/// runs the receipt-SHA-recompute check (M008) AND the state/verdict
/// internal-consistency check (substrate-verify). Returns the legacy
/// `"hle verify verdict={} receipts={} ledger={}"` format on success and
/// propagates `[E2722]` / `[E2723]` errors on SHA mismatch / missing.
fn verify_ledger_cmd(ledger: &Path, strict_sha: bool) -> Result<String, HleError> {
    use std::fs;
    // verify_ledger_with_opts already ran SHA recompute + verify_report. If it
    // returned Err (e.g., [E2722] receipt_sha_mismatch), propagate immediately.
    let _report = verify::verify_ledger_with_opts(ledger, strict_sha)?;
    // Re-parse for the legacy output format expected by existing tests.
    let text = fs::read_to_string(ledger)
        .map_err(|err| HleError::new(format!("read ledger {} failed: {err}", ledger.display())))?;
    let receipts = verify::parse_receipt_states(&text)?;
    let verdict = verify_report(&receipts)?;
    Ok(format!(
        "hle verify verdict={verdict} receipts={} ledger={}",
        receipts.len(),
        ledger.display()
    ))
}

// ---------------------------------------------------------------------------
// Private helpers re-exported for test compatibility.
// Tests in this file use `super::flag_path`, `super::parse_string`, etc.
// These delegate directly to the owning modules.
// ---------------------------------------------------------------------------

#[allow(dead_code)] // called only from tests (super::flag_path)
fn flag_path(args: &[String], name: &str) -> Result<PathBuf, HleError> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| PathBuf::from(&window[1]))
        .ok_or_else(|| HleError::new(format!("missing required flag {name}")))
}

// Delegate parse helpers to run module so tests can call `super::parse_string` etc.
#[allow(dead_code)] // called only from tests (super::parse_workflow)
fn parse_workflow(path: &Path) -> Result<substrate_types::Workflow, HleError> {
    run::parse_workflow(path)
}

#[allow(dead_code)] // called only from tests (super::push_step)
fn push_step(
    steps: &mut Vec<substrate_types::WorkflowStep>,
    current_id: &mut String,
    current_title: &mut String,
    current_state: substrate_types::StepState,
    current_human: bool,
) {
    run::push_step(
        steps,
        current_id,
        current_title,
        current_state,
        current_human,
    );
}

#[allow(dead_code)] // called only from tests (super::parse_string)
fn parse_string(value: &str) -> Result<String, HleError> {
    run::parse_string(value)
}

#[allow(dead_code)] // called only from tests (super::parse_bool)
fn parse_bool(value: &str) -> Result<bool, HleError> {
    run::parse_bool(value)
}

#[allow(dead_code)] // called only from tests (super::parse_receipt_states)
fn parse_receipt_states(text: &str) -> Result<Vec<substrate_types::Receipt>, HleError> {
    verify::parse_receipt_states(text)
}

#[allow(dead_code)] // called only from tests (super::json_field)
fn json_field(line: &str, field: &str) -> Result<String, HleError> {
    verify::json_field(line, field)
}

#[allow(dead_code)] // called only from tests (super::parse_json_string_tail)
fn parse_json_string_tail(value: &str, field: &str) -> Result<String, HleError> {
    verify::parse_json_string_tail(value, field)
}

// Legacy wrappers for tests that call `super::run_workflow`, `super::verify_ledger`,
// and `super::daemon_once` with raw `&[String]` slice signatures.

#[allow(dead_code)] // called only from tests (super::run_workflow)
fn run_workflow(args: &[String]) -> Result<String, HleError> {
    let workflow_path = flag_path(args, "--workflow")?;
    let ledger_path = flag_path(args, "--ledger")?;
    run_workflow_cmd(&workflow_path, &ledger_path)
}

#[allow(dead_code)] // called only from tests (super::verify_ledger)
fn verify_ledger(args: &[String]) -> Result<String, HleError> {
    let ledger_path = flag_path(args, "--ledger")?;
    verify_ledger_cmd(&ledger_path, false)
}

#[allow(dead_code)] // called only from tests (super::daemon_once)
fn daemon_once(args: &[String]) -> Result<String, HleError> {
    if !args.iter().any(|arg| arg == "--once") {
        return Err(HleError::new(
            "daemon command requires --once because this codebase needs to be 'one shotted' for bounded M0 operation",
        ));
    }
    let result = run_workflow(args)?;
    Ok(format!("hle daemon bounded-once complete; {result}"))
}

// ---------------------------------------------------------------------------
// Integration tests (end-to-end pipeline)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration_tests {
    use super::run;
    use std::fs;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn example_workflow() -> PathBuf {
        workspace_root().join("examples/workflow.example.toml")
    }

    fn temp_ledger(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hle-integration-{name}-{}.jsonl",
            std::process::id()
        ))
    }

    /// T01: full pipeline — workflow → phase_executor → ledger written → verify reads it.
    #[test]
    fn t01_run_writes_ledger_verify_reads_it() {
        let ledger = temp_ledger("t01");
        let _ = fs::remove_file(&ledger);

        let run_args = vec![
            String::from("run"),
            String::from("--workflow"),
            example_workflow().display().to_string(),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let run_result = run(&run_args);
        assert!(run_result.is_ok(), "run failed: {run_result:?}");
        assert!(run_result.map_or(false, |s| s.contains("verdict=")));

        let verify_args = vec![
            String::from("verify"),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let verify_result = run(&verify_args);
        assert!(verify_result.is_ok(), "verify failed: {verify_result:?}");

        let _ = fs::remove_file(&ledger);
    }

    /// T02: phase_executor writes JSONL receipts to the ledger.
    #[test]
    fn t02_phase_executor_produces_jsonl_ledger() {
        let ledger = temp_ledger("t02");
        let _ = fs::remove_file(&ledger);

        let run_args = vec![
            String::from("run"),
            String::from("--workflow"),
            example_workflow().display().to_string(),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let _ = run(&run_args);

        let content = fs::read_to_string(&ledger).unwrap_or_default();
        // PhaseExecutor writes hle.receipt.v1 schema lines.
        assert!(!content.is_empty(), "expected non-empty ledger after run");

        let _ = fs::remove_file(&ledger);
    }

    /// T03: verify on a fresh pass ledger reports receipts count.
    #[test]
    fn t03_verify_reports_receipt_count() {
        let ledger = temp_ledger("t03");
        let _ = fs::remove_file(&ledger);
        fs::write(
            &ledger,
            r#"{"workflow":"demo","step_id":"s1","state":"passed","verdict":"PASS","message":"ok"}
"#,
        )
        .expect("write ledger");

        let verify_args = vec![
            String::from("verify"),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let result = run(&verify_args);
        assert!(result.is_ok());
        assert!(result.map_or(false, |s| s.contains("receipts=1")));

        let _ = fs::remove_file(&ledger);
    }

    /// T04: daemon --once integrates the full pipeline end-to-end.
    #[test]
    fn t04_daemon_once_runs_full_pipeline() {
        let ledger = temp_ledger("t04");
        let _ = fs::remove_file(&ledger);

        let args = vec![
            String::from("daemon"),
            String::from("--once"),
            String::from("--workflow"),
            example_workflow().display().to_string(),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let result = run(&args);
        assert!(result.is_ok(), "daemon --once failed: {result:?}");
        assert!(result.map_or(false, |s| s.contains("bounded-once complete")));

        let _ = fs::remove_file(&ledger);
    }

    /// T05: verify on a tampered receipt (state/verdict inconsistent) reports FAIL.
    #[test]
    fn t05_verify_fails_on_inconsistent_receipt() {
        let ledger = temp_ledger("t05");
        let _ = fs::remove_file(&ledger);
        // state=passed but verdict=FAIL — substrate verifier catches this.
        fs::write(
            &ledger,
            r#"{"workflow":"demo","step_id":"s1","state":"passed","verdict":"FAIL","message":"tampered"}
"#,
        )
        .expect("write ledger");

        let verify_args = vec![
            String::from("verify"),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let result = run(&verify_args);
        // The substrate verifier returns "FAIL" for inconsistent receipts;
        // the output should contain verdict=FAIL.
        assert!(result.map_or(false, |s| s.contains("verdict=FAIL")));

        let _ = fs::remove_file(&ledger);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_receipt_states, parse_workflow, run};
    use std::fs;
    use std::path::PathBuf;
    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    #[test]
    fn version_command_reports_m0() {
        let args = vec![String::from("--version")];
        let result = run(&args);
        assert_eq!(result, Ok(String::from("hle m0-runtime 0.1.0")));
    }
    #[test]
    fn parses_example_workflow() {
        let workflow = parse_workflow(&workspace_root().join("examples/workflow.example.toml"));
        assert!(workflow.is_ok());
        assert_eq!(workflow.map_or(0, |value| value.steps.len()), 2);
    }
    #[test]
    fn daemon_requires_one_shot_once_flag() {
        let workflow = workspace_root().join("examples/workflow.example.toml");
        let ledger = std::env::temp_dir().join("hle-m0-daemon-reject-ledger.jsonl");
        let args = vec![
            String::from("daemon"),
            String::from("--workflow"),
            workflow.display().to_string(),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let result = run(&args);
        assert!(result.is_err());
        // args::parse rejects daemon without --once; error names the missing flag.
        assert!(result
            .err()
            .is_some_and(|err| err.to_string().contains("--once")));
    }
    #[test]
    fn run_and_verify_round_trip() {
        let ledger = std::env::temp_dir().join("hle-m0-test-ledger.jsonl");
        let workflow = workspace_root().join("examples/workflow.example.toml");
        let _ = fs::remove_file(&ledger);
        let run_args = vec![
            String::from("run"),
            String::from("--workflow"),
            workflow.display().to_string(),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let run_result = run(&run_args);
        assert!(run_result.is_ok());
        let verify_args = vec![
            String::from("verify"),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let verify_result = run(&verify_args);
        assert!(verify_result.is_ok());
        assert!(
            verify_result.map_or_else(|_| false, |value| value.contains("verdict=AWAITING_HUMAN"))
        );
        let _ = fs::remove_file(&ledger);
    }
    #[test]
    fn verify_accepts_escaped_json_receipt_messages() {
        let ledger = std::env::temp_dir().join("hle-m0-escaped-message-ledger.jsonl");
        let _ = fs::remove_file(&ledger);
        assert!(fs::write(
            &ledger,
            "{\"schema\":\"hle.receipt.v1\",\"created_unix\":1,\"workflow\":\"quoted-workflow\",\"step_id\":\"quoted-step\",\"state\":\"passed\",\"verdict\":\"PASS\",\"message\":\"verifier accepted \\\"quoted\\\" output\"}\n",
        )
        .is_ok());
        let verify_args = vec![
            String::from("verify"),
            String::from("--ledger"),
            ledger.display().to_string(),
        ];
        let verify_result = run(&verify_args);
        let receipt_text = fs::read_to_string(&ledger).unwrap_or_default();
        let receipts = parse_receipt_states(&receipt_text).unwrap_or_default();
        assert_eq!(receipts[0].message, "verifier accepted \"quoted\" output");
        assert_eq!(
            verify_result,
            Ok(format!(
                "hle verify verdict=PASS receipts=1 ledger={}",
                ledger.display()
            ))
        );
        let _ = fs::remove_file(&ledger);
    }
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hle-cli-{name}-{}.toml", std::process::id()))
    }
    fn write_temp_workflow(name: &str, body: &str) -> PathBuf {
        let path = temp_path(name);
        let _ = fs::remove_file(&path);
        assert!(fs::write(&path, body).is_ok());
        path
    }
    #[test]
    fn help_command_lists_run() {
        assert!(run(&[String::from("help")])
            .map_or(String::new(), |value| value)
            .contains("hle run"));
    }
    #[test]
    fn no_args_returns_help() {
        assert!(run(&[])
            .map_or(String::new(), |value| value)
            .contains("hle commands"));
    }
    #[test]
    fn unknown_command_errors() {
        assert!(run(&[String::from("bogus")]).is_err());
    }
    #[test]
    fn unknown_command_error_names_command() {
        assert!(run(&[String::from("bogus")])
            .map_or_else(|err| err.to_string(), |_| String::new())
            .contains("bogus"));
    }
    #[test]
    fn flag_path_finds_workflow_flag() {
        let args = vec![String::from("--workflow"), String::from("demo.toml")];
        assert_eq!(
            super::flag_path(&args, "--workflow"),
            Ok(PathBuf::from("demo.toml"))
        );
    }
    #[test]
    fn flag_path_finds_ledger_flag_after_other_flag() {
        let args = vec![
            String::from("--workflow"),
            String::from("demo.toml"),
            String::from("--ledger"),
            String::from("ledger.jsonl"),
        ];
        assert_eq!(
            super::flag_path(&args, "--ledger"),
            Ok(PathBuf::from("ledger.jsonl"))
        );
    }
    #[test]
    fn flag_path_errors_when_missing() {
        assert!(super::flag_path(&[], "--ledger").is_err());
    }
    #[test]
    fn parse_string_accepts_quoted_value() {
        assert_eq!(super::parse_string("\"demo\""), Ok(String::from("demo")));
    }
    #[test]
    fn parse_string_rejects_unquoted_value() {
        assert!(super::parse_string("demo").is_err());
    }
    #[test]
    fn parse_string_rejects_open_quote_only() {
        assert!(super::parse_string("\"demo").is_err());
    }
    #[test]
    fn parse_bool_accepts_true() {
        assert_eq!(super::parse_bool("true"), Ok(true));
    }
    #[test]
    fn parse_bool_accepts_false() {
        assert_eq!(super::parse_bool("false"), Ok(false));
    }
    #[test]
    fn parse_bool_rejects_yes() {
        assert!(super::parse_bool("yes").is_err());
    }
    #[test]
    fn parse_json_string_tail_plain() {
        assert_eq!(
            super::parse_json_string_tail("plain\"", "x"),
            Ok(String::from("plain"))
        );
    }
    #[test]
    fn parse_json_string_tail_quote_escape() {
        assert_eq!(
            super::parse_json_string_tail("a\\\"b\"", "x"),
            Ok(String::from("a\"b"))
        );
    }
    #[test]
    fn parse_json_string_tail_newline_escape() {
        assert_eq!(
            super::parse_json_string_tail("a\\nb\"", "x"),
            Ok(String::from("a\nb"))
        );
    }
    #[test]
    fn parse_json_string_tail_tab_escape() {
        assert_eq!(
            super::parse_json_string_tail("a\\tb\"", "x"),
            Ok(String::from("a\tb"))
        );
    }
    #[test]
    fn parse_json_string_tail_rejects_unterminated() {
        assert!(super::parse_json_string_tail("plain", "x").is_err());
    }
    #[test]
    fn json_field_reads_workflow() {
        let line = r#"{"workflow":"demo","step_id":"s1"}"#;
        assert_eq!(
            super::json_field(line, "workflow"),
            Ok(String::from("demo"))
        );
    }
    #[test]
    fn json_field_reads_step_id() {
        let line = r#"{"workflow":"demo","step_id":"s1"}"#;
        assert_eq!(super::json_field(line, "step_id"), Ok(String::from("s1")));
    }
    #[test]
    fn json_field_errors_on_missing_field() {
        assert!(super::json_field("{}", "workflow").is_err());
    }
    #[test]
    fn parse_receipts_empty_ledger_returns_empty_vec() {
        assert_eq!(parse_receipt_states(""), Ok(Vec::new()));
    }
    #[test]
    fn parse_receipts_reads_pass_state() {
        let text = r#"{"workflow":"demo","step_id":"s1","state":"passed","verdict":"PASS","message":"ok"}"#;
        assert_eq!(parse_receipt_states(text).map_or(0, |value| value.len()), 1);
    }
    #[test]
    fn parse_receipts_rejects_unknown_state() {
        let text =
            r#"{"workflow":"demo","step_id":"s1","state":"bogus","verdict":"PASS","message":"ok"}"#;
        assert!(parse_receipt_states(text).is_err());
    }
    #[test]
    fn parse_workflow_rejects_missing_file() {
        assert!(parse_workflow(&std::env::temp_dir().join("hle-missing-workflow.toml")).is_err());
    }
    #[test]
    fn parse_workflow_rejects_missing_name() {
        let path = write_temp_workflow(
            "missing-name",
            r#"[[steps]]
id = "s1"
title = "printf ok"
"#,
        );
        assert!(parse_workflow(&path).is_err());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_rejects_empty_steps() {
        let path = write_temp_workflow("empty-steps", "name = \"demo\"\n");
        assert!(parse_workflow(&path).is_err());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_reads_name() {
        let path = write_temp_workflow(
            "reads-name",
            r#"name = "demo"
[[steps]]
id = "s1"
title = "printf ok"
"#,
        );
        assert_eq!(
            parse_workflow(&path).map_or(String::new(), |value| value.name),
            "demo"
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_reads_step_id() {
        let path = write_temp_workflow(
            "reads-id",
            r#"name = "demo"
[[steps]]
id = "s1"
title = "printf ok"
"#,
        );
        assert_eq!(
            parse_workflow(&path).map_or(String::new(), |value| value.steps[0].id.clone()),
            "s1"
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_reads_step_title() {
        let path = write_temp_workflow(
            "reads-title",
            r#"name = "demo"
[[steps]]
id = "s1"
title = "printf ok"
"#,
        );
        assert_eq!(
            parse_workflow(&path).map_or(String::new(), |value| value.steps[0].title.clone()),
            "printf ok"
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_reads_desired_failed_state() {
        let path = write_temp_workflow(
            "reads-failed",
            r#"name = "demo"
[[steps]]
id = "s1"
title = "false"
desired_state = "failed"
"#,
        );
        assert_eq!(
            parse_workflow(&path)
                .map_or(substrate_types::StepState::Passed, |value| value.steps[0]
                    .desired_state),
            substrate_types::StepState::Failed
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_reads_requires_human() {
        let path = write_temp_workflow(
            "reads-human",
            r#"name = "demo"
[[steps]]
id = "s1"
title = "ask"
requires_human = true
"#,
        );
        assert!(parse_workflow(&path).is_ok_and(|value| value.steps[0].requires_human));
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parse_workflow_skips_comments_and_blank_lines() {
        let path = write_temp_workflow(
            "comments",
            r#"# comment
name = "demo"
[[steps]]
id = "s1"
title = "printf ok"
"#,
        );
        assert!(parse_workflow(&path).is_ok());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn daemon_once_accepts_once_flag_missing_other_flags_then_errors_for_workflow() {
        assert!(super::daemon_once(&[String::from("--once")])
            .map_or_else(|err| err.to_string(), |_| String::new())
            .contains("--workflow"));
    }
    #[test]
    fn verify_ledger_rejects_missing_flag() {
        assert!(super::verify_ledger(&[]).is_err());
    }
    #[test]
    fn run_workflow_rejects_missing_flag() {
        assert!(super::run_workflow(&[]).is_err());
    }
    #[test]
    fn verify_ledger_rejects_missing_file() {
        let args = vec![
            String::from("--ledger"),
            std::env::temp_dir()
                .join("hle-missing-ledger.jsonl")
                .display()
                .to_string(),
        ];
        assert!(super::verify_ledger(&args).is_err());
    }
    #[test]
    fn verify_command_rejects_missing_ledger_flag() {
        assert!(run(&[String::from("verify")]).is_err());
    }
    #[test]
    fn run_command_rejects_missing_workflow_flag() {
        assert!(run(&[String::from("run")]).is_err());
    }
    #[test]
    fn daemon_command_rejects_missing_once_flag() {
        assert!(run(&[String::from("daemon")]).is_err());
    }
    #[test]
    fn verify_ledger_reports_receipt_count() {
        let ledger = std::env::temp_dir().join("hle-cli-verify-count.jsonl");
        let _ = fs::remove_file(&ledger);
        assert!(fs::write(
            &ledger,
            r#"{"workflow":"demo","step_id":"s1","state":"passed","verdict":"PASS","message":"ok"}
"#
        )
        .is_ok());
        let args = vec![String::from("--ledger"), ledger.display().to_string()];
        assert!(super::verify_ledger(&args)
            .map_or(String::new(), |value| value)
            .contains("receipts=1"));
        let _ = fs::remove_file(ledger);
    }
    #[test]
    fn verify_ledger_reports_fail_verdict() {
        let ledger = std::env::temp_dir().join("hle-cli-verify-fail.jsonl");
        let _ = fs::remove_file(&ledger);
        assert!(fs::write(
            &ledger,
            r#"{"workflow":"demo","step_id":"s1","state":"failed","verdict":"FAIL","message":"bad"}
"#
        )
        .is_ok());
        let args = vec![String::from("--ledger"), ledger.display().to_string()];
        assert!(super::verify_ledger(&args)
            .map_or(String::new(), |value| value)
            .contains("verdict=FAIL"));
        let _ = fs::remove_file(ledger);
    }
    #[test]
    fn verify_ledger_fails_inconsistent_pass_state_fail_verdict() {
        let ledger = std::env::temp_dir().join("hle-cli-verify-tampered.jsonl");
        let _ = fs::remove_file(&ledger);
        assert!(fs::write(
            &ledger,
            r#"{"workflow":"demo","step_id":"s1","state":"passed","verdict":"FAIL","message":"tampered"}
"#
        )
        .is_ok());
        let args = vec![String::from("--ledger"), ledger.display().to_string()];
        assert!(super::verify_ledger(&args)
            .map_or(String::new(), |value| value)
            .contains("verdict=FAIL"));
        let _ = fs::remove_file(ledger);
    }
    #[test]
    fn verify_ledger_rejects_empty_receipts() {
        let ledger = std::env::temp_dir().join("hle-cli-empty-ledger.jsonl");
        let _ = fs::remove_file(&ledger);
        assert!(fs::write(&ledger, "").is_ok());
        let args = vec![String::from("--ledger"), ledger.display().to_string()];
        assert!(super::verify_ledger(&args).is_err());
        let _ = fs::remove_file(ledger);
    }
    #[test]
    fn run_version_alias_reports_m0() {
        assert_eq!(
            run(&[String::from("version")]),
            Ok(String::from("hle m0-runtime 0.1.0"))
        );
    }
    #[test]
    fn help_alias_with_double_dash_lists_daemon() {
        assert!(run(&[String::from("--help")])
            .map_or(String::new(), |value| value)
            .contains("daemon"));
    }
    #[test]
    fn push_step_ignores_empty_id() {
        let mut steps = Vec::new();
        let mut id = String::new();
        let mut title = String::from("title");
        super::push_step(
            &mut steps,
            &mut id,
            &mut title,
            substrate_types::StepState::Passed,
            false,
        );
        assert!(steps.is_empty());
    }
    #[test]
    fn push_step_adds_non_human_step() {
        let mut steps = Vec::new();
        let mut id = String::from("s1");
        let mut title = String::from("title");
        super::push_step(
            &mut steps,
            &mut id,
            &mut title,
            substrate_types::StepState::Passed,
            false,
        );
        assert_eq!(steps.len(), 1);
    }
    #[test]
    fn push_step_adds_human_step() {
        let mut steps = Vec::new();
        let mut id = String::from("s1");
        let mut title = String::from("title");
        super::push_step(
            &mut steps,
            &mut id,
            &mut title,
            substrate_types::StepState::Passed,
            true,
        );
        assert!(steps[0].requires_human);
    }
    #[test]
    fn push_step_takes_id_after_push() {
        let mut steps = Vec::new();
        let mut id = String::from("s1");
        let mut title = String::from("title");
        super::push_step(
            &mut steps,
            &mut id,
            &mut title,
            substrate_types::StepState::Passed,
            false,
        );
        assert!(id.is_empty());
    }
    #[test]
    fn push_step_takes_title_after_push() {
        let mut steps = Vec::new();
        let mut id = String::from("s1");
        let mut title = String::from("title");
        super::push_step(
            &mut steps,
            &mut id,
            &mut title,
            substrate_types::StepState::Passed,
            false,
        );
        assert!(title.is_empty());
    }
}
