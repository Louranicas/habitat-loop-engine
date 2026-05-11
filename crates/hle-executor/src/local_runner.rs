//! M016 `LocalRunner` — one-shot local command runner.
//!
//! The planned topology successor to `substrate_emit::run_local_command_with_timeout`.
//! Enforces allowlist/blocklist/metachar/URL rejection, process-group isolation,
//! TERM→KILL escalation (delegated to M018 [`TimeoutPolicy`]), output bounding
//! (via M015 [`BoundedString`]), and secret redaction.
//!
//! All execution is synchronous (no async).  AP29 compliance: `LocalRunner::run`
//! never enters a Tokio runtime.
//!
//! Error codes: 2210–2213.

use std::fmt;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};

use substrate_types::StepState;

use crate::bounded::{BoundedString, MAX_COMMAND_OUTPUT_BYTES};
use crate::timeout_policy::TimeoutPolicy;

// ── Security constants ────────────────────────────────────────────────────────

/// Binaries allowed by basename (informational — actual resolution is in
/// [`default_allowed_path`]).
#[allow(dead_code)]
const DEFAULT_ALLOWED_PROGRAMS: [&str; 5] = ["printf", "true", "false", "sleep", "echo"];

/// Tokens that are always rejected, regardless of allowlist.
const BLOCKED_TOKENS: [&str; 13] = [
    "curl", "wget", "ssh", "scp", "rsync", "nc", "netcat", "socat", "hermes", "orac", "povm", "rm",
    "sudo",
];

/// Shell metacharacters whose presence causes immediate rejection.
const SHELL_METACHAR: &str = ";&|<>$`(){}";

/// Secret key suffixes (normalised, dashes→underscores, uppercased).
const SECRET_SUFFIXES: [&str; 3] = ["_TOKEN", "_SECRET", "_PASSWORD"];

/// Exact secret key names (normalised).
const EXACT_SECRET_KEYS: [&str; 7] = [
    "API_KEY",
    "API_TOKEN",
    "AUTH_TOKEN",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
];

/// Trusted prefix for extra allowed programs.
const TRUSTED_PREFIX: &str = "/usr/bin/";

// ── RunnerError ───────────────────────────────────────────────────────────────

/// Errors produced by M016 [`LocalRunner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    /// `[HLE-2210]` Command rejected before spawn (allowlist miss, blocklist
    /// hit, metachar, URL).  Never retryable.
    CommandRejected {
        /// Human-readable rejection reason.
        reason: String,
    },
    /// `[HLE-2211]` OS-level spawn failure.  Retryable when the OS error kind
    /// is `EAGAIN`/`ResourceBusy`.
    SpawnFailed {
        /// Program that failed to spawn.
        program: String,
        /// OS error message.
        reason: String,
    },
    /// `[HLE-2212]` `wait_with_output` failed after child exit.  Never
    /// retryable.
    OutputReadFailed {
        /// OS error message.
        reason: String,
    },
    /// `[HLE-2213]` Output contained secrets; they were redacted.  Warning
    /// level only — callers still receive `CommandOutput`.
    SecretRedacted,
}

impl RunnerError {
    /// HLE error code: 2210–2213.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::CommandRejected { .. } => 2210,
            Self::SpawnFailed { .. } => 2211,
            Self::OutputReadFailed { .. } => 2212,
            Self::SecretRedacted => 2213,
        }
    }

    /// Returns `true` only for [`SpawnFailed`][Self::SpawnFailed] variants
    /// whose `reason` string contains an EAGAIN/`ResourceBusy` indicator.
    ///
    /// Note: We cannot inspect `io::ErrorKind` here because it has been
    /// stringified.  We match on the canonical OS message instead.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::SpawnFailed { reason, .. } => {
                let r = reason.to_ascii_lowercase();
                r.contains("resource temporarily unavailable")
                    || r.contains("eagain")
                    || r.contains("would block")
            }
            _ => false,
        }
    }
}

impl crate::retry_policy::RetryableError for RunnerError {
    fn is_retryable(&self) -> bool {
        RunnerError::is_retryable(self)
    }
}

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandRejected { reason } => {
                write!(f, "[HLE-2210] command rejected: {reason}")
            }
            Self::SpawnFailed { program, reason } => {
                write!(f, "[HLE-2211] spawn failed for '{program}': {reason}")
            }
            Self::OutputReadFailed { reason } => {
                write!(f, "[HLE-2212] output read failed: {reason}")
            }
            Self::SecretRedacted => f.write_str("[HLE-2213] output contained secrets (redacted)"),
        }
    }
}

