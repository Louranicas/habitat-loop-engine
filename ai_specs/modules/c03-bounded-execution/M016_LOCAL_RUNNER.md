# M016 LocalRunner — local_runner.rs

> **File:** `crates/hle-executor/src/local_runner.rs` | **Target LOC:** ~350 | **Target Tests:** 60
> **Layer:** L03 | **Cluster:** C03_BOUNDED_EXECUTION | **Error Codes:** 2210-2213
> **Role:** One-shot local command runner. The planned topology successor to `substrate_emit::run_local_command_with_timeout`. Enforces allowlist/blocklist/metachar/URL rejection, process-group isolation, TERM→KILL escalation, output bounding, and secret redaction.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `LocalRunner` | struct | No | Configured runner instance with policy baked in at construction |
| `RunnerConfig` | struct | No | Builder-pattern configuration for `LocalRunner` |
| `CommandOutput` | struct | No | Bounded stdout+stderr result from one invocation |
| `RunnerError` | enum | No | Errors 2210-2213 for this module |

---

## LocalRunner

```rust
/// A one-shot local command executor bounded by an explicit timeout policy
/// and output cap.
///
/// Commands must be on the allowlist and must not contain blocked tokens,
/// shell metacharacters, or external URLs. Output is bounded by
/// `max_output_bytes` and secrets are redacted before returning.
///
/// `LocalRunner` executes synchronously (no async). See AP29 — blocking
/// in async is only safe when the caller is not in a Tokio context, which
/// is guaranteed by C03's foreground-M0 execution model.
#[derive(Debug)]
pub struct LocalRunner {
    config: RunnerConfig,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(config: RunnerConfig) -> Result<Self, RunnerError>` | Validates config at construction. |
| `run` | `fn(&self, command: &str) -> Result<CommandOutput, RunnerError>` | One-shot execution. Blocks until completion or timeout. |
| `config` | `fn(&self) -> &RunnerConfig` | #[must_use] |

### RunnerConfig

```rust
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub timeout_policy: TimeoutPolicy,       // from M018
    pub max_output_bytes: usize,             // applied via BoundedString (M015)
    pub extra_allowed_programs: Vec<String>, // additional paths beyond defaults
}
```

| Method | Signature | Notes |
|---|---|---|
| `builder` | `fn() -> RunnerConfigBuilder` | #[must_use] |
| `default_m0` | `fn() -> Self` | 30s graceful / 100ms hard-kill / 64KiB output cap |

```rust
pub struct RunnerConfigBuilder {
    timeout_policy: Option<TimeoutPolicy>,
    max_output_bytes: Option<usize>,
    extra_allowed_programs: Vec<String>,
}

impl RunnerConfigBuilder {
    #[must_use] pub fn timeout_policy(self, policy: TimeoutPolicy) -> Self;
    #[must_use] pub fn max_output_bytes(self, bytes: usize) -> Self;
    #[must_use] pub fn allow_program(self, path: impl Into<String>) -> Self;
    pub fn build(self) -> Result<RunnerConfig, RunnerError>;
}
```

---

## CommandOutput

```rust
/// Result of a single `LocalRunner::run` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: BoundedString,      // M015 — bounded and redacted
    pub stderr: BoundedString,      // M015 — bounded and redacted
    pub exit_code: Option<i32>,     // None if killed by signal
    pub timed_out: bool,
    pub combined_message: String,   // "stdout: ...; stderr: ..." summary
}

impl CommandOutput {
    #[must_use] pub fn was_successful(&self) -> bool;   // exit_code == Some(0)
    #[must_use] pub fn to_step_state(&self) -> StepState; // Passed / Failed
}
```

---

## Security Model

### Allowlist (positive gate — must match first)

```rust
/// Default allowed binaries. All are resolved to absolute /usr/bin/* paths.
const DEFAULT_ALLOWED_PROGRAMS: [&str; 5] = [
    "printf", "true", "false", "sleep", "echo",
];

/// Resolved to absolute paths via `allowed_command_path`:
fn allowed_command_path(basename: &str) -> Option<&'static str> {
    match basename {
        "printf" => Some("/usr/bin/printf"),
        "true"   => Some("/usr/bin/true"),
        "false"  => Some("/usr/bin/false"),
        "sleep"  => Some("/usr/bin/sleep"),
        "echo"   => Some("/usr/bin/echo"),
        _        => None,
    }
}
```

`extra_allowed_programs` in `RunnerConfig` may add additional absolute `/usr/bin/*` paths at runner construction time. Paths not starting with `/usr/bin/` are rejected at config build time.

### Blocklist (secondary safety net — checked after allowlist miss)

```rust
const BLOCKED_TOKENS: [&str; 13] = [
    "curl", "wget", "ssh", "scp", "rsync",
    "nc", "netcat", "socat",
    "hermes", "orac", "povm",
    "rm", "sudo",
];
```

