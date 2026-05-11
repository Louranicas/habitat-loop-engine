//! M018 `TimeoutPolicy` — TERM-to-KILL bounded timeout policy primitive.
//!
//! Encodes the two-phase escalation pattern from `substrate_emit`
//! (`-TERM` → 100 ms sleep → `-KILL`) as a typed, validated value type with
//! declared durations.  All escalation logic is in [`TimeoutPolicy::apply`];
//! `LocalRunner` (M016) delegates to this module entirely and carries no kill
//! logic of its own.
//!
//! Error codes: 2230–2231.

use std::fmt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

// ── TimeoutPolicyError ───────────────────────────────────────────────────────

/// Errors produced by M018 timeout-policy construction and application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutPolicyError {
    /// `[HLE-2230]` Escalation completed; the child was killed by the policy.
    ///
    /// This is informational — it does not prevent `CommandOutput` from being
    /// returned; `LocalRunner` converts it to `timed_out: true`.
    TimeoutElapsed {
        /// Graceful window in milliseconds.
        graceful_ms: u64,
        /// Hard-kill window in milliseconds.
        hard_kill_ms: u64,
    },
    /// `[HLE-2231]` Policy values are invalid (zero duration, inverted range).
    InvalidTimeoutPolicy {
        /// Human-readable reason.
        reason: String,
    },
}

impl TimeoutPolicyError {
    /// HLE error code: 2230 or 2231.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::TimeoutElapsed { .. } => 2230,
            Self::InvalidTimeoutPolicy { .. } => 2231,
        }
    }

    /// All `TimeoutPolicyError` variants are non-retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        false
    }
}

impl fmt::Display for TimeoutPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeoutElapsed {
                graceful_ms,
                hard_kill_ms,
            } => write!(
                f,
                "[HLE-2230] timeout elapsed: graceful={graceful_ms}ms hard_kill={hard_kill_ms}ms"
            ),
            Self::InvalidTimeoutPolicy { reason } => {
                write!(f, "[HLE-2231] invalid timeout policy: {reason}")
            }
        }
    }
}

impl std::error::Error for TimeoutPolicyError {}

// ── EscalationOutcome ────────────────────────────────────────────────────────

/// Whether SIGTERM alone sufficed or SIGKILL was needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationOutcome {
    /// The child exited on its own before the graceful timeout.
    ExitedCleanly,
    /// The child was sent SIGTERM then SIGKILL; the process group was killed.
    KilledByPolicy,
}

impl EscalationOutcome {
    /// Returns `true` when the child was killed by policy escalation.
    #[must_use]
    pub const fn was_killed(self) -> bool {
        matches!(self, Self::KilledByPolicy)
    }

    /// Stable wire string: `"clean"` or `"killed"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExitedCleanly => "clean",
            Self::KilledByPolicy => "killed",
        }
    }
}

impl fmt::Display for EscalationOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExitedCleanly => "ExitedCleanly",
            Self::KilledByPolicy => "KilledByPolicy",
        })
    }
}

// ── TimeoutPolicy ────────────────────────────────────────────────────────────

/// A two-phase process-termination policy: send SIGTERM, wait `hard_kill`,
/// then send SIGKILL to the process group.
///
/// `hard_kill` must be strictly less than `graceful` so that the escalation
/// window never arrives before the graceful deadline.
///
/// `TimeoutPolicy` is `Copy` because it contains only two `Duration` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutPolicy {
    graceful: Duration,
    hard_kill: Duration,
}

impl TimeoutPolicy {
    /// Default policy matching `substrate_emit`: 30 s graceful, 100 ms
    /// hard-kill window.
    pub const DEFAULT: Self = Self {
        graceful: Duration::from_secs(30),
        hard_kill: Duration::from_millis(100),
    };

    /// Fast policy for tests: 10 ms graceful, 50 ms hard-kill window.
    ///
    /// Note: `hard_kill > graceful` here is intentional for test speed; it
    /// bypasses the builder invariant.  Do NOT use in production code.
    pub const TEST_FAST: Self = Self {
        graceful: Duration::from_millis(10),
        hard_kill: Duration::from_millis(50),
    };

    /// Return a validated builder.
    #[must_use]
    pub fn builder() -> TimeoutPolicyBuilder {
        TimeoutPolicyBuilder::default()
    }

    /// Total wall-clock budget: `graceful + hard_kill`.
    #[must_use]
    pub fn total_budget(&self) -> Duration {
        self.graceful.saturating_add(self.hard_kill)
    }