impl std::error::Error for RunnerError {}

// ── CommandOutput ─────────────────────────────────────────────────────────────

/// Result of a single [`LocalRunner::run`] invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Bounded and redacted stdout.
    pub stdout: BoundedString,
    /// Bounded and redacted stderr.
    pub stderr: BoundedString,
    /// Process exit code; `None` if killed by signal.
    pub exit_code: Option<i32>,
    /// `true` when the command was killed by the [`TimeoutPolicy`].
    pub timed_out: bool,
    /// Human-readable summary: `"stdout: ...; stderr: ..."`.
    pub combined_message: String,
}

impl CommandOutput {
    /// Returns `true` when the process exited with code 0.
    #[must_use]
    pub fn was_successful(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Map to [`StepState`]: `Passed` on exit-0, `Failed` otherwise.
    #[must_use]
    pub fn to_step_state(&self) -> StepState {
        if self.was_successful() {
            StepState::Passed
        } else {
            StepState::Failed
        }
    }
}

// ── RunnerConfigBuilder ───────────────────────────────────────────────────────

/// Builder for [`RunnerConfig`].
#[derive(Debug, Default)]
pub struct RunnerConfigBuilder {
    timeout_policy: Option<TimeoutPolicy>,
    max_output_bytes: Option<usize>,
    extra_allowed_programs: Vec<String>,
}

impl RunnerConfigBuilder {
    /// Set the timeout policy.
    #[must_use]
    pub fn timeout_policy(mut self, policy: TimeoutPolicy) -> Self {
        self.timeout_policy = Some(policy);
        self
    }

    /// Set the maximum combined output bytes.
    #[must_use]
    pub fn max_output_bytes(mut self, bytes: usize) -> Self {
        self.max_output_bytes = Some(bytes);
        self
    }

    /// Allow an additional absolute program path.  Must start with
    /// `/usr/bin/`.
    #[must_use]
    pub fn allow_program(mut self, path: impl Into<String>) -> Self {
        self.extra_allowed_programs.push(path.into());
        self
    }

    /// Build and validate the config.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError::CommandRejected`] when any `extra_allowed_programs`
    /// entry does not start with `/usr/bin/`.
    pub fn build(self) -> Result<RunnerConfig, RunnerError> {
        for path in &self.extra_allowed_programs {
            if !path.starts_with(TRUSTED_PREFIX) {
                return Err(RunnerError::CommandRejected {
                    reason: format!(
                        "extra allowed program '{path}' must start with '{TRUSTED_PREFIX}'"
                    ),
                });
            }
        }
        Ok(RunnerConfig {
            timeout_policy: self.timeout_policy.unwrap_or(TimeoutPolicy::DEFAULT),
            max_output_bytes: self.max_output_bytes.unwrap_or(MAX_COMMAND_OUTPUT_BYTES),
            extra_allowed_programs: self.extra_allowed_programs,
        })
    }
}

// ── RunnerConfig ──────────────────────────────────────────────────────────────

/// Configuration for a [`LocalRunner`] instance.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Timeout + escalation policy (M018).
    pub timeout_policy: TimeoutPolicy,
    /// Maximum combined stdout+stderr bytes (applied via M015 [`BoundedString`]).
    pub max_output_bytes: usize,
    /// Additional absolute `/usr/bin/*` paths permitted beyond the defaults.
    pub extra_allowed_programs: Vec<String>,
}

impl RunnerConfig {
    /// Return a builder.
    #[must_use]
    pub fn builder() -> RunnerConfigBuilder {
        RunnerConfigBuilder::default()
    }

    /// Default M0 config: 30 s graceful / 100 ms hard-kill / 64 KiB output cap.
    #[must_use]
    pub fn default_m0() -> Self {
        Self {
            timeout_policy: TimeoutPolicy::DEFAULT,
            max_output_bytes: MAX_COMMAND_OUTPUT_BYTES,
            extra_allowed_programs: Vec::new(),
        }
    }
}

// ── LocalRunner ───────────────────────────────────────────────────────────────

