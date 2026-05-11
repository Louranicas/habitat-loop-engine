//! `M049` — `cli_daemon_once`: bounded `hle daemon --once` adapter.
//!
//! Enforces the one-shot boundary: refuses to run without `--once`, delegates
//! to `run::run_workflow`, and wraps output with the bounded-once prefix.
//!
//! CRITICAL invariant: the exact string
//! `"this codebase needs to be 'one shotted'"`
//! MUST appear verbatim in any 2730 rejection error so downstream tooling
//! can detect it.
//!
//! Error codes: 2730-2731.

#![forbid(unsafe_code)]
// Stub module: public items are not yet called from main.rs.
#![allow(dead_code)]

use std::path::Path;
use substrate_types::HleError;

/// Prefix prepended to all successful daemon-once output.
pub const DAEMON_ONCE_PREFIX: &str = "hle daemon bounded-once complete; ";

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute the `hle daemon --once` command.
///
/// Guard: if `once` is `false`, returns `HleError` 2730 containing the
/// verbatim framework string `"this codebase needs to be 'one shotted'"`.
///
/// If the guard passes, delegates to `crate::run::run_workflow` and wraps the
/// result with [`DAEMON_ONCE_PREFIX`].
///
/// # Errors
///
/// - `HleError` 2730 when `once` is `false`.
/// - `HleError` 2731 wrapping the inner run error on executor failure.
pub fn daemon_once(
    once: bool,
    workflow: &Path,
    ledger: &Path,
    json: bool,
) -> Result<String, HleError> {
    guard_once(once)?;
    let report = crate::run::run_workflow(workflow, ledger).map_err(|inner| {
        HleError::new(format!("[2731] daemon --once inner run failed: {inner}"))
    })?;
    let inner = crate::run::format_report(&report, json);
    Ok(wrap_result(&inner))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Enforce the `--once` gate.
///
/// Returns `Ok(())` if `once` is `true`.
/// Returns `Err` with code 2730 and the verbatim framework string if `false`.
fn guard_once(once: bool) -> Result<(), HleError> {
    if once {
        return Ok(());
    }
    Err(HleError::new(
        "[2730] daemon command requires --once because this codebase needs to be \
         'one shotted' for bounded M0 operation",
    ))
}

/// Prepend [`DAEMON_ONCE_PREFIX`] to the inner run result string.
fn wrap_result(inner: &str) -> String {
    format!("{DAEMON_ONCE_PREFIX}{inner}")
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
        std::env::temp_dir().join(format!("hle-daemon-{name}-{}.jsonl", std::process::id()))
    }

    // -- guard_once -------------------------------------------------------------

    #[test]
    fn guard_once_true_returns_ok() {
        assert!(guard_once(true).is_ok());
    }

    #[test]
    fn guard_once_false_returns_err() {
        assert!(guard_once(false).is_err());
    }

    #[test]
    fn guard_once_err_message_contains_one_shotted() {
        let e = guard_once(false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(e.contains("one shotted"), "expected 'one shotted' in: {e}");
    }

    #[test]
    fn guard_once_err_message_contains_2730_code() {
        let e = guard_once(false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(e.contains("2730"), "expected '2730' in: {e}");
    }

    #[test]
    fn guard_once_err_message_verbatim_framework_string() {
        let e = guard_once(false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            e.contains("this codebase needs to be 'one shotted'"),
            "verbatim framework string missing: {e}"
        );
    }

    // -- wrap_result ------------------------------------------------------------

    #[test]
    fn wrap_result_prepends_prefix() {
        let out = wrap_result("inner");
        assert!(out.starts_with(DAEMON_ONCE_PREFIX));
    }

    #[test]
    fn wrap_result_preserves_inner() {
        let inner = "hle run verdict=PASS";
        let out = wrap_result(inner);
        assert!(out.contains(inner));
    }

    #[test]
    fn daemon_once_prefix_const_value() {
        assert_eq!(DAEMON_ONCE_PREFIX, "hle daemon bounded-once complete; ");
    }

    // -- daemon_once ------------------------------------------------------------

    #[test]
    fn execute_fails_2730_without_once_flag() {
        let r = daemon_once(false, &example_workflow(), &temp_ledger("no-once"), false);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2730")));
    }

    #[test]
    fn execute_error_message_contains_one_shotted_string() {
        let r = daemon_once(false, &example_workflow(), &temp_ledger("shotted"), false);
        assert!(r
            .err()
            .map_or(false, |e| e.to_string().contains("one shotted")));
    }

    #[test]
    fn execute_succeeds_with_once_flag() {
        let ledger = temp_ledger("ok");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_ok_contains_bounded_once_complete_prefix() {
        let ledger = temp_ledger("prefix");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        assert!(r.map_or_else(|_| false, |s| s.starts_with(DAEMON_ONCE_PREFIX)));
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_ok_contains_inner_run_result() {
        let ledger = temp_ledger("inner");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        // The inner hle run output should appear after the prefix.
        assert!(r.map_or_else(|_| false, |s| s.contains("hle run verdict=")));
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_wraps_inner_error_with_2731() {
        // Non-existent workflow triggers inner 2710 -> wrapped as 2731.
        let missing = PathBuf::from("/tmp/hle-daemon-missing-99999.toml");
        let ledger = temp_ledger("inner-err");
        let r = daemon_once(true, &missing, &ledger, false);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2731")));
    }

    #[test]
    fn execute_inner_error_message_preserved_in_2731() {
        let missing = PathBuf::from("/tmp/hle-daemon-missing-12345.toml");
        let ledger = temp_ledger("preserve");
        let r = daemon_once(true, &missing, &ledger, false);
        // Inner 2710 message must survive wrapping.
        assert!(r.err().map_or(false, |e| e.to_string().contains("2710")));
    }

    #[test]
    fn execute_json_mode_output_contains_schema_field() {
        let ledger = temp_ledger("json");
        let r = daemon_once(true, &example_workflow(), &ledger, true);
        assert!(r.map_or_else(|_| false, |s| s.contains("hle.run.summary.v1")));
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_human_mode_output_does_not_contain_schema_field() {
        let ledger = temp_ledger("human");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        assert!(r.map_or_else(|_| false, |s| !s.contains("hle.run.summary.v1")));
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_once_false_errors_before_workflow_path_check() {
        // Even with a non-existent workflow, once=false errors first.
        let missing = PathBuf::from("/tmp/hle-daemon-never-read.toml");
        let r = daemon_once(false, &missing, &temp_ledger("order"), false);
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("2730"), "expected 2730, got: {msg}");
    }

    #[test]
    fn execute_once_true_proceeds_to_workflow_check() {
        // once=true but missing workflow -> 2731 (inner 2710), not 2730.
        let missing = PathBuf::from("/tmp/hle-daemon-missing-77777.toml");
        let r = daemon_once(true, &missing, &temp_ledger("proceeds"), false);
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(!msg.contains("2730"), "should not be 2730: {msg}");
        assert!(msg.contains("2731"), "expected 2731: {msg}");
    }

    // -- guard_once: additional invariants -------------------------------------

    #[test]
    fn guard_once_true_repeated_calls_always_ok() {
        assert!(guard_once(true).is_ok());
        assert!(guard_once(true).is_ok());
    }

    #[test]
    fn guard_once_false_repeated_calls_always_err() {
        assert!(guard_once(false).is_err());
        assert!(guard_once(false).is_err());
    }

    #[test]
    fn guard_once_err_message_contains_bounded() {
        let e = guard_once(false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(e.contains("bounded"), "expected 'bounded' in: {e}");
    }

    #[test]
    fn guard_once_err_message_contains_m0() {
        let e = guard_once(false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(e.contains("M0"), "expected 'M0' in: {e}");
    }

    // -- wrap_result: edge cases ------------------------------------------------

    #[test]
    fn wrap_result_empty_inner_still_has_prefix() {
        let out = wrap_result("");
        assert_eq!(out, DAEMON_ONCE_PREFIX);
    }

    #[test]
    fn wrap_result_long_inner_fully_preserved() {
        let inner = "x".repeat(1000);
        let out = wrap_result(&inner);
        assert!(out.ends_with(&inner));
    }

    #[test]
    fn wrap_result_prefix_followed_immediately_by_inner() {
        let out = wrap_result("abc");
        assert_eq!(out, format!("{DAEMON_ONCE_PREFIX}abc"));
    }

    // -- daemon_once: ledger path passthrough -----------------------------------

    #[test]
    fn execute_ok_output_contains_ledger_path() {
        let ledger = temp_ledger("ledger-path");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        let ledger_str = ledger.display().to_string();
        assert!(r.map_or_else(|_| false, |s| s.contains(&ledger_str)));
        let _ = std::fs::remove_file(&ledger);
    }

    #[test]
    fn execute_ok_output_contains_verdict_field() {
        let ledger = temp_ledger("verdict-field");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        assert!(r.map_or_else(|_| false, |s| s.contains("verdict=")));
        let _ = std::fs::remove_file(&ledger);
    }

    // -- daemon_once: error codes are independent of json flag -----------------

    #[test]
    fn execute_fails_2730_without_once_regardless_of_json_true() {
        let r = daemon_once(
            false,
            &example_workflow(),
            &temp_ledger("no-once-json"),
            true,
        );
        assert!(r.err().map_or(false, |e| e.to_string().contains("2730")));
    }

    #[test]
    fn execute_fails_2731_json_mode_wraps_inner_error() {
        let missing = PathBuf::from("/tmp/hle-daemon-missing-json-99991.toml");
        let r = daemon_once(true, &missing, &temp_ledger("json-err"), true);
        assert!(r.err().map_or(false, |e| e.to_string().contains("2731")));
    }

    // -- DAEMON_ONCE_PREFIX const -----------------------------------------------

    #[test]
    fn daemon_once_prefix_ends_with_space_after_semicolon() {
        // The prefix must separate properly from the inner run string.
        assert!(DAEMON_ONCE_PREFIX.ends_with(' '));
    }

    #[test]
    fn daemon_once_prefix_contains_daemon() {
        assert!(DAEMON_ONCE_PREFIX.contains("daemon"));
    }

    #[test]
    fn daemon_once_prefix_contains_bounded_once() {
        assert!(DAEMON_ONCE_PREFIX.contains("bounded-once"));
    }

    #[test]
    fn daemon_once_prefix_contains_complete() {
        assert!(DAEMON_ONCE_PREFIX.contains("complete"));
    }

    // -- Idempotency of successful run -----------------------------------------

    #[test]
    fn execute_success_output_is_string_not_empty() {
        let ledger = temp_ledger("nonempty");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        if let Ok(s) = r {
            assert!(!s.is_empty());
        }
        let _ = std::fs::remove_file(temp_ledger("nonempty"));
    }

    // -- 2731 wrapping preserves inner 2710 text --------------------------------

    #[test]
    fn execute_inner_2710_text_inside_2731_message() {
        let missing = PathBuf::from("/tmp/hle-daemon-inner-2710-55555.toml");
        let r = daemon_once(true, &missing, &temp_ledger("inner-2710"), false);
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        // Both codes appear: 2731 wraps 2710.
        assert!(
            msg.contains("2731") && msg.contains("2710"),
            "expected both 2731 and 2710 in: {msg}"
        );
    }

    // -- once=false short-circuits workflow file read ---------------------------

    #[test]
    fn execute_once_false_does_not_require_valid_workflow_file() {
        // Non-existent workflow but once=false: should get 2730, not 2731/2710.
        let bogus = PathBuf::from("/tmp/hle-bogus-workflow-12399.toml");
        let r = daemon_once(false, &bogus, &temp_ledger("short-circuit"), false);
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("2730"), "expected 2730: {msg}");
    }

    // -- json output structure when once=true ----------------------------------

    #[test]
    fn execute_json_mode_output_is_not_empty() {
        let ledger = temp_ledger("json-nonempty");
        let r = daemon_once(true, &example_workflow(), &ledger, true);
        if let Ok(s) = r {
            assert!(!s.is_empty());
        }
        let _ = std::fs::remove_file(temp_ledger("json-nonempty"));
    }

    #[test]
    fn execute_json_mode_prefix_still_present() {
        let ledger = temp_ledger("json-prefix");
        let r = daemon_once(true, &example_workflow(), &ledger, true);
        assert!(r.map_or_else(|_| false, |s| s.starts_with(DAEMON_ONCE_PREFIX)));
        let _ = std::fs::remove_file(temp_ledger("json-prefix"));
    }

    // -- Assertion: daemon_once signature matches public contract ---------------

    #[test]
    fn fn_signature_takes_bool_path_path_bool() {
        // This test exercises all four parameter types without calling fs.
        let r = daemon_once(
            false,
            &PathBuf::from("/tmp/never"),
            &PathBuf::from("/tmp/never2"),
            false,
        );
        // once=false -> always 2730 error
        assert!(r.is_err());
    }

    // -- DAEMON_ONCE_PREFIX is a valid UTF-8 string ----------------------------

    #[test]
    fn daemon_once_prefix_is_valid_utf8() {
        assert!(std::str::from_utf8(DAEMON_ONCE_PREFIX.as_bytes()).is_ok());
    }

    // -- wrap_result with whitespace-only inner --------------------------------

    #[test]
    fn wrap_result_whitespace_inner_preserved() {
        let out = wrap_result("   ");
        assert_eq!(out, format!("{DAEMON_ONCE_PREFIX}   "));
    }

    // -- once=false error code is always 2730, not 2731 -----------------------

    #[test]
    fn error_code_2730_not_2731_on_missing_once() {
        let r = daemon_once(
            false,
            &example_workflow(),
            &temp_ledger("code-check"),
            false,
        );
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(!msg.contains("2731"), "must not contain 2731: {msg}");
    }

    // -- once=true + valid workflow: output does not contain 2730 or 2731 ------

    #[test]
    fn execute_ok_no_error_codes_in_output() {
        let ledger = temp_ledger("no-error-codes");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        if let Ok(s) = r {
            assert!(
                !s.contains("2730") && !s.contains("2731"),
                "unexpected code: {s}"
            );
        }
        let _ = std::fs::remove_file(temp_ledger("no-error-codes"));
    }

    // -- guard_once result is Ok(()) not Ok(something_else) -------------------

    #[test]
    fn guard_once_true_returns_unit() {
        let result = guard_once(true);
        assert_eq!(result, Ok(()));
    }

    // -- 2731 message starts with the code prefix -----------------------------

    #[test]
    fn execute_inner_error_2731_starts_with_prefix() {
        let missing = PathBuf::from("/tmp/hle-daemon-missing-prefix-33333.toml");
        let r = daemon_once(true, &missing, &temp_ledger("prefix-2731"), false);
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.starts_with("[2731]"),
            "expected msg to start with [2731]: {msg}"
        );
    }

    // -- wrap_result: result length = prefix + inner length -------------------

    #[test]
    fn wrap_result_length_equals_prefix_plus_inner() {
        let inner = "test-inner-string";
        let out = wrap_result(inner);
        assert_eq!(out.len(), DAEMON_ONCE_PREFIX.len() + inner.len());
    }

    // -- daemon_once ok output length is bounded under 4 KB -------------------

    #[test]
    fn execute_ok_output_bounded_under_4kb() {
        let ledger = temp_ledger("4kb-bound");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        if let Ok(s) = r {
            assert!(s.len() <= 4096, "output too long: {} bytes", s.len());
        }
        let _ = std::fs::remove_file(temp_ledger("4kb-bound"));
    }

    // -- json mode: prefix + JSON object is well-formed string -----------------

    #[test]
    fn execute_json_mode_inner_json_starts_with_brace() {
        let ledger = temp_ledger("json-brace");
        let r = daemon_once(true, &example_workflow(), &ledger, true);
        if let Ok(s) = r {
            // After stripping the prefix, the JSON object should start with '{'.
            let inner = s.strip_prefix(DAEMON_ONCE_PREFIX).unwrap_or("");
            assert!(
                inner.starts_with('{'),
                "inner should start with '{{': {inner}"
            );
        }
        let _ = std::fs::remove_file(temp_ledger("json-brace"));
    }

    // -- once=true error does not contain the verbatim one-shotted phrase ------

    #[test]
    fn execute_once_true_success_omits_one_shotted_phrase() {
        let ledger = temp_ledger("no-phrase");
        let r = daemon_once(true, &example_workflow(), &ledger, false);
        if let Ok(s) = r {
            assert!(
                !s.contains("one shotted"),
                "success output should not contain 'one shotted': {s}"
            );
        }
        let _ = std::fs::remove_file(temp_ledger("no-phrase"));
    }
}