    /// The total wall-clock timeout before SIGTERM is sent.
    #[must_use]
    pub const fn graceful(&self) -> Duration {
        self.graceful
    }

    /// The time between SIGTERM and SIGKILL.
    #[must_use]
    pub const fn hard_kill(&self) -> Duration {
        self.hard_kill
    }

    /// Block until `child` exits or the graceful timeout elapses, then
    /// escalate via SIGTERM → SIGKILL.
    ///
    /// Polls at 10 ms intervals (inherited from `substrate_emit` discipline;
    /// not a busy-spin, safe for foreground M0 execution).
    ///
    /// Returns [`EscalationOutcome::ExitedCleanly`] when the child exits
    /// before the deadline, or [`EscalationOutcome::KilledByPolicy`] after
    /// the full TERM→KILL sequence completes.
    ///
    /// # Errors
    ///
    /// Returns [`TimeoutPolicyError::TimeoutElapsed`] after the escalation
    /// sequence runs (i.e., when `KilledByPolicy` applies).  Callers should
    /// treat this as an informational signal, not a hard error.
    pub fn apply(&self, child: &mut Child) -> Result<EscalationOutcome, TimeoutPolicyError> {
        let started = Instant::now();

        while started.elapsed() < self.graceful {
            match child.try_wait() {
                Ok(Some(_)) => return Ok(EscalationOutcome::ExitedCleanly),
                Ok(None) => {}
                Err(_) => {
                    // try_wait failed — fall through to escalation below.
                    break;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }

        // Graceful timeout elapsed or try_wait failed — send SIGTERM to group.
        terminate_process_group(child.id(), "-TERM");
        thread::sleep(self.hard_kill);

        // Hard-kill window elapsed — send SIGKILL to group.
        terminate_process_group(child.id(), "-KILL");
        // Belt-and-suspenders: kill the direct child handle as well.
        child.kill().ok();

        Err(TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: u64::try_from(self.graceful.as_millis()).unwrap_or(u64::MAX),
            hard_kill_ms: u64::try_from(self.hard_kill.as_millis()).unwrap_or(u64::MAX),
        })
    }
}

impl fmt::Display for TimeoutPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TimeoutPolicy(graceful={:?}, hard_kill={:?})",
            self.graceful, self.hard_kill
        )
    }
}