/// A one-shot local command executor bounded by an explicit timeout policy
/// and output cap.
///
/// Commands must appear on the allowlist and must not contain blocked tokens,
/// shell metacharacters, or external URLs.  Output is bounded by
/// `max_output_bytes` and secrets are redacted before returning.
///
/// `LocalRunner` executes synchronously (no async).  See AP29 — blocking in
/// async is only safe when the caller is not in a Tokio context, which is
/// guaranteed by C03's foreground-M0 execution model.
#[derive(Debug)]
pub struct LocalRunner {
    config: RunnerConfig,
}

impl LocalRunner {
    /// Construct a `LocalRunner` from a validated config.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError::CommandRejected`] when config validation fails.
    pub fn new(config: RunnerConfig) -> Result<Self, RunnerError> {
        // Config is already validated by RunnerConfigBuilder::build.
        // Validate extra_allowed_programs again defensively.
        for path in &config.extra_allowed_programs {
            if !path.starts_with(TRUSTED_PREFIX) {
                return Err(RunnerError::CommandRejected {
                    reason: format!(
                        "extra allowed program '{path}' must start with '{TRUSTED_PREFIX}'"
                    ),
                });
            }
        }
        Ok(Self { config })
    }

    /// The runner's configuration.
    #[must_use]
    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    /// Execute `command` once, blocking until completion or timeout.
    ///
    /// Returns `Ok(CommandOutput)` for both successful and failed child exits;
    /// `Err` only for infrastructure failures (rejected command, spawn failure,
    /// output-read failure).
    ///
    /// # Errors
    ///
    /// - [`RunnerError::CommandRejected`] — pre-spawn security rejection.
    /// - [`RunnerError::SpawnFailed`] — OS-level spawn failure.
    /// - [`RunnerError::OutputReadFailed`] — `wait_with_output` failure.
    pub fn run(&self, command: &str) -> Result<CommandOutput, RunnerError> {
        let parts = self.parse_and_validate(command)?;
        let (program, args) = parts
            .split_first()
            .ok_or_else(|| RunnerError::CommandRejected {
                reason: String::from("empty command after validation"),
            })?;

        // SAFETY: process_group is a unix extension; forbid(unsafe_code) applies
        // to our code, not stdlib internals.
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|err| RunnerError::SpawnFailed {
                program: program.clone(),
                reason: err.to_string(),
            })?;

        // Delegate timeout + escalation entirely to M018.
        let escalation = self.config.timeout_policy.apply(&mut child);

        let timed_out = escalation.is_err();

        let output = child
            .wait_with_output()
            .map_err(|err| RunnerError::OutputReadFailed {
                reason: err.to_string(),
            })?;

        let exit_code = output.status.code();

        let (stdout_raw, was_redacted_out) =
            redact_secrets(&String::from_utf8_lossy(&output.stdout));
        let (stderr_raw, was_redacted_err) =
            redact_secrets(&String::from_utf8_lossy(&output.stderr));

        let stdout =
            BoundedString::new(stdout_raw, self.config.max_output_bytes).map_err(|err| {
                RunnerError::CommandRejected {
                    reason: err.to_string(),
                }
            })?;
        let stderr =
            BoundedString::new(stderr_raw, self.config.max_output_bytes).map_err(|err| {
                RunnerError::CommandRejected {
                    reason: err.to_string(),
                }
            })?;

        let combined_message = build_combined_message(&stdout, &stderr, timed_out);

