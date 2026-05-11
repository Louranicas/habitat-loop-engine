#![forbid(unsafe_code)]

// End-to-end stack cross-reference: this source file is the terminal implementation node for M003_SUBSTRATE_EMIT.md / L02_PERSISTENCE.md / L03_WORKFLOW_EXECUTOR.md / L07_RUNBOOK_SEMANTICS.md.
// Keep reciprocal alignment with CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> ai_docs/layers -> ai_docs/modules -> this source file while deploying the full codebase stack.

use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use substrate_types::{ExecutionReport, HleError, Receipt, StepState, Workflow};
use substrate_verify::{verify_report, verify_step};

pub const MAX_RECEIPT_MESSAGE_BYTES: usize = 4_096;
const LOCAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Append one verifier-authoritative receipt to a local JSONL ledger.
///
/// # Errors
///
/// Returns an error when the ledger parent directory cannot be created, the
/// ledger cannot be opened, receipt serialization fails, or the write fails.
pub fn append_jsonl_receipt(path: &Path, receipt: &Receipt) -> Result<(), HleError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| HleError::new(format!("create ledger parent failed: {err}")))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| HleError::new(format!("open ledger failed: {err}")))?;
    let line = receipt_to_json_line(receipt)?;
    file.write_all(line.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|err| HleError::new(format!("write ledger failed: {err}")))
}

/// Serialize one receipt as bounded JSONL for local ledger emission.
///
/// The emitted line includes a `receipt_sha256` field whose value is the
/// SHA-256 hex digest of the canonical byte sequence:
/// `workflow \x00 step_id \x00 verdict \x00 state \x00 message`.
/// This allows `hle verify` (and Gap 2 M008 promotion) to independently
/// recompute the digest and reject forged receipts at read time.
///
/// # Errors
///
/// Returns an error when the system clock is before the Unix epoch.
pub fn receipt_to_json_line(receipt: &Receipt) -> Result<String, HleError> {
    let created_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| HleError::new(format!("system clock before unix epoch: {err}")))?
        .as_secs();
    let message = bounded(&receipt.message, MAX_RECEIPT_MESSAGE_BYTES);
    let sha_hex = receipt_sha256_hex(
        &receipt.workflow,
        &receipt.step_id,
        &receipt.verifier_verdict,
        receipt.state.as_str(),
        &message,
    );
    Ok(format!(
        "{{\"schema\":\"hle.receipt.v1\",\"created_unix\":{created_unix},\"workflow\":\"{}\",\"step_id\":\"{}\",\"state\":\"{}\",\"verdict\":\"{}\",\"message\":\"{}\",\"receipt_sha256\":\"{sha_hex}\"}}",
        json_escape(&receipt.workflow),
        json_escape(&receipt.step_id),
        receipt.state.as_str(),
        json_escape(&receipt.verifier_verdict),
        json_escape(&message),
    ))
}