/// Send `signal` (e.g. `"-TERM"` or `"-KILL"`) to the negative PID which
/// targets the entire process group created by `Command::process_group(0)` in
/// `LocalRunner`.  Mirrors `substrate_emit::terminate_child_group`.
fn terminate_process_group(pid: u32, signal: &str) {
    Command::new("/usr/bin/kill")
        .arg(signal)
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

// ── TimeoutPolicyBuilder ──────────────────────────────────────────────────────

/// Validated builder for [`TimeoutPolicy`].
#[derive(Debug, Default)]
pub struct TimeoutPolicyBuilder {
    graceful: Option<Duration>,
    hard_kill: Option<Duration>,
}

impl TimeoutPolicyBuilder {
    /// Set the graceful (total wall-clock) timeout.
    #[must_use]
    pub fn graceful(mut self, d: Duration) -> Self {
        self.graceful = Some(d);
        self
    }

    /// Set the hard-kill grace window (time between SIGTERM and SIGKILL).
    #[must_use]
    pub fn hard_kill(mut self, d: Duration) -> Self {
        self.hard_kill = Some(d);
        self
    }

    /// Build the policy.
    ///
    /// # Errors
    ///
    /// Returns [`TimeoutPolicyError::InvalidTimeoutPolicy`] when:
    /// - `graceful` is not set or is zero.
    /// - `hard_kill` is not set or is zero.
    /// - `hard_kill >= graceful`.
    pub fn build(self) -> Result<TimeoutPolicy, TimeoutPolicyError> {
        let graceful = self
            .graceful
            .ok_or_else(|| TimeoutPolicyError::InvalidTimeoutPolicy {
                reason: String::from("graceful duration not set"),
            })?;
        let hard_kill = self
            .hard_kill
            .ok_or_else(|| TimeoutPolicyError::InvalidTimeoutPolicy {
                reason: String::from("hard_kill duration not set"),
            })?;
        if graceful.is_zero() {
            return Err(TimeoutPolicyError::InvalidTimeoutPolicy {
                reason: String::from("graceful must be non-zero"),
            });
        }
        if hard_kill.is_zero() {
            return Err(TimeoutPolicyError::InvalidTimeoutPolicy {
                reason: String::from("hard_kill must be non-zero"),
            });
        }
        if hard_kill >= graceful {
            return Err(TimeoutPolicyError::InvalidTimeoutPolicy {
                reason: format!(
                    "hard_kill ({hard_kill:?}) must be strictly less than graceful ({graceful:?})"
                ),
            });
        }
        Ok(TimeoutPolicy {
            graceful,
            hard_kill,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── TimeoutPolicyBuilder ─────────────────────────────────────────────────

    #[test]
    fn builder_accepts_valid_policy() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(30))
            .hard_kill(Duration::from_millis(100))
            .build();
        assert!(policy.is_ok());
    }

    #[test]
    fn builder_rejects_zero_graceful() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::ZERO)
            .hard_kill(Duration::from_millis(100))
            .build();
        assert!(policy.is_err());
    }

    #[test]
    fn builder_rejects_zero_hard_kill() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(10))
            .hard_kill(Duration::ZERO)
            .build();
        assert!(policy.is_err());
    }

    #[test]
    fn builder_rejects_hard_kill_equal_to_graceful() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(1))
            .hard_kill(Duration::from_secs(1))
            .build();
        assert!(policy.is_err());
    }

    #[test]
    fn builder_rejects_hard_kill_greater_than_graceful() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_millis(50))
            .hard_kill(Duration::from_secs(1))
            .build();
        assert!(policy.is_err());
    }

    #[test]
    fn builder_rejects_missing_graceful() {
        let policy = TimeoutPolicy::builder()
            .hard_kill(Duration::from_millis(100))
            .build();
        assert!(policy.is_err());
    }

    #[test]
    fn builder_rejects_missing_hard_kill() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(10))
            .build();
        assert!(policy.is_err());
    }

    // ── TimeoutPolicy constants ───────────────────────────────────────────────

    #[test]
    fn default_policy_graceful_is_30s() {
        assert_eq!(TimeoutPolicy::DEFAULT.graceful(), Duration::from_secs(30));
    }

    #[test]
    fn default_policy_hard_kill_is_100ms() {
        assert_eq!(
            TimeoutPolicy::DEFAULT.hard_kill(),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn default_policy_total_budget_is_graceful_plus_hard_kill() {
        let policy = TimeoutPolicy::DEFAULT;
        assert_eq!(policy.total_budget(), Duration::from_millis(30_100));
    }

    // ── EscalationOutcome ────────────────────────────────────────────────────

    #[test]
    fn exited_cleanly_was_killed_is_false() {
        assert!(!EscalationOutcome::ExitedCleanly.was_killed());
    }

    #[test]
    fn killed_by_policy_was_killed_is_true() {
        assert!(EscalationOutcome::KilledByPolicy.was_killed());
    }

    #[test]
    fn exited_cleanly_as_str_is_clean() {
        assert_eq!(EscalationOutcome::ExitedCleanly.as_str(), "clean");
    }

    #[test]
    fn killed_by_policy_as_str_is_killed() {
        assert_eq!(EscalationOutcome::KilledByPolicy.as_str(), "killed");
    }

    // ── TimeoutPolicyError ───────────────────────────────────────────────────

    #[test]
    fn timeout_elapsed_error_code_is_2230() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 30_000,
            hard_kill_ms: 100,
        };
        assert_eq!(e.error_code(), 2230);
    }

    #[test]
    fn invalid_policy_error_code_is_2231() {
        let e = TimeoutPolicyError::InvalidTimeoutPolicy {
            reason: String::from("zero"),
        };
        assert_eq!(e.error_code(), 2231);
    }

    #[test]
    fn timeout_policy_error_not_retryable() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 1000,
            hard_kill_ms: 100,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn timeout_elapsed_display_contains_hle_2230() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 30_000,
            hard_kill_ms: 100,
        };
        assert!(e.to_string().contains("[HLE-2230]"));
    }

    #[test]
    fn invalid_policy_display_contains_hle_2231() {
        let e = TimeoutPolicyError::InvalidTimeoutPolicy {
            reason: String::from("test"),
        };
        assert!(e.to_string().contains("[HLE-2231]"));
    }

    // ── TimeoutPolicy — additional builder and accessor coverage ─────────────

    #[test]
    fn builder_produces_policy_with_correct_graceful() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(5))
            .hard_kill(Duration::from_millis(100))
            .build()
            .expect("ok");
        assert_eq!(policy.graceful(), Duration::from_secs(5));
    }

    #[test]
    fn builder_produces_policy_with_correct_hard_kill() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(5))
            .hard_kill(Duration::from_millis(200))
            .build()
            .expect("ok");
        assert_eq!(policy.hard_kill(), Duration::from_millis(200));
    }

    #[test]
    fn total_budget_equals_graceful_plus_hard_kill() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(10))
            .hard_kill(Duration::from_millis(500))
            .build()
            .expect("ok");
        assert_eq!(policy.total_budget(), Duration::from_millis(10_500));
    }

    #[test]
    fn test_fast_constant_graceful_is_10ms() {
        assert_eq!(
            TimeoutPolicy::TEST_FAST.graceful(),
            Duration::from_millis(10)
        );
    }

    #[test]
    fn test_fast_constant_hard_kill_is_50ms() {
        assert_eq!(
            TimeoutPolicy::TEST_FAST.hard_kill(),
            Duration::from_millis(50)
        );
    }

    #[test]
    fn timeout_policy_is_copy() {
        let p = TimeoutPolicy::DEFAULT;
        let q = p; // Copy, not move
        assert_eq!(p.graceful(), q.graceful());
    }

    #[test]
    fn timeout_policy_display_contains_graceful_and_hard_kill() {
        let s = TimeoutPolicy::DEFAULT.to_string();
        assert!(s.contains("TimeoutPolicy"));
        assert!(s.contains("graceful"));
        assert!(s.contains("hard_kill"));
    }

    // ── EscalationOutcome — additional ───────────────────────────────────────

    #[test]
    fn exited_cleanly_display_is_exited_cleanly() {
        assert_eq!(
            EscalationOutcome::ExitedCleanly.to_string(),
            "ExitedCleanly"
        );
    }

    #[test]
    fn killed_by_policy_display_is_killed_by_policy() {
        assert_eq!(
            EscalationOutcome::KilledByPolicy.to_string(),
            "KilledByPolicy"
        );
    }

    #[test]
    fn escalation_outcome_copy_trait() {
        let a = EscalationOutcome::ExitedCleanly;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    // ── apply() — process actually exits cleanly ──────────────────────────────

    #[test]
    fn apply_returns_exited_cleanly_for_quick_process() {
        // `true` exits immediately — should never hit the timeout.
        let policy = TimeoutPolicy::DEFAULT;
        let mut child = std::process::Command::new("/usr/bin/true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("true spawns");
        let outcome = policy.apply(&mut child).expect("clean exit");
        assert_eq!(outcome, EscalationOutcome::ExitedCleanly);
        assert!(!outcome.was_killed());
    }

    #[test]
    fn apply_returns_exited_cleanly_for_false_process() {
        // `false` exits immediately with code 1 — still a clean exit (not timeout).
        let policy = TimeoutPolicy::DEFAULT;
        let mut child = std::process::Command::new("/usr/bin/false")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("false spawns");
        let outcome = policy.apply(&mut child).expect("clean exit");
        assert_eq!(outcome, EscalationOutcome::ExitedCleanly);
    }

    // ── apply() — process exceeds graceful window and must be killed ──────────

    #[test]
    fn apply_kills_hung_process_after_timeout() {
        use std::os::unix::process::CommandExt as _;
        // sleep 60: will not exit within 15ms graceful window.
        let policy = TimeoutPolicy {
            graceful: Duration::from_millis(15),
            hard_kill: Duration::from_millis(50),
        };
        let mut child = std::process::Command::new("/usr/bin/sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sleep spawns");
        let result = policy.apply(&mut child);
        assert!(
            result.is_err(),
            "should return TimeoutElapsed for hung process"
        );
        if let Err(TimeoutPolicyError::TimeoutElapsed {
            graceful_ms,
            hard_kill_ms,
        }) = result
        {
            assert_eq!(graceful_ms, 15);
            assert_eq!(hard_kill_ms, 50);
        } else {
            panic!("unexpected error variant");
        }
    }

    #[test]
    fn apply_timeout_elapsed_error_code_is_2230() {
        use std::os::unix::process::CommandExt as _;
        let policy = TimeoutPolicy {
            graceful: Duration::from_millis(15),
            hard_kill: Duration::from_millis(50),
        };
        let mut child = std::process::Command::new("/usr/bin/sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sleep spawns");
        let result = policy.apply(&mut child);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), 2230);
    }

    // ── TimeoutPolicyError — additional ──────────────────────────────────────

    #[test]
    fn timeout_elapsed_error_is_not_retryable() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 30_000,
            hard_kill_ms: 100,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn invalid_policy_error_is_not_retryable() {
        let e = TimeoutPolicyError::InvalidTimeoutPolicy {
            reason: String::from("test"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn timeout_elapsed_display_contains_graceful_and_hard_kill() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 5000,
            hard_kill_ms: 200,
        };
        let s = e.to_string();
        assert!(s.contains("5000"));
        assert!(s.contains("200"));
    }

    #[test]
    fn invalid_policy_display_contains_reason() {
        let e = TimeoutPolicyError::InvalidTimeoutPolicy {
            reason: String::from("my custom reason"),
        };
        assert!(e.to_string().contains("my custom reason"));
    }

    #[test]
    fn timeout_policy_error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(TimeoutPolicyError::InvalidTimeoutPolicy {
            reason: String::from("test"),
        });
        assert!(e.to_string().contains("[HLE-2231]"));
    }

    #[test]
    fn timeout_policy_error_clone_equality() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 1000,
            hard_kill_ms: 100,
        };
        assert_eq!(e.clone(), e);
    }

    // ── Additional TimeoutPolicy and EscalationOutcome tests ─────────────────

    #[test]
    fn timeout_policy_equality() {
        let a = TimeoutPolicy::DEFAULT;
        let b = TimeoutPolicy::DEFAULT;
        assert_eq!(a, b);
    }

    #[test]
    fn test_fast_and_default_are_not_equal() {
        assert_ne!(TimeoutPolicy::DEFAULT, TimeoutPolicy::TEST_FAST);
    }

    #[test]
    fn builder_minimum_valid_policy() {
        // graceful=2ms, hard_kill=1ms: hard_kill < graceful and both non-zero.
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_millis(2))
            .hard_kill(Duration::from_millis(1))
            .build();
        assert!(policy.is_ok());
    }

    #[test]
    fn total_budget_does_not_overflow_on_large_values() {
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(3600))
            .hard_kill(Duration::from_secs(1))
            .build()
            .expect("ok");
        // saturating_add should not panic.
        let _ = policy.total_budget();
    }

    #[test]
    fn escalation_outcome_equality() {
        assert_eq!(
            EscalationOutcome::ExitedCleanly,
            EscalationOutcome::ExitedCleanly
        );
        assert_eq!(
            EscalationOutcome::KilledByPolicy,
            EscalationOutcome::KilledByPolicy
        );
        assert_ne!(
            EscalationOutcome::ExitedCleanly,
            EscalationOutcome::KilledByPolicy
        );
    }

    #[test]
    fn timeout_elapsed_graceful_ms_field_accessible() {
        let e = TimeoutPolicyError::TimeoutElapsed {
            graceful_ms: 12_000,
            hard_kill_ms: 250,
        };
        if let TimeoutPolicyError::TimeoutElapsed {
            graceful_ms,
            hard_kill_ms,
        } = &e
        {
            assert_eq!(*graceful_ms, 12_000);
            assert_eq!(*hard_kill_ms, 250);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn apply_exited_cleanly_is_not_killed() {
        let policy = TimeoutPolicy::DEFAULT;
        let mut child = std::process::Command::new("/usr/bin/true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("true spawns");
        let outcome = policy.apply(&mut child).expect("clean exit");
        assert!(!outcome.was_killed());
        assert_eq!(outcome.as_str(), "clean");
    }

    #[test]
    fn apply_echo_exits_cleanly() {
        let policy = TimeoutPolicy::DEFAULT;
        let mut child = std::process::Command::new("/usr/bin/echo")
            .arg("hello")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("echo spawns");
        let outcome = policy.apply(&mut child).expect("clean exit");
        assert_eq!(outcome, EscalationOutcome::ExitedCleanly);
    }

    #[test]
    fn apply_printf_exits_cleanly() {
        let policy = TimeoutPolicy::DEFAULT;
        let mut child = std::process::Command::new("/usr/bin/printf")
            .arg("test")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("printf spawns");
        let outcome = policy.apply(&mut child).expect("clean exit");
        assert_eq!(outcome, EscalationOutcome::ExitedCleanly);
    }

    #[test]
    fn killed_by_policy_as_str_is_killed_confirmed() {
        assert_eq!(EscalationOutcome::KilledByPolicy.as_str(), "killed");
        assert!(EscalationOutcome::KilledByPolicy.was_killed());
    }

    #[test]
    fn builder_successive_overrides_last_value_wins() {
        // Calling .graceful() twice: only the last value should take effect.
        let policy = TimeoutPolicy::builder()
            .graceful(Duration::from_secs(100))
            .graceful(Duration::from_secs(10)) // override
            .hard_kill(Duration::from_millis(100))
            .build()
            .expect("ok");
        assert_eq!(policy.graceful(), Duration::from_secs(10));
    }
}