        if was_redacted_out || was_redacted_err {
            // SecretRedacted is a warning; we still return the output.
            // Callers that care can inspect `combined_message` for [REDACTED].
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
            timed_out,
            combined_message,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Parse and validate `command`, returning `[program, arg1, arg2, ...]`
    /// with the program resolved to an absolute `/usr/bin/*` path.
    fn parse_and_validate(&self, command: &str) -> Result<Vec<String>, RunnerError> {
        // URL check.
        let lowered = command.to_ascii_lowercase();
        if lowered.contains("http://") || lowered.contains("https://") {
            return Err(RunnerError::CommandRejected {
                reason: String::from("external URL rejected for local M0 command"),
            });
        }

        // Metacharacter check.
        if command.chars().any(|ch| SHELL_METACHAR.contains(ch)) {
            return Err(RunnerError::CommandRejected {
                reason: String::from("shell metacharacters rejected for local M0 command"),
            });
        }

        // Blocklist check (word-boundary tokenisation).
        let tokens: Vec<&str> = command.split_whitespace().collect();
        for token in &tokens {
            let base = basename_of(token).to_ascii_lowercase();
            if BLOCKED_TOKENS.contains(&base.as_str()) {
                return Err(RunnerError::CommandRejected {
                    reason: format!("command token '{base}' rejected for local M0 command"),
                });
            }
        }

        // Allowlist check on the program (first token).
        let raw_program = tokens.first().ok_or_else(|| RunnerError::CommandRejected {
            reason: String::from("empty local M0 command rejected"),
        })?;
        let base = basename_of(raw_program).to_ascii_lowercase();

        let resolved = self
            .resolve_program(&base)
            .ok_or_else(|| RunnerError::CommandRejected {
                reason: format!("command token '{base}' is not in the local M0 allowlist"),
            })?;

        let mut parts: Vec<String> = vec![resolved];
        for arg in tokens.iter().skip(1) {
            let a = arg
                .trim_matches(|ch: char| matches!(ch, '"' | '\''))
                .to_owned();
            if !a.is_empty() {
                parts.push(a);
            }
        }
        Ok(parts)
    }

    /// Resolve a lowercase basename to an absolute `/usr/bin/*` path, checking
    /// default and extra allowed programs.
    fn resolve_program(&self, base: &str) -> Option<String> {
        // Check default allowlist.
        if let Some(abs) = default_allowed_path(base) {
            return Some(abs.to_owned());
        }
        // Check extra allowed programs by basename.
        for path in &self.config.extra_allowed_programs {
            let path_base = basename_of(path).to_ascii_lowercase();
            if path_base == base {
                return Some(path.clone());
            }
        }
        None
    }
}

/// Extract the basename from an (optionally absolute) path token.
fn basename_of(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

/// Map default-allowed basenames to their absolute paths.
fn default_allowed_path(base: &str) -> Option<&'static str> {
    match base {
        "printf" => Some("/usr/bin/printf"),
        "true" => Some("/usr/bin/true"),
        "false" => Some("/usr/bin/false"),
        "sleep" => Some("/usr/bin/sleep"),
        "echo" => Some("/usr/bin/echo"),
        _ => None,
    }
}

/// Redact `KEY=[VALUE]` patterns where KEY matches known secret names.
///
/// Returns the (possibly-modified) string and a flag indicating whether any
/// redaction was performed.
fn redact_secrets(input: &str) -> (String, bool) {
    let mut output = String::with_capacity(input.len());
    let mut redacted = false;

    for word in input.split_whitespace() {
        if !output.is_empty() {
            output.push(' ');
        }
        if let Some(eq) = word.find('=') {
            let key_raw = &word[..eq];
            let normalised = key_raw.replace('-', "_").to_ascii_uppercase();
            if is_secret_key(&normalised) {
                output.push_str(key_raw);
                output.push_str("=[REDACTED]");
                redacted = true;
                continue;
            }
        }
        output.push_str(word);
    }
    (output, redacted)
}

/// Returns `true` when `key` (normalised: dashes→underscores, uppercased)
/// matches a known secret suffix or exact key.
fn is_secret_key(key: &str) -> bool {
    if EXACT_SECRET_KEYS.contains(&key) {
        return true;
    }
    SECRET_SUFFIXES.iter().any(|suffix| key.ends_with(suffix))
}

/// Build the combined `"stdout: ...; stderr: ..."` summary used as a step
/// receipt message.
fn build_combined_message(
    stdout: &BoundedString,
    stderr: &BoundedString,
    timed_out: bool,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !stdout.is_empty() {
        parts.push(format!("stdout: {}", stdout.as_str().trim()));
    }
    if !stderr.is_empty() {
        parts.push(format!("stderr: {}", stderr.as_str().trim()));
    }
    if timed_out {
        parts.push(String::from("timed-out: true"));
    }
    parts.join("; ")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_runner() -> LocalRunner {
        LocalRunner::new(RunnerConfig::default_m0()).expect("default runner")
    }

    // ── RunnerConfig ──────────────────────────────────────────────────────────

    #[test]
    fn runner_config_builder_rejects_untrusted_extra_program() {
        let result = RunnerConfig::builder().allow_program("/bin/sh").build();
        assert!(result.is_err());
    }

    #[test]
    fn runner_config_builder_accepts_trusted_extra_program() {
        let result = RunnerConfig::builder()
            .allow_program("/usr/bin/cargo")
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn runner_config_default_m0_timeout_is_30s() {
        let cfg = RunnerConfig::default_m0();
        assert_eq!(cfg.timeout_policy.graceful().as_secs(), 30);
    }

    // ── Command rejection ─────────────────────────────────────────────────────

    #[test]
    fn runner_rejects_url_in_command() {
        let runner = default_runner();
        let err = runner
            .run("echo https://evil.com")
            .expect_err("url rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_http_url_in_command() {
        let runner = default_runner();
        let err = runner
            .run("echo http://example.com")
            .expect_err("http rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_shell_metacharacter_semicolon() {
        let runner = default_runner();
        let err = runner
            .run("echo hello;rm -rf /")
            .expect_err("metachar rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_shell_metacharacter_pipe() {
        let runner = default_runner();
        let err = runner.run("echo hello|cat").expect_err("pipe rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_blocklisted_curl() {
        let runner = default_runner();
        let err = runner
            .run("curl http://example.com")
            .expect_err("curl rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_blocklisted_rm() {
        let runner = default_runner();
        let err = runner.run("rm -rf /").expect_err("rm rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_program_not_in_allowlist() {
        let runner = default_runner();
        let err = runner.run("cat /etc/passwd").expect_err("cat not allowed");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_empty_command() {
        let runner = default_runner();
        let err = runner.run("").expect_err("empty rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    // ── Allowed commands ──────────────────────────────────────────────────────

    #[test]
    fn runner_allows_true_command() {
        let runner = default_runner();
        let output = runner.run("true").expect("true is allowed");
        assert!(output.was_successful());
    }

    #[test]
    fn runner_allows_false_command() {
        let runner = default_runner();
        let output = runner.run("false").expect("false is allowed");
        assert!(!output.was_successful());
        assert_eq!(output.to_step_state(), StepState::Failed);
    }

    #[test]
    fn runner_allows_echo_command() {
        let runner = default_runner();
        let output = runner.run("echo hello").expect("echo is allowed");
        assert!(output.was_successful());
        assert!(output.stdout.as_str().contains("hello"));
    }

    // ── RunnerError ───────────────────────────────────────────────────────────

    #[test]
    fn runner_error_command_rejected_code_is_2210() {
        let e = RunnerError::CommandRejected {
            reason: String::from("x"),
        };
        assert_eq!(e.error_code(), 2210);
    }

    #[test]
    fn runner_error_spawn_failed_code_is_2211() {
        let e = RunnerError::SpawnFailed {
            program: String::from("x"),
            reason: String::from("y"),
        };
        assert_eq!(e.error_code(), 2211);
    }

    #[test]
    fn runner_error_output_read_failed_code_is_2212() {
        let e = RunnerError::OutputReadFailed {
            reason: String::from("x"),
        };
        assert_eq!(e.error_code(), 2212);
    }

    #[test]
    fn runner_error_secret_redacted_code_is_2213() {
        assert_eq!(RunnerError::SecretRedacted.error_code(), 2213);
    }

    #[test]
    fn runner_error_command_rejected_display_contains_hle_2210() {
        let e = RunnerError::CommandRejected {
            reason: String::from("test"),
        };
        assert!(e.to_string().contains("[HLE-2210]"));
    }

    #[test]
    fn command_rejected_not_retryable() {
        let e = RunnerError::CommandRejected {
            reason: String::from("x"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn spawn_failed_retryable_on_eagain_message() {
        let e = RunnerError::SpawnFailed {
            program: String::from("x"),
            reason: String::from("resource temporarily unavailable"),
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn spawn_failed_not_retryable_on_other_error() {
        let e = RunnerError::SpawnFailed {
            program: String::from("x"),
            reason: String::from("no such file or directory"),
        };
        assert!(!e.is_retryable());
    }

    // ── Secret redaction ──────────────────────────────────────────────────────

    #[test]
    fn redact_secrets_replaces_exact_key() {
        let (out, flag) = redact_secrets("TOKEN=abc123");
        assert!(flag);
        assert_eq!(out, "TOKEN=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_suffix_key() {
        let (out, flag) = redact_secrets("MY_TOKEN=abc123");
        assert!(flag);
        assert_eq!(out, "MY_TOKEN=[REDACTED]");
    }

    #[test]
    fn redact_secrets_leaves_safe_word_untouched() {
        let (out, flag) = redact_secrets("SAFE_VAR=value");
        assert!(!flag);
        assert_eq!(out, "SAFE_VAR=value");
    }

    // ── Additional command rejection tests ────────────────────────────────────

    #[test]
    fn runner_rejects_blocklisted_wget() {
        let runner = default_runner();
        let err = runner
            .run("wget https://example.com")
            .expect_err("wget rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_blocklisted_ssh() {
        let runner = default_runner();
        let err = runner.run("ssh user@host").expect_err("ssh rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_blocklisted_sudo() {
        let runner = default_runner();
        let err = runner.run("sudo true").expect_err("sudo rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_absolute_path_blocked_rm() {
        let runner = default_runner();
        let err = runner
            .run("/bin/rm -rf /")
            .expect_err("absolute rm rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_absolute_path_blocked_curl() {
        let runner = default_runner();
        let err = runner
            .run("/usr/bin/curl http://example.com")
            .expect_err("absolute curl rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_ampersand() {
        let runner = default_runner();
        let err = runner.run("true&&false").expect_err("& rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_backtick() {
        let runner = default_runner();
        let err = runner.run("echo `id`").expect_err("backtick rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_dollar() {
        let runner = default_runner();
        let err = runner.run("echo $HOME").expect_err("dollar rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_redirect() {
        let runner = default_runner();
        let err = runner
            .run("echo hi > /tmp/x")
            .expect_err("redirect rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_paren() {
        let runner = default_runner();
        let err = runner.run("echo (hi)").expect_err("paren rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    #[test]
    fn runner_rejects_metachar_brace() {
        let runner = default_runner();
        let err = runner.run("echo {a,b}").expect_err("brace rejected");
        assert!(matches!(err, RunnerError::CommandRejected { .. }));
    }

    // ── Allowed commands — additional ────────────────────────────────────────

    #[test]
    fn runner_allows_printf_command() {
        let runner = default_runner();
        let output = runner.run("printf hello").expect("printf allowed");
        assert!(output.was_successful());
        assert!(output.stdout.as_str().contains("hello"));
    }

    #[test]
    fn runner_allows_sleep_zero() {
        let runner = default_runner();
        let output = runner.run("sleep 0").expect("sleep 0 allowed");
        assert!(output.was_successful());
    }

    #[test]
    fn runner_true_maps_to_step_state_passed() {
        let runner = default_runner();
        let output = runner.run("true").expect("ok");
        assert_eq!(output.to_step_state(), substrate_types::StepState::Passed);
    }

    #[test]
    fn runner_false_exit_code_is_1() {
        let runner = default_runner();
        let output = runner.run("false").expect("ok");
        assert_eq!(output.exit_code, Some(1));
    }

    #[test]
    fn runner_echo_stdout_not_empty() {
        let runner = default_runner();
        let output = runner.run("echo world").expect("ok");
        assert!(!output.stdout.is_empty());
    }

    #[test]
    fn runner_true_has_empty_stdout() {
        let runner = default_runner();
        let output = runner.run("true").expect("ok");
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn runner_allows_extra_trusted_program() {
        let cfg = RunnerConfig::builder()
            .allow_program("/usr/bin/env")
            .build()
            .expect("ok");
        let runner = LocalRunner::new(cfg).expect("ok");
        // env with no args should succeed
        let result = runner.run("env");
        // May or may not succeed depending on env output, but must not reject.
        match result {
            Ok(_)
            | Err(RunnerError::OutputReadFailed { .. })
            | Err(RunnerError::SpawnFailed { .. }) => {}
            Err(RunnerError::CommandRejected { .. }) => {
                panic!("extra allowed program was rejected")
            }
            Err(RunnerError::SecretRedacted) => {}
        }
    }

    #[test]
    fn runner_extra_untrusted_program_rejected_at_build() {
        let result = RunnerConfig::builder().allow_program("/bin/ls").build();
        assert!(result.is_err());
    }

    // ── Secret redaction — additional ────────────────────────────────────────

    #[test]
    fn redact_secrets_replaces_password_key() {
        let (out, flag) = redact_secrets("PASSWORD=hunter2");
        assert!(flag);
        assert_eq!(out, "PASSWORD=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_api_key() {
        let (out, flag) = redact_secrets("API_KEY=sk-12345");
        assert!(flag);
        assert_eq!(out, "API_KEY=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_api_token() {
        let (out, flag) = redact_secrets("API_TOKEN=abc");
        assert!(flag);
        assert_eq!(out, "API_TOKEN=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_auth_token() {
        let (out, flag) = redact_secrets("AUTH_TOKEN=xyz");
        assert!(flag);
        assert_eq!(out, "AUTH_TOKEN=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_passwd_key() {
        let (out, flag) = redact_secrets("PASSWD=secret");
        assert!(flag);
        assert_eq!(out, "PASSWD=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_my_secret_suffix() {
        let (out, flag) = redact_secrets("MY_SECRET=abc");
        assert!(flag);
        assert_eq!(out, "MY_SECRET=[REDACTED]");
    }

    #[test]
    fn redact_secrets_replaces_my_password_suffix() {
        let (out, flag) = redact_secrets("MY_PASSWORD=abc");
        assert!(flag);
        assert_eq!(out, "MY_PASSWORD=[REDACTED]");
    }

    #[test]
    fn redact_secrets_multi_word_redacts_secret_only() {
        let (out, flag) = redact_secrets("SAFE=ok TOKEN=secret other=value");
        assert!(flag);
        assert!(out.contains("TOKEN=[REDACTED]"));
        assert!(out.contains("SAFE=ok"));
        assert!(out.contains("other=value"));
    }

    #[test]
    fn redact_secrets_empty_input_no_redaction() {
        let (out, flag) = redact_secrets("");
        assert!(!flag);
        assert_eq!(out, "");
    }

    #[test]
    fn redact_secrets_no_equals_sign_no_redaction() {
        let (out, flag) = redact_secrets("TOKEN");
        assert!(!flag);
        assert_eq!(out, "TOKEN");
    }

    // ── CommandOutput helpers ─────────────────────────────────────────────────

    #[test]
    fn command_output_was_successful_on_exit_0() {
        use crate::bounded::BoundedString;
        let out = super::CommandOutput {
            stdout: BoundedString::new("", 64).expect("ok"),
            stderr: BoundedString::new("", 64).expect("ok"),
            exit_code: Some(0),
            timed_out: false,
            combined_message: String::new(),
        };
        assert!(out.was_successful());
    }

    #[test]
    fn command_output_not_successful_on_exit_1() {
        use crate::bounded::BoundedString;
        let out = super::CommandOutput {
            stdout: BoundedString::new("", 64).expect("ok"),
            stderr: BoundedString::new("", 64).expect("ok"),
            exit_code: Some(1),
            timed_out: false,
            combined_message: String::new(),
        };
        assert!(!out.was_successful());
    }

    #[test]
    fn command_output_not_successful_on_signal_kill() {
        use crate::bounded::BoundedString;
        let out = super::CommandOutput {
            stdout: BoundedString::new("", 64).expect("ok"),
            stderr: BoundedString::new("", 64).expect("ok"),
            exit_code: None, // killed by signal
            timed_out: true,
            combined_message: String::new(),
        };
        assert!(!out.was_successful());
    }

    // ── RunnerError additional ────────────────────────────────────────────────

    #[test]
    fn runner_error_output_read_failed_display_contains_hle_2212() {
        let e = RunnerError::OutputReadFailed {
            reason: String::from("io err"),
        };
        assert!(e.to_string().contains("[HLE-2212]"));
    }

    #[test]
    fn runner_error_secret_redacted_display_contains_hle_2213() {
        assert!(RunnerError::SecretRedacted
            .to_string()
            .contains("[HLE-2213]"));
    }

    #[test]
    fn runner_error_secret_redacted_not_retryable() {
        assert!(!RunnerError::SecretRedacted.is_retryable());
    }

    #[test]
    fn spawn_failed_retryable_on_would_block_message() {
        let e = RunnerError::SpawnFailed {
            program: String::from("x"),
            reason: String::from("would block"),
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn runner_config_max_output_bytes_settable() {
        let cfg = RunnerConfig::builder()
            .max_output_bytes(1024)
            .build()
            .expect("ok");
        assert_eq!(cfg.max_output_bytes, 1024);
    }

    #[test]
    fn runner_new_rejects_untrusted_extra_in_config() {
        let cfg = RunnerConfig {
            timeout_policy: crate::timeout_policy::TimeoutPolicy::DEFAULT,
            max_output_bytes: crate::bounded::MAX_COMMAND_OUTPUT_BYTES,
            extra_allowed_programs: vec![String::from("/bin/sh")],
        };
        let result = LocalRunner::new(cfg);
        assert!(result.is_err());
    }

    #[test]
    fn runner_config_accessor() {
        let runner = default_runner();
        assert_eq!(
            runner.config().max_output_bytes,
            crate::bounded::MAX_COMMAND_OUTPUT_BYTES
        );
    }
}