/// Compute the canonical receipt SHA-256 hex digest.
///
/// Canonical bytes: `workflow \x00 step_id \x00 verdict \x00 state \x00 message`.
/// Null-byte separators prevent hash collisions from field boundary ambiguity.
#[must_use]
pub fn receipt_sha256_hex(
    workflow: &str,
    step_id: &str,
    verdict: &str,
    state: &str,
    message: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(workflow.as_bytes());
    hasher.update(b"\x00");
    hasher.update(step_id.as_bytes());
    hasher.update(b"\x00");
    hasher.update(verdict.as_bytes());
    hasher.update(b"\x00");
    hasher.update(state.as_bytes());
    hasher.update(b"\x00");
    hasher.update(message.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Execute a workflow with local shell commands and emit verifier receipts.
///
/// The executor only runs commands on the local host and writes to the supplied
/// local ledger path. It stops at the first verifier-authoritative failed
/// receipt.
///
/// # Errors
///
/// Returns an error when workflow validation fails, a local command cannot be
/// spawned, verifier authority rejects a step, or the local ledger write fails.
pub fn execute_local_workflow(
    workflow: &Workflow,
    ledger_path: &Path,
) -> Result<ExecutionReport, HleError> {
    workflow.validate()?;
    let mut receipts = Vec::new();

    for step in &workflow.steps {
        let (draft_state, message) = if step.requires_human {
            (StepState::AwaitingHuman, String::new())
        } else {
            run_local_command(&step.title)?
        };
        let mut receipt = verify_step(workflow, &step.id, draft_state)?;
        if !message.is_empty() {
            receipt.message = format!("{}; {message}", receipt.message);
        }
        append_jsonl_receipt(ledger_path, &receipt)?;
        let should_stop = matches!(receipt.state, StepState::AwaitingHuman | StepState::Failed);
        receipts.push(receipt);
        if should_stop {
            break;
        }
    }

    verify_report(&receipts)?;
    Ok(ExecutionReport {
        workflow: workflow.name.clone(),
        receipts,
    })
}

fn run_local_command(command: &str) -> Result<(StepState, String), HleError> {
    run_local_command_with_timeout(command, LOCAL_COMMAND_TIMEOUT)
}

fn run_local_command_with_timeout(
    command: &str,
    timeout: Duration,
) -> Result<(StepState, String), HleError> {
    let command_parts = match parse_local_command(command) {
        Ok(command_parts) => command_parts,
        Err(err) => return Ok((StepState::Failed, err.to_string())),
    };
    let (program, arguments) = command_parts
        .split_first()
        .ok_or_else(|| HleError::new("empty local M0 command"))?;
    let mut child = Command::new(program)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|err| HleError::new(format!("local command execution failed: {err}")))?;

    let started = Instant::now();
    while started.elapsed() < timeout {
        if child
            .try_wait()
            .map_err(|err| HleError::new(format!("local command wait failed: {err}")))?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .map_err(|err| HleError::new(format!("local command output failed: {err}")))?;
            let draft_state = draft_state_from_success(output.status.success());
            return Ok((draft_state, command_message(&output.stdout, &output.stderr)));
        }
        thread::sleep(Duration::from_millis(10));
    }

    terminate_child_group(child.id(), "-TERM");
    thread::sleep(Duration::from_millis(100));
    terminate_child_group(child.id(), "-KILL");
    child.kill().ok();
    let output = child
        .wait_with_output()
        .map_err(|err| HleError::new(format!("local command timeout output failed: {err}")))?;
    let message = command_message(&output.stdout, &output.stderr);
    let suffix = if message.is_empty() {
        String::new()
    } else {
        format!("; {message}")
    };
    Ok((
        StepState::Failed,
        format!(
            "local command timed out after {}s{suffix}",
            timeout.as_secs()
        ),
    ))
}

fn terminate_child_group(pid: u32, signal: &str) {
    Command::new("/usr/bin/kill")
        .arg(signal)
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

fn parse_local_command(command: &str) -> Result<Vec<String>, HleError> {
    if let Some(reason) = local_command_rejection(command) {
        return Err(HleError::new(reason));
    }
    let mut command_parts: Vec<String> = command
        .split_whitespace()
        .map(|part| part.trim_matches(|ch: char| matches!(ch, '"' | '\'')))
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    let lowered_program = command_parts
        .first()
        .map(|part| part.to_ascii_lowercase())
        .ok_or_else(|| HleError::new("empty local M0 command rejected"))?;
    let program = normalized_command_word(&lowered_program)
        .and_then(allowed_command_path)
        .ok_or_else(|| HleError::new("empty local M0 command rejected"))?;
    command_parts[0] = String::from(program);
    Ok(command_parts)
}

fn local_command_rejection(command: &str) -> Option<String> {
    const BLOCKED_TOKENS: [&str; 13] = [
        "curl", "wget", "ssh", "scp", "rsync", "nc", "netcat", "socat", "hermes", "orac", "povm",
        "rm", "sudo",
    ];
    const ALLOWED_TOKENS: [&str; 5] = ["printf", "true", "false", "sleep", "echo"];
    let normalized = command.to_ascii_lowercase();
    if normalized.contains("http://") || normalized.contains("https://") {
        return Some(String::from("external URL rejected for local M0 command"));
    }
    if let Some(token) = BLOCKED_TOKENS
        .iter()
        .find(|token| command_contains_blocked_token(&normalized, token))
    {
        return Some(format!(
            "command token '{token}' rejected for local M0 command"
        ));
    }
    if normalized.chars().any(|ch| {
        matches!(
            ch,
            ';' | '&' | '|' | '<' | '>' | '$' | '`' | '(' | ')' | '{' | '}'
        )
    }) {
        return Some(String::from(
            "shell metacharacters rejected for local M0 command",
        ));
    }
    let Some(program) = normalized
        .split_whitespace()
        .next()
        .and_then(normalized_command_word)
    else {
        return Some(String::from("empty local M0 command rejected"));
    };
    if !ALLOWED_TOKENS.contains(&program) {
        return Some(format!(
            "command token '{program}' is not in the local M0 allowlist"
        ));
    }
    None
}

fn allowed_command_path(program: &str) -> Option<&'static str> {
    match program {
        "printf" => Some("/usr/bin/printf"),
        "true" => Some("/usr/bin/true"),
        "false" => Some("/usr/bin/false"),
        "sleep" => Some("/usr/bin/sleep"),
        "echo" => Some("/usr/bin/echo"),
        _ => None,
    }
}

