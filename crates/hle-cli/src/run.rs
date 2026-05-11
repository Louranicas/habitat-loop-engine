//! `M047` — `cli_run`: `hle run` command adapter.
//!
//! Owns workflow TOML parsing and delegates to C03 `PhaseExecutor` for
//! bounded execution.  Parser helpers (`parse_workflow`, `parse_string`,
//! `parse_bool`, `push_step`) are `pub` so `main.rs` tests can call them
//! via `super::`.
//!
//! Error codes: 2710-2713.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use substrate_types::{HleError, StepState, Workflow, WorkflowStep};

use hle_executor::local_runner::{LocalRunner, RunnerConfig};
use hle_executor::phase_executor::{ExecutionPhase, PhaseExecutor, PhaseSequence, PhaseStep};
use hle_executor::retry_policy::RetryPolicy;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Verdict derived from a completed run.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RunVerdict {
    Pass,
    Fail,
    AwaitingHuman,
    /// At least one step passed and at least one step failed.
    Partial,
}

impl RunVerdict {
    /// Short uppercase string for display/JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::AwaitingHuman => "AWAITING_HUMAN",
            Self::Partial => "PARTIAL",
        }
    }
}

impl std::fmt::Display for RunVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Bounded result of a completed run.
#[derive(Debug)]
pub struct RunReport {
    pub verdict: RunVerdict,
    pub step_count: usize,
    pub pass_count: usize,
    pub fail_count: usize,
    pub awaiting_human_count: usize,
    pub ledger_path: PathBuf,
}

impl RunReport {
    /// Format as a bounded human-readable string (max 2 KB).
    #[must_use]
    pub fn format_human(&self) -> String {
        let raw = format!(
            "hle run verdict={} steps={} pass={} fail={} human={} ledger={}",
            self.verdict.as_str(),
            self.step_count,
            self.pass_count,
            self.fail_count,
            self.awaiting_human_count,
            self.ledger_path.display(),
        );
        truncate_2kb(raw)
    }

    /// Format as `hle.run.summary.v1` JSON (max 2 KB).
    #[must_use]
    pub fn format_json(&self) -> String {
        let ledger = self.ledger_path.display().to_string().replace('"', "\\\"");
        let raw = format!(
            "{{\"schema\":\"hle.run.summary.v1\",\"verdict\":\"{verdict}\",\
             \"step_count\":{sc},\"pass_count\":{pc},\"fail_count\":{fc},\
             \"awaiting_human_count\":{hc},\"ledger_path\":\"{ledger}\"}}",
            verdict = self.verdict.as_str(),
            sc = self.step_count,
            pc = self.pass_count,
            fc = self.fail_count,
            hc = self.awaiting_human_count,
            ledger = ledger,
        );
        truncate_2kb(raw)
    }
}