Token detection uses word-boundary splitting on whitespace and shell metacharacters so `printf safe;rm bad` fires the `rm` detector. Basename extraction via `rsplit('/')` catches absolute paths like `/bin/rm`.

### Metacharacter rejection

```rust
const SHELL_METACHAR: &str = ";&|<>$`(){}";
```

Any character in this set causes immediate `CommandRejected`.

### URL rejection

```rust
if command.to_ascii_lowercase().contains("http://")
    || command.to_ascii_lowercase().contains("https://") {
    return Err(RunnerError::CommandRejected { reason: "external URL rejected".into() });
}
```

### Secret redaction

Applied to stdout and stderr before constructing `BoundedString`. Redaction tokens:

```rust
const SECRET_SUFFIXES: [&str; 3] = ["_TOKEN", "_SECRET", "_PASSWORD"];
const EXACT_SECRET_KEYS: [&str; 7] = [
    "API_KEY", "API_TOKEN", "AUTH_TOKEN", "SECRET", "TOKEN", "PASSWORD", "PASSWD",
];
```

Words matching `KEY=VALUE` where KEY (normalized, dashes→underscores, uppercased) matches any suffix or exact key are replaced with `KEY=[REDACTED]`.

---

## Process Lifecycle

```rust
impl LocalRunner {
    fn run(&self, command: &str) -> Result<CommandOutput, RunnerError> {
        let parts = self.parse_and_validate(command)?;   // reject on allowlist/blocklist/metachar/URL
        let (program, args) = parts.split_first()
            .ok_or_else(|| RunnerError::CommandRejected { reason: "empty command".into() })?;
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)      // new process group for TERM/KILL to cover entire tree
            .spawn()
            .map_err(|e| RunnerError::SpawnFailed { program: program.to_owned(), reason: e.to_string() })?;

        // Poll with 10ms sleep until done or graceful timeout elapsed
        self.config.timeout_policy.apply(&mut child)?;  // M018 escalation

        let output = child.wait_with_output()
            .map_err(|e| RunnerError::OutputReadFailed { reason: e.to_string() })?;

        self.build_output(output)  // bound + redact + summarize
    }
}
```

The `TimeoutPolicy::apply` method (M018) handles the TERM→KILL escalation. `LocalRunner` does not contain its own kill logic; escalation is entirely delegated to M018.

---

## RunnerError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    /// Code 2210. Command rejected before spawn (allowlist miss, blocklist hit, metachar, URL).
    CommandRejected { reason: String },
    /// Code 2211. OS-level spawn failure. Retryable if kind is EAGAIN/ResourceBusy.
    SpawnFailed { program: String, reason: String },
    /// Code 2212. wait_with_output failed after child exit.
    OutputReadFailed { reason: String },
    /// Code 2213. Output contained secrets; they were redacted. Not a hard error.
    SecretRedacted,
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` |
| `is_retryable` | `fn(&self) -> bool` — `SpawnFailed` only if OS says resource-temporarily-unavailable |

**Traits:** `Display` ("[HLE-2210] command rejected: ..."), `std::error::Error`

---

## Design Notes

- `LocalRunner` is the topology successor to `substrate_emit::run_local_command_with_timeout`. The existing function remains for `substrate-emit` backward compat; new code in `hle-executor` uses `LocalRunner`.
- `process_group(0)` calls `setpgid(0, 0)` on the child. `TimeoutPolicy::apply` then sends `-PGID` to kill all descendants. This is the same pattern as the existing `terminate_child_group` helper in `substrate-emit`.
- `SecretRedacted` is a warning-level variant, not a hard error. The caller receives `CommandOutput` with redacted content and `RunnerError::SecretRedacted` is surfaced only in the step message, not as a failure state.
- `extra_allowed_programs` allows callers like M017 to add QI-specific binaries (e.g. `/usr/bin/cargo`, `/usr/bin/git`) without modifying the default allowlist. The implementation validates that all extra paths start with `/usr/bin/` or another absolute prefix listed in a compile-time `TRUSTED_PREFIX` set.
- AP29 (blocking in async) is satisfied by design: C03 executes in foreground, synchronous M0 mode. `LocalRunner::run` never enters a Tokio runtime.

---

## Cross-Cluster Events Emitted

- M016 populates `CommandOutput`, which M017 converts to a `Receipt` emitted to C01 (`receipts_store`) and C05 (`evidence_store`).
- The `combined_message` field of `CommandOutput` becomes the `Receipt.message` field after bounding via M015 `MAX_RECEIPT_MESSAGE_BYTES`.

---

*M016 LocalRunner Spec v1.0 | C03 Bounded Execution | 2026-05-10*