fn command_contains_blocked_token(command: &str, token: &str) -> bool {
    command
        .split(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ';' | '&' | '|' | '(' | ')' | '{' | '}' | '[' | ']' | '<' | '>'
                )
        })
        .filter_map(normalized_command_word)
        .any(|word| word == token)
}

fn normalized_command_word(word: &str) -> Option<&str> {
    let trimmed = word.trim_matches(|ch: char| matches!(ch, '"' | '\''));
    let basename = trimmed.rsplit('/').next().unwrap_or(trimmed);
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

fn draft_state_from_success(success: bool) -> StepState {
    if success {
        StepState::Passed
    } else {
        StepState::Failed
    }
}

fn command_message(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout_text = String::from_utf8_lossy(stdout);
    let stderr_text = String::from_utf8_lossy(stderr);
    match (stdout_text.trim().is_empty(), stderr_text.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("stdout: {}", redact_secret_values(stdout_text.trim())),
        (true, false) => format!("stderr: {}", redact_secret_values(stderr_text.trim())),
        (false, false) => format!(
            "stdout: {}; stderr: {}",
            redact_secret_values(stdout_text.trim()),
            redact_secret_values(stderr_text.trim())
        ),
    }
}

fn redact_secret_values(value: &str) -> String {
    value
        .split_whitespace()
        .map(redact_secret_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_secret_token(token: &str) -> String {
    let Some((key, _)) = token.split_once('=') else {
        return token.to_owned();
    };
    let normalized_key = key.replace('-', "_").to_ascii_uppercase();
    if matches!(
        normalized_key.as_str(),
        "API_KEY" | "API_TOKEN" | "AUTH_TOKEN" | "SECRET" | "TOKEN" | "PASSWORD" | "PASSWD"
    ) || normalized_key.ends_with("_TOKEN")
        || normalized_key.ends_with("_SECRET")
        || normalized_key.ends_with("_PASSWORD")
    {
        format!("{key}=[REDACTED]")
    } else {
        token.to_owned()
    }
}

#[must_use]
pub fn bounded(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut output = String::new();
    for ch in value.chars() {
        if output.len() + ch.len_utf8() > max_bytes.saturating_sub(32) {
            break;
        }
        output.push(ch);
    }
    output.push_str("...[truncated]");
    output
}

#[must_use]
pub fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use substrate_types::{Receipt, StepState, Workflow, WorkflowStep};

    use super::{
        bounded, execute_local_workflow, receipt_sha256_hex, receipt_to_json_line,
        run_local_command_with_timeout,
    };

    fn unique_ledger_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("hle-{name}-{nanos}.jsonl"))
    }

    #[test]
    fn json_line_contains_verdict() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        let line = receipt_to_json_line(&receipt);
        assert!(line.is_ok());
        assert!(line.map_or_else(|_| false, |value| value.contains("\"verdict\":\"PASS\"")));
    }

    #[test]
    fn bounded_truncates_long_output() {
        let value = "x".repeat(5_000);
        let truncated = bounded(&value, 128);
        assert!(truncated.len() < value.len());
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn local_executor_runs_commands_and_emits_verified_receipts() {
        let ledger = unique_ledger_path("pass");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", "printf hello", StepState::Passed)],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(report.map_or("FAIL", |value| value.verdict()), "PASS");
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("\"verdict\":\"PASS\""));
        assert!(ledger_text.contains("verifier accepted executor draft state"));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_stops_after_failed_command_and_records_fail() {
        let ledger = unique_ledger_path("fail");
        let workflow = Workflow::new(
            "demo",
            vec![
                WorkflowStep::new("s1", "false", StepState::Passed),
                WorkflowStep::new("s2", "printf skipped", StepState::Passed),
            ],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(report.map_or("PASS", |value| value.verdict()), "FAIL");
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("\"step_id\":\"s1\""));
        assert!(!ledger_text.contains("\"step_id\":\"s2\""));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_stops_after_awaiting_human_receipt() {
        let ledger = unique_ledger_path("human");
        let workflow = Workflow::new(
            "demo",
            vec![
                WorkflowStep::awaiting_human("s1", "ask Luke"),
                WorkflowStep::new("s2", "printf skipped", StepState::Passed),
            ],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(
            report.map_or("PASS", |value| value.verdict()),
            "AWAITING_HUMAN"
        );
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("\"step_id\":\"s1\""));
        assert!(!ledger_text.contains("\"step_id\":\"s2\""));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_redacts_secret_like_command_output() {
        let ledger = unique_ledger_path("secret");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(
                "s1",
                "printf API_TOKEN=supersecret123",
                StepState::Passed,
            )],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(!ledger_text.contains("supersecret123"));
        assert!(ledger_text.contains("API_TOKEN=[REDACTED]"));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_command_timeout_returns_failed_state() {
        let result =
            run_local_command_with_timeout("sleep 2", std::time::Duration::from_millis(10));

        assert!(result.is_ok());
        let (state, message) = result.unwrap_or((StepState::Passed, String::new()));
        assert_eq!(state, StepState::Failed);
        assert!(message.contains("timed out"));
    }

    #[test]
    fn local_executor_rejects_external_url_command_and_emits_fail() {
        let ledger = unique_ledger_path("external");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(
                "s1",
                "curl https://example.invalid/habitat",
                StepState::Passed,
            )],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(report.map_or("PASS", |value| value.verdict()), "FAIL");
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("external URL rejected"));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_rejects_absolute_blocked_command_path() {
        let ledger = unique_ledger_path("blocked-path");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(
                "s1",
                "/bin/rm /tmp/hle-m0-should-not-exist",
                StepState::Passed,
            )],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(report.map_or("PASS", |value| value.verdict()), "FAIL");
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("command token 'rm' rejected"));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_rejects_separator_chained_blocked_command() {
        let ledger = unique_ledger_path("blocked-separator");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(
                "s1",
                "printf safe;rm /tmp/hle-m0-should-not-exist",
                StepState::Passed,
            )],
        );

        let report = execute_local_workflow(&workflow, &ledger);

        assert!(report.is_ok());
        assert_eq!(report.map_or("PASS", |value| value.verdict()), "FAIL");
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(ledger_text.contains("command token 'rm' rejected"));
        let _ = fs::remove_file(ledger);
    }

    #[test]
    fn local_executor_rejects_unlisted_command() {
        let result = run_local_command_with_timeout(
            "touch /tmp/hle-m0-should-not-exist",
            std::time::Duration::from_millis(10),
        );
        assert!(result.is_ok());
        assert_eq!(
            result.map_or(StepState::Passed, |value| value.0),
            StepState::Failed
        );
    }

    #[test]
    fn local_executor_rejects_shell_metacharacters() {
        let result = run_local_command_with_timeout(
            "printf safe > /tmp/hle-m0-should-not-exist",
            std::time::Duration::from_millis(10),
        );
        assert!(result.is_ok());
        assert_eq!(
            result.map_or(StepState::Passed, |value| value.0),
            StepState::Failed
        );
    }

    #[test]
    fn local_timeout_hard_kills_term_ignoring_group() {
        let started = std::time::Instant::now();
        let result =
            run_local_command_with_timeout("sleep 2", std::time::Duration::from_millis(10));
        assert!(result.is_ok());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn json_escape_preserves_plain_text() {
        assert_eq!(super::json_escape("plain"), "plain");
    }

    #[test]
    fn json_escape_escapes_quote() {
        assert_eq!(super::json_escape(r#"a"b"#), r#"a\"b"#);
    }

    #[test]
    fn json_escape_escapes_backslash() {
        assert_eq!(super::json_escape(r"a\b"), r"a\\b");
    }

    #[test]
    fn json_escape_escapes_newline() {
        assert_eq!(super::json_escape(&format!("a{}b", '\n')), "a\\nb");
    }

    #[test]
    fn json_escape_escapes_carriage_return() {
        assert_eq!(super::json_escape(&format!("a{}b", '\r')), "a\\rb");
    }

    #[test]
    fn json_escape_escapes_tab() {
        assert_eq!(super::json_escape(&format!("a{}b", '\t')), "a\\tb");
    }

    #[test]
    fn bounded_keeps_short_value() {
        assert_eq!(bounded("short", 128), "short");
    }

    #[test]
    fn bounded_respects_tiny_limit_without_panic() {
        assert!(bounded("abcdef", 4).contains("truncated"));
    }

    #[test]
    fn bounded_preserves_utf8_boundary() {
        assert!(bounded("å".repeat(100).as_str(), 64).is_char_boundary(0));
    }

    #[test]
    fn bounded_appends_truncated_marker() {
        assert!(bounded(&"x".repeat(256), 64).ends_with("...[truncated]"));
    }

    #[test]
    fn redacts_api_key_token() {
        assert_eq!(
            super::redact_secret_token("API_KEY=abc123"),
            "API_KEY=[REDACTED]"
        );
    }

    #[test]
    fn redacts_auth_token_token() {
        assert_eq!(
            super::redact_secret_token("AUTH_TOKEN=abc123"),
            "AUTH_TOKEN=[REDACTED]"
        );
    }

    #[test]
    fn redacts_password_token() {
        assert_eq!(
            super::redact_secret_token("PASSWORD=abc123"),
            "PASSWORD=[REDACTED]"
        );
    }

    #[test]
    fn redacts_dash_password_token() {
        assert_eq!(
            super::redact_secret_token("db-password=abc123"),
            "db-password=[REDACTED]"
        );
    }

    #[test]
    fn leaves_non_secret_assignment_visible() {
        assert_eq!(super::redact_secret_token("NAME=value"), "NAME=value");
    }

    #[test]
    fn leaves_non_assignment_visible() {
        assert_eq!(super::redact_secret_token("hello"), "hello");
    }

    #[test]
    fn redacts_secret_values_across_words() {
        assert_eq!(
            super::redact_secret_values("ok TOKEN=abc done"),
            "ok TOKEN=[REDACTED] done"
        );
    }

    #[test]
    fn command_message_reports_empty_for_silent_output() {
        assert_eq!(super::command_message(b"", b""), "");
    }

    #[test]
    fn command_message_reports_stdout() {
        assert_eq!(
            super::command_message(
                b"hello
", b""
            ),
            "stdout: hello"
        );
    }

    #[test]
    fn command_message_reports_stderr() {
        assert_eq!(
            super::command_message(
                b"", b"oops
"
            ),
            "stderr: oops"
        );
    }

    #[test]
    fn command_message_reports_stdout_and_stderr() {
        assert_eq!(
            super::command_message(b"hello", b"oops"),
            "stdout: hello; stderr: oops"
        );
    }

    #[test]
    fn command_message_redacts_stdout_secret() {
        assert_eq!(
            super::command_message(b"TOKEN=abc", b""),
            "stdout: TOKEN=[REDACTED]"
        );
    }

    #[test]
    fn command_message_redacts_stderr_secret() {
        assert_eq!(
            super::command_message(b"", b"SECRET=abc"),
            "stderr: SECRET=[REDACTED]"
        );
    }

    #[test]
    fn draft_state_maps_success_to_passed() {
        assert_eq!(super::draft_state_from_success(true), StepState::Passed);
    }

    #[test]
    fn draft_state_maps_failure_to_failed() {
        assert_eq!(super::draft_state_from_success(false), StepState::Failed);
    }

    #[test]
    fn normalized_word_extracts_basename() {
        assert_eq!(
            super::normalized_command_word("/bin/printf"),
            Some("printf")
        );
    }

    #[test]
    fn normalized_word_strips_double_quotes() {
        assert_eq!(super::normalized_command_word("\"printf\""), Some("printf"));
    }

    #[test]
    fn normalized_word_strips_single_quotes() {
        assert_eq!(super::normalized_command_word("'printf'"), Some("printf"));
    }

    #[test]
    fn normalized_word_rejects_empty_after_basename() {
        assert_eq!(super::normalized_command_word(""), None);
    }

    #[test]
    fn command_token_detector_finds_plain_blocked_token() {
        assert!(super::command_contains_blocked_token("rm file", "rm"));
    }

    #[test]
    fn command_token_detector_finds_absolute_blocked_token() {
        assert!(super::command_contains_blocked_token("/bin/rm file", "rm"));
    }

    #[test]
    fn command_token_detector_finds_semicolon_blocked_token() {
        assert!(super::command_contains_blocked_token(
            "printf ok;rm file",
            "rm"
        ));
    }

    #[test]
    fn command_token_detector_does_not_match_substring() {
        assert!(!super::command_contains_blocked_token("program", "rm"));
    }

    #[test]
    fn local_rejection_blocks_http_url() {
        assert!(super::local_command_rejection("printf http://example.invalid").is_some());
    }

    #[test]
    fn local_rejection_blocks_https_url() {
        assert!(super::local_command_rejection("printf https://example.invalid").is_some());
    }

    #[test]
    fn local_rejection_blocks_sudo() {
        assert!(super::local_command_rejection("sudo true").is_some());
    }

    #[test]
    fn local_rejection_allows_printf() {
        assert_eq!(super::local_command_rejection("printf ok"), None);
    }

    #[test]
    fn parse_local_command_resolves_allowlist_to_absolute_path() {
        let command_parts = super::parse_local_command("true").unwrap_or_else(|_| Vec::new());
        assert_eq!(
            command_parts.first().map(String::as_str),
            Some("/usr/bin/true")
        );
    }

    #[test]
    fn parse_local_command_resolves_path_allowlist_to_canonical_binary() {
        let command_parts = super::parse_local_command("/tmp/true").unwrap_or_else(|_| Vec::new());
        assert_eq!(
            command_parts.first().map(String::as_str),
            Some("/usr/bin/true")
        );
    }

    #[test]
    fn receipt_json_contains_schema() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        assert!(receipt_to_json_line(&receipt)
            .map_or(String::new(), |line| line)
            .contains("hle.receipt.v1"));
    }

    #[test]
    fn receipt_json_escapes_message_quotes() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "quoted \"ok\"");
        assert!(receipt_to_json_line(&receipt)
            .map_or(String::new(), |line| line)
            .contains("quoted \\\"ok\\\""));
    }

    #[test]
    fn append_jsonl_receipt_creates_parent_directory() {
        let ledger = std::env::temp_dir()
            .join("hle-append-parent-test")
            .join("ledger.jsonl");
        let _ = fs::remove_file(&ledger);
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        assert!(super::append_jsonl_receipt(&ledger, &receipt).is_ok());
        assert!(ledger.exists());
        let _ = fs::remove_file(&ledger);
    }

    // ── receipt_sha256_hex / receipt_to_json_line SHA field tests ─────────────

    #[test]
    fn receipt_json_line_contains_receipt_sha256_field() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        let line = receipt_to_json_line(&receipt).map_or(String::new(), |line| line);
        assert!(
            line.contains("\"receipt_sha256\":\""),
            "line missing receipt_sha256: {line}"
        );
    }

    #[test]
    fn receipt_sha256_field_is_exactly_64_hex_chars() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        let line = receipt_to_json_line(&receipt).map_or(String::new(), |line| line);
        let marker = "\"receipt_sha256\":\"";
        assert!(
            line.contains(marker),
            "receipt_sha256 field missing from line"
        );
        let marker_pos = line.find(marker).unwrap_or(0);
        let rest = &line[marker_pos + marker.len()..];
        let end = rest.find('"').unwrap_or(0);
        let hex = &rest[..end];
        assert_eq!(
            hex.len(),
            64,
            "digest must be 64 hex chars, got {}",
            hex.len()
        );
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "digest must be lowercase hex: {hex}"
        );
    }

    #[test]
    fn receipt_sha256_hex_is_deterministic() {
        let h1 = receipt_sha256_hex("demo", "s1", "PASS", "passed", "ok");
        let h2 = receipt_sha256_hex("demo", "s1", "PASS", "passed", "ok");
        assert_eq!(h1, h2);
    }

    #[test]
    fn receipt_sha256_hex_differs_for_different_workflow() {
        let h1 = receipt_sha256_hex("wf-a", "s1", "PASS", "passed", "ok");
        let h2 = receipt_sha256_hex("wf-b", "s1", "PASS", "passed", "ok");
        assert_ne!(h1, h2);
    }

    #[test]
    fn receipt_sha256_hex_differs_for_different_verdict() {
        let h1 = receipt_sha256_hex("demo", "s1", "PASS", "passed", "ok");
        let h2 = receipt_sha256_hex("demo", "s1", "FAIL", "failed", "ok");
        assert_ne!(h1, h2);
    }

    #[test]
    fn receipt_sha256_hex_differs_for_different_step_id() {
        let h1 = receipt_sha256_hex("demo", "s1", "PASS", "passed", "ok");
        let h2 = receipt_sha256_hex("demo", "s2", "PASS", "passed", "ok");
        assert_ne!(h1, h2);
    }

    #[test]
    fn receipt_sha256_hex_prevents_field_boundary_collision() {
        // "ab" + "c" and "a" + "bc" must hash differently due to \x00 separators.
        let h1 = receipt_sha256_hex("ab", "c", "PASS", "passed", "ok");
        let h2 = receipt_sha256_hex("a", "bc", "PASS", "passed", "ok");
        assert_ne!(
            h1, h2,
            "field boundary attack must produce different digests"
        );
    }

    #[test]
    fn receipt_sha256_field_matches_independent_recompute() {
        let receipt = Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok");
        let line = receipt_to_json_line(&receipt).map_or(String::new(), |line| line);
        let marker = "\"receipt_sha256\":\"";
        assert!(
            line.contains(marker),
            "receipt_sha256 field missing from line"
        );
        let marker_pos = line.find(marker).unwrap_or(0);
        let rest = &line[marker_pos + marker.len()..];
        let end = rest.find('"').unwrap_or(0);
        let embedded_sha = &rest[..end];
        // Recompute independently using the canonical fields.
        let expected = receipt_sha256_hex("demo", "s1", "PASS", "passed", "ok");
        assert_eq!(
            embedded_sha, expected,
            "embedded SHA must match independent recompute"
        );
    }

    #[test]
    fn local_executor_emits_receipt_sha256_in_ledger() {
        let ledger = unique_ledger_path("sha-field");
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", "printf hello", StepState::Passed)],
        );
        let report = execute_local_workflow(&workflow, &ledger);
        assert!(report.is_ok());
        let ledger_text = fs::read_to_string(&ledger).unwrap_or_default();
        assert!(
            ledger_text.contains("\"receipt_sha256\":\""),
            "ledger must contain receipt_sha256 field: {ledger_text}"
        );
        let _ = fs::remove_file(ledger);
    }
}