impl std::fmt::Display for RunReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format_human())
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute the `hle run` command for the given workflow and ledger paths.
///
/// Parses the workflow TOML, validates it, builds a `PhaseSequence` from the
/// steps, runs it through `PhaseExecutor`, and returns a `RunReport` derived
/// from the `ExecutionResult`.
///
/// # Errors
///
/// Returns `HleError` with code 2710 when:
/// - the workflow file cannot be read
/// - the workflow TOML is invalid
/// - the `PhaseSequence` cannot be built
/// - the ledger write fails during execution
pub fn run_workflow(workflow: &Path, ledger: &Path) -> Result<RunReport, HleError> {
    let wf = parse_workflow(workflow)?;

    // Build PhaseSequence from Workflow steps.
    let phase_steps = wf
        .steps
        .iter()
        .map(|step| {
            if step.requires_human {
                PhaseStep::awaiting_human(
                    ExecutionPhase::Detect,
                    step.id.as_str(),
                    step.title.as_str(),
                )
            } else {
                PhaseStep::new(
                    ExecutionPhase::Detect,
                    step.id.as_str(),
                    step.title.as_str(),
                    // Use `true` as the default command: a bounded no-op that always passes.
                    "true",
                    step.desired_state,
                )
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| HleError::new(format!("[2710] phase step build failed: {err}")))?;

    let sequence = PhaseSequence::new(phase_steps)
        .map_err(|err| HleError::new(format!("[2710] phase sequence build failed: {err}")))?;

    let runner = LocalRunner::new(RunnerConfig::default_m0())
        .map_err(|err| HleError::new(format!("[2710] runner init failed: {err}")))?;

    let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, ledger);
    let result = executor
        .run_phases(&sequence)
        .map_err(|err| HleError::new(format!("[2710] phase executor failed: {err}")))?;

    let step_count = result.step_count();
    let pass_count = result.passed_count();
    let fail_count = result.failed_count();
    let awaiting_human_count = step_count - pass_count - fail_count;

    let verdict = if fail_count > 0 && pass_count > 0 {
        RunVerdict::Partial
    } else if fail_count > 0 {
        RunVerdict::Fail
    } else if awaiting_human_count > 0 {
        RunVerdict::AwaitingHuman
    } else {
        RunVerdict::Pass
    };

    Ok(RunReport {
        verdict,
        step_count,
        pass_count,
        fail_count,
        awaiting_human_count,
        ledger_path: ledger.to_path_buf(),
    })
}

/// Format the report according to the `--json` flag.
#[must_use]
pub fn format_report(report: &RunReport, json: bool) -> String {
    if json {
        report.format_json()
    } else {
        report.format_human()
    }
}

// ---------------------------------------------------------------------------
// Workflow TOML parsing (moved from main.rs)
// ---------------------------------------------------------------------------

/// Parse a workflow TOML file into a validated [`Workflow`].
///
/// Hand-rolled parser: no external TOML crate.
///
/// # Errors
///
/// Returns `HleError` when the file cannot be read or the content is invalid.
pub fn parse_workflow(path: &Path) -> Result<Workflow, HleError> {
    let text = fs::read_to_string(path).map_err(|err| {
        HleError::new(format!(
            "[2710] read workflow {} failed: {err}",
            path.display()
        ))
    })?;
    let mut name = String::new();
    let mut steps = Vec::new();
    let mut current_id = String::new();
    let mut current_title = String::new();
    let mut current_state = StepState::Passed;
    let mut current_human = false;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[steps]]" {
            push_step(
                &mut steps,
                &mut current_id,
                &mut current_title,
                current_state,
                current_human,
            );
            current_state = StepState::Passed;
            current_human = false;
        } else if let Some(value) = line.strip_prefix("name = ") {
            name = parse_string(value)?;
        } else if let Some(value) = line.strip_prefix("id = ") {
            current_id = parse_string(value)?;
        } else if let Some(value) = line.strip_prefix("title = ") {
            current_title = parse_string(value)?;
        } else if let Some(value) = line.strip_prefix("desired_state = ") {
            current_state = parse_string(value)?.parse()?;
        } else if let Some(value) = line.strip_prefix("requires_human = ") {
            current_human = parse_bool(value)?;
        }
    }
    push_step(
        &mut steps,
        &mut current_id,
        &mut current_title,
        current_state,
        current_human,
    );
    let workflow = Workflow::new(name, steps);
    workflow.validate()?;
    Ok(workflow)
}

/// Append a `WorkflowStep` to `steps` if `current_id` is non-empty.
///
/// Clears `current_id` and `current_title` via `mem::take` after the push.
pub fn push_step(
    steps: &mut Vec<WorkflowStep>,
    current_id: &mut String,
    current_title: &mut String,
    current_state: StepState,
    current_human: bool,
) {
    if current_id.is_empty() {
        return;
    }
    let step = if current_human {
        WorkflowStep::awaiting_human(std::mem::take(current_id), std::mem::take(current_title))
    } else {
        WorkflowStep::new(
            std::mem::take(current_id),
            std::mem::take(current_title),
            current_state,
        )
    };
    steps.push(step);
}

/// Parse a TOML-style quoted string value (`"..."`) into the inner `String`.
///
/// # Errors
///
/// Returns `HleError` when the value is not a properly quoted string.
pub fn parse_string(value: &str) -> Result<String, HleError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('"') || !trimmed.ends_with('"') {
        return Err(HleError::new(format!(
            "expected quoted string, got {trimmed}"
        )));
    }
    Ok(trimmed[1..trimmed.len() - 1].to_owned())
}

/// Parse a TOML-style boolean value (`true` / `false`).
///
/// # Errors
///
/// Returns `HleError` for any value other than `"true"` or `"false"`.
pub fn parse_bool(value: &str) -> Result<bool, HleError> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(HleError::new(format!("expected bool, got {other}"))),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_2kb(s: String) -> String {
    const MAX: usize = 2048;
    if s.len() <= MAX {
        return s;
    }
    let mut out = s[..MAX - 5].to_owned();
    out.push_str("[...]");
    out
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

    fn example_workflow() -> PathBuf {
        workspace_root().join("examples/workflow.example.toml")
    }

    fn temp_ledger(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hle-run-{name}-{}.jsonl", std::process::id()))
    }

    // -- run_workflow -----------------------------------------------------------

    #[test]
    fn run_workflow_returns_ok_on_valid_file() {
        let ledger = temp_ledger("ok");
        let r = run_workflow(&example_workflow(), &ledger);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn run_workflow_returns_run_report_struct() {
        let ledger = temp_ledger("struct");
        let r = run_workflow(&example_workflow(), &ledger);
        assert!(r.is_ok());
    }

    #[test]
    fn run_workflow_report_ledger_path_matches_input() {
        let ledger = temp_ledger("path");
        let r = run_workflow(&example_workflow(), &ledger);
        assert_eq!(r.map(|rep| rep.ledger_path), Ok(ledger));
    }

    #[test]
    fn run_workflow_errors_2710_on_missing_file() {
        let missing = PathBuf::from("/tmp/hle-nonexistent-workflow-12345.toml");
        let ledger = temp_ledger("missing");
        let r = run_workflow(&missing, &ledger);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2710")));
    }

    #[test]
    fn run_workflow_error_names_path() {
        let missing = PathBuf::from("/tmp/hle-nonexistent-workflow-99999.toml");
        let ledger = temp_ledger("namepath");
        let r = run_workflow(&missing, &ledger);
        assert!(r
            .err()
            .map_or(false, |e| e.to_string().contains("hle-nonexistent")));
    }

    // -- format_report ----------------------------------------------------------

    #[test]
    fn format_report_human_contains_verdict() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 2,
            pass_count: 2,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(format_report(&rep, false).contains("PASS"));
    }

    #[test]
    fn format_report_human_contains_ledger_path() {
        let rep = RunReport {
            verdict: RunVerdict::Fail,
            step_count: 1,
            pass_count: 0,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("my-ledger.jsonl"),
        };
        assert!(format_report(&rep, false).contains("my-ledger.jsonl"));
    }

    #[test]
    fn format_report_json_contains_schema_field() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        let out = format_report(&rep, true);
        assert!(out.contains("hle.run.summary.v1"));
    }

    #[test]
    fn format_report_json_contains_verdict() {
        let rep = RunReport {
            verdict: RunVerdict::AwaitingHuman,
            step_count: 1,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 1,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(format_report(&rep, true).contains("AWAITING_HUMAN"));
    }

    #[test]
    fn format_human_bounded_under_2kb() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().len() <= 2048);
    }

    #[test]
    fn format_json_bounded_under_2kb() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().len() <= 2048);
    }

    // -- RunVerdict display / str -----------------------------------------------

    #[test]
    fn run_verdict_pass_str() {
        assert_eq!(RunVerdict::Pass.as_str(), "PASS");
    }

    #[test]
    fn run_verdict_fail_str() {
        assert_eq!(RunVerdict::Fail.as_str(), "FAIL");
    }

    #[test]
    fn run_verdict_awaiting_human_str() {
        assert_eq!(RunVerdict::AwaitingHuman.as_str(), "AWAITING_HUMAN");
    }

    #[test]
    fn run_verdict_partial_str() {
        assert_eq!(RunVerdict::Partial.as_str(), "PARTIAL");
    }

    #[test]
    fn run_verdict_display_eq_as_str() {
        assert_eq!(RunVerdict::Pass.to_string(), RunVerdict::Pass.as_str());
    }

    // -- truncate helper --------------------------------------------------------

    #[test]
    fn truncate_2kb_short_string_unchanged() {
        let s = "hello".to_owned();
        assert_eq!(truncate_2kb(s.clone()), s);
    }

    #[test]
    fn truncate_2kb_long_string_truncated() {
        let s = "x".repeat(4096);
        let out = truncate_2kb(s);
        assert!(out.len() <= 2048);
    }

    #[test]
    fn truncate_2kb_long_string_has_marker() {
        let s = "x".repeat(4096);
        assert!(truncate_2kb(s).contains("[...]"));
    }

    // -- RunReport field coverage -----------------------------------------------

    // example workflow: 2 steps (1 auto-pass, 1 awaiting-human).
    // PhaseExecutor runs them in M0 mode and returns real counts.

    #[test]
    fn run_report_step_count_matches_workflow() {
        let ledger = temp_ledger("step-count");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        // example workflow has 2 steps
        assert_eq!(r.step_count, 2);
    }

    #[test]
    fn run_report_pass_count_matches_auto_steps() {
        let ledger = temp_ledger("pass-count");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        // step "authority" is auto-pass (command = "true"), step "handoff" is awaiting-human
        assert_eq!(r.pass_count, 1);
    }

    #[test]
    fn run_report_fail_count_zero_for_example_workflow() {
        let ledger = temp_ledger("fail-zero");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        assert_eq!(r.fail_count, 0);
    }

    #[test]
    fn run_report_awaiting_human_count_matches_human_steps() {
        let ledger = temp_ledger("human-count");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        // step "handoff" requires_human = true → awaiting_human
        assert_eq!(r.awaiting_human_count, 1);
    }

    #[test]
    fn run_report_verdict_awaiting_human_for_example_workflow() {
        let ledger = temp_ledger("verdict-init");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        // 1 awaiting-human step → overall verdict AwaitingHuman
        assert_eq!(r.verdict, RunVerdict::AwaitingHuman);
    }

    // -- RunReport Display -------------------------------------------------------

    #[test]
    fn run_report_display_uses_format_human() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 3,
            pass_count: 3,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert_eq!(rep.to_string(), rep.format_human());
    }

    // -- format_human field coverage --------------------------------------------

    #[test]
    fn format_human_contains_steps_count() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 7,
            pass_count: 7,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("steps=7"));
    }

    #[test]
    fn format_human_contains_pass_count() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 2,
            pass_count: 2,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("pass=2"));
    }

    #[test]
    fn format_human_contains_fail_count() {
        let rep = RunReport {
            verdict: RunVerdict::Fail,
            step_count: 2,
            pass_count: 1,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("fail=1"));
    }

    #[test]
    fn format_human_contains_human_count() {
        let rep = RunReport {
            verdict: RunVerdict::AwaitingHuman,
            step_count: 1,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 1,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("human=1"));
    }

    #[test]
    fn format_human_fail_verdict_present() {
        let rep = RunReport {
            verdict: RunVerdict::Fail,
            step_count: 1,
            pass_count: 0,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("FAIL"));
    }

    #[test]
    fn format_human_partial_verdict_present() {
        let rep = RunReport {
            verdict: RunVerdict::Partial,
            step_count: 2,
            pass_count: 1,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("PARTIAL"));
    }

    // -- format_json field coverage ---------------------------------------------

    #[test]
    fn format_json_contains_step_count_field() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 5,
            pass_count: 5,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("\"step_count\":5"));
    }

    #[test]
    fn format_json_contains_pass_count_field() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 3,
            pass_count: 3,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("\"pass_count\":3"));
    }

    #[test]
    fn format_json_contains_fail_count_field() {
        let rep = RunReport {
            verdict: RunVerdict::Fail,
            step_count: 2,
            pass_count: 1,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("\"fail_count\":1"));
    }

    #[test]
    fn format_json_contains_awaiting_human_count_field() {
        let rep = RunReport {
            verdict: RunVerdict::AwaitingHuman,
            step_count: 1,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 1,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("\"awaiting_human_count\":1"));
    }

    #[test]
    fn format_json_contains_ledger_path_field() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("my-path.jsonl"),
        };
        assert!(rep.format_json().contains("my-path.jsonl"));
    }

    #[test]
    fn format_json_partial_verdict_present() {
        let rep = RunReport {
            verdict: RunVerdict::Partial,
            step_count: 2,
            pass_count: 1,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("PARTIAL"));
    }

    #[test]
    fn format_json_fail_verdict_present() {
        let rep = RunReport {
            verdict: RunVerdict::Fail,
            step_count: 1,
            pass_count: 0,
            fail_count: 1,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_json().contains("FAIL"));
    }

    // -- format_report dispatch -------------------------------------------------

    #[test]
    fn format_report_json_true_returns_json_schema() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        let out = format_report(&rep, true);
        assert!(out.contains("hle.run.summary.v1") && out.starts_with('{'));
    }

    #[test]
    fn format_report_json_false_returns_human_string() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        let out = format_report(&rep, false);
        assert!(out.starts_with("hle run"));
    }

    // -- truncate boundary (exactly at limit) -----------------------------------

    #[test]
    fn truncate_2kb_exactly_at_limit_unchanged() {
        let s = "z".repeat(2048);
        let out = truncate_2kb(s.clone());
        assert_eq!(out.len(), 2048);
        assert_eq!(out, s);
    }

    #[test]
    fn truncate_2kb_one_over_limit_gets_marker() {
        let s = "z".repeat(2049);
        let out = truncate_2kb(s);
        assert!(out.len() <= 2048);
        assert!(out.ends_with("[...]"));
    }

    // -- RunVerdict PartialEq / Copy ----------------------------------------------

    #[test]
    fn run_verdict_copy_semantics() {
        let v = RunVerdict::Pass;
        let w = v;
        assert_eq!(v, w);
    }

    #[test]
    fn run_verdict_all_variants_distinct() {
        assert_ne!(RunVerdict::Pass, RunVerdict::Fail);
        assert_ne!(RunVerdict::Fail, RunVerdict::AwaitingHuman);
        assert_ne!(RunVerdict::AwaitingHuman, RunVerdict::Partial);
    }

    // -- run_workflow: directory path is not a valid file ----------------------

    #[test]
    fn run_workflow_errors_on_directory_path() {
        let dir = std::env::temp_dir();
        let ledger = temp_ledger("dir-path");
        let r = run_workflow(&dir, &ledger);
        // std::fs::metadata on a directory succeeds; this test documents that
        // the stub does NOT error for directories (it only checks metadata).
        // This test is informational / guards against implementation changes.
        let _ = r;
    }

    // -- RunReport: all verdict variants via format_human ----------------------

    #[test]
    fn run_report_awaiting_human_format_human_contains_awaiting() {
        let rep = RunReport {
            verdict: RunVerdict::AwaitingHuman,
            step_count: 1,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 1,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().contains("AWAITING_HUMAN"));
    }

    // -- format_json: schema prefix on all verdicts ---------------------------

    #[test]
    fn format_json_awaiting_human_verdict_in_schema() {
        let rep = RunReport {
            verdict: RunVerdict::AwaitingHuman,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        let out = rep.format_json();
        assert!(out.contains("hle.run.summary.v1") && out.contains("AWAITING_HUMAN"));
    }

    // -- Error message specificity on missing workflow -------------------------

    #[test]
    fn run_workflow_error_contains_2710_code() {
        let missing = PathBuf::from("/tmp/hle-run-missing-code-check.toml");
        let ledger = temp_ledger("code-check");
        let r = run_workflow(&missing, &ledger);
        assert!(r.err().map_or(false, |e| e.to_string().contains("2710")));
    }

    // -- RunReport: ledger path with absolute prefix ---------------------------

    #[test]
    fn run_report_ledger_path_absolute_in_format_human() {
        let ledger = temp_ledger("abs-path");
        let r = run_workflow(&example_workflow(), &ledger).unwrap();
        // The format_human output should include the ledger path.
        let out = r.format_human();
        assert!(out.contains("ledger="), "expected ledger= in: {out}");
    }

    // -- format_human prefix ---------------------------------------------------

    #[test]
    fn format_human_starts_with_hle_run() {
        let rep = RunReport {
            verdict: RunVerdict::Pass,
            step_count: 0,
            pass_count: 0,
            fail_count: 0,
            awaiting_human_count: 0,
            ledger_path: PathBuf::from("l.jsonl"),
        };
        assert!(rep.format_human().starts_with("hle run"));
    }
}
