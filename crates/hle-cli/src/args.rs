//! `M046` — `cli_args`: typed hand-rolled CLI argument parser.
//!
//! Single gateway from raw `&[String]` argv to `ParsedCommand`. No external
//! deps; algorithm mirrors the `windows(2)` pattern in `main.rs`.
//!
//! Error codes: 2700-2703. Scan subcommand adds 2760-2762.

#![forbid(unsafe_code)]
// Stub module: public items are not yet called from main.rs.
#![allow(dead_code)]

use std::fmt;
use std::path::PathBuf;
use substrate_types::HleError;

use crate::scan::SeverityFilter;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Discriminated union of all valid top-level commands.
#[derive(Debug, PartialEq)]
pub enum ParsedCommand {
    Run(RunArgs),
    Verify(VerifyArgs),
    DaemonOnce(DaemonOnceArgs),
    Status(StatusArgs),
    Scan(ScanArgs),
    Audit(AuditArgs),
    Taxonomy(TaxonomyArgs),
    Help,
    Version,
}

impl fmt::Display for ParsedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Run(_) => f.write_str("Run"),
            Self::Verify(_) => f.write_str("Verify"),
            Self::DaemonOnce(_) => f.write_str("DaemonOnce"),
            Self::Status(_) => f.write_str("Status"),
            Self::Scan(_) => f.write_str("Scan"),
            Self::Audit(_) => f.write_str("Audit"),
            Self::Taxonomy(_) => f.write_str("Taxonomy"),
            Self::Help => f.write_str("Help"),
            Self::Version => f.write_str("Version"),
        }
    }
}

/// Validated arguments for `hle run`.
#[derive(Debug, PartialEq)]
pub struct RunArgs {
    /// Path to the workflow TOML file. Required.
    pub workflow_path: PathBuf,
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
}

/// Validated arguments for `hle verify`.
#[derive(Debug, PartialEq)]
pub struct VerifyArgs {
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
    /// Reject receipts that lack a `receipt_sha256` field (`[E2723]`). Optional.
    pub strict_sha: bool,
}

/// Validated arguments for `hle daemon --once`.
#[derive(Debug, PartialEq)]
pub struct DaemonOnceArgs {
    /// `--once` flag was present.
    pub once: bool,
    /// Path to the workflow TOML file. Required.
    pub workflow_path: PathBuf,
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
}

/// Validated arguments for `hle status`.
#[derive(Debug, PartialEq)]
pub struct StatusArgs {
    /// Emit machine-readable JSON (`hle.status.v1` schema). Optional.
    pub json: bool,
}

/// Validated arguments for `hle scan`.
#[derive(Debug, PartialEq)]
pub struct ScanArgs {
    /// Root directory to scan. Required.
    pub root: PathBuf,
    /// Minimum severity to report. Default: `Low`.
    pub severity_min: SeverityFilter,
    /// Emit machine-readable JSON (`hle.scan.v1` schema). Optional.
    pub json: bool,
}

/// Validated arguments for `hle audit`.
#[derive(Debug, PartialEq)]
pub struct AuditArgs {
    /// Path to the source file to audit (JSONL, JSON, or .md receipt). Required.
    pub source: PathBuf,
    /// Reject unrecognized file extensions instead of falling back to JSONL. Optional.
    pub strict: bool,
    /// Emit machine-readable JSON (`hle.audit.v1` schema). Optional.
    pub json: bool,
}

/// Validated arguments for `hle taxonomy`.
#[derive(Debug, PartialEq)]
pub struct TaxonomyArgs {
    /// Root directory to scan. Required.
    pub root: PathBuf,
    /// Emit machine-readable JSON (`hle.taxonomy.v1` schema). Optional.
    pub json: bool,
}

/// Structured parse error (codes 2700-2703).
#[derive(Debug, PartialEq)]
pub enum ArgsError {
    /// An unrecognized flag was encountered. Code 2700.
    UnknownFlag { flag: String },
    /// A required flag was not present. Code 2701.
    MissingFlag { flag: &'static str },
    /// A flag was present but its value token was absent. Code 2702.
    MissingValue { flag: &'static str },
    /// Two mutually exclusive flags were both present. Code 2703.
    Conflict {
        flag_a: &'static str,
        flag_b: &'static str,
    },
}

impl ArgsError {
    /// Error code: 2700-2703.
    #[must_use]
    pub fn code(&self) -> u16 {
        match self {
            Self::UnknownFlag { .. } => 2700,
            Self::MissingFlag { .. } => 2701,
            Self::MissingValue { .. } => 2702,
            Self::Conflict { .. } => 2703,
        }
    }
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag { flag } => {
                write!(f, "[2700] unknown flag: {flag}")
            }
            Self::MissingFlag { flag } => {
                write!(f, "[2701] missing required flag {flag}")
            }
            Self::MissingValue { flag } => {
                write!(f, "[2702] flag {flag} present but value missing")
            }
            Self::Conflict { flag_a, flag_b } => {
                write!(f, "[2703] conflicting flags: {flag_a} and {flag_b}")
            }
        }
    }
}

impl From<ArgsError> for HleError {
    fn from(err: ArgsError) -> Self {
        Self::new(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse raw argv (caller skips argv\[0\]).
///
/// Returns `Ok(ParsedCommand::Help)` for empty argv or help tokens.
/// Returns `Ok(ParsedCommand::Version)` for version tokens.
/// Returns `Err(ArgsError)` on unknown flags, missing required flags, or
/// missing values.
pub fn parse(args: &[String]) -> Result<ParsedCommand, ArgsError> {
    let Some(command) = args.first() else {
        return Ok(ParsedCommand::Help);
    };
    match command.as_str() {
        "help" | "--help" => Ok(ParsedCommand::Help),
        "version" | "--version" => Ok(ParsedCommand::Version),
        "run" => parse_run(&args[1..]).map(ParsedCommand::Run),
        "verify" => parse_verify(&args[1..]).map(ParsedCommand::Verify),
        "daemon" => parse_daemon(&args[1..]).map(ParsedCommand::DaemonOnce),
        "status" => parse_status(&args[1..]).map(ParsedCommand::Status),
        "scan" => parse_scan(&args[1..]).map(ParsedCommand::Scan),
        "audit" => parse_audit(&args[1..]).map(ParsedCommand::Audit),
        "taxonomy" => parse_taxonomy(&args[1..]).map(ParsedCommand::Taxonomy),
        other => Err(ArgsError::UnknownFlag {
            flag: other.to_owned(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Private sub-parsers
// ---------------------------------------------------------------------------

fn parse_run(args: &[String]) -> Result<RunArgs, ArgsError> {
    const KNOWN: &[&str] = &["--workflow", "--ledger", "--json"];
    reject_unknown(args, KNOWN)?;
    let workflow_path = flag_path(args, "--workflow")?;
    let ledger_path = flag_path(args, "--ledger")?;
    let json = flag_bool(args, "--json");
    Ok(RunArgs {
        workflow_path,
        ledger_path,
        json,
    })
}

fn parse_verify(args: &[String]) -> Result<VerifyArgs, ArgsError> {
    const KNOWN: &[&str] = &["--ledger", "--json", "--strict-sha"];
    reject_unknown(args, KNOWN)?;
    let ledger_path = flag_path(args, "--ledger")?;
    let json = flag_bool(args, "--json");
    let strict_sha = flag_bool(args, "--strict-sha");
    Ok(VerifyArgs {
        ledger_path,
        json,
        strict_sha,
    })
}

fn parse_daemon(args: &[String]) -> Result<DaemonOnceArgs, ArgsError> {
    const KNOWN: &[&str] = &["--once", "--workflow", "--ledger", "--json"];
    reject_unknown(args, KNOWN)?;
    // --once must be present before we attempt to read --workflow / --ledger.
    if !args.iter().any(|a| a == "--once") {
        return Err(ArgsError::MissingFlag { flag: "--once" });
    }
    let workflow_path = flag_path(args, "--workflow")?;
    let ledger_path = flag_path(args, "--ledger")?;
    let json = flag_bool(args, "--json");
    Ok(DaemonOnceArgs {
        once: true,
        workflow_path,
        ledger_path,
        json,
    })
}

fn parse_status(args: &[String]) -> Result<StatusArgs, ArgsError> {
    const KNOWN: &[&str] = &["--json"];
    reject_unknown(args, KNOWN)?;
    let json = flag_bool(args, "--json");
    Ok(StatusArgs { json })
}

fn parse_scan(args: &[String]) -> Result<ScanArgs, ArgsError> {
    const KNOWN: &[&str] = &["--root", "--severity", "--json"];
    reject_unknown(args, KNOWN)?;
    let root = flag_path(args, "--root")?;
    let severity_min = match flag_string_opt(args, "--severity") {
        Some(s) => SeverityFilter::from_str(&s).ok_or(ArgsError::UnknownFlag {
            flag: format!("--severity {s}"),
        })?,
        None => SeverityFilter::Low,
    };
    let json = flag_bool(args, "--json");
    Ok(ScanArgs {
        root,
        severity_min,
        json,
    })
}

fn parse_audit(args: &[String]) -> Result<AuditArgs, ArgsError> {
    const KNOWN: &[&str] = &["--source", "--strict", "--json"];
    reject_unknown(args, KNOWN)?;
    let source = flag_path(args, "--source")?;
    let strict = flag_bool(args, "--strict");
    let json = flag_bool(args, "--json");
    Ok(AuditArgs {
        source,
        strict,
        json,
    })
}

fn parse_taxonomy(args: &[String]) -> Result<TaxonomyArgs, ArgsError> {
    const KNOWN: &[&str] = &["--root", "--json"];
    reject_unknown(args, KNOWN)?;
    let root = flag_path(args, "--root")?;
    let json = flag_bool(args, "--json");
    Ok(TaxonomyArgs { root, json })
}

// ---------------------------------------------------------------------------
// Flag helpers
// ---------------------------------------------------------------------------

/// Scan for `--name value` pair; return `PathBuf(value)`.
fn flag_path(args: &[String], name: &'static str) -> Result<PathBuf, ArgsError> {
    for pair in args.windows(2) {
        if pair[0] == name {
            // Value must not start with `--` (would be another flag, not a path).
            if pair[1].starts_with("--") {
                return Err(ArgsError::MissingValue { flag: name });
            }
            return Ok(PathBuf::from(&pair[1]));
        }
    }
    // Flag was not followed by anything (last arg or absent entirely).
    // Check if flag appears as last token with no following value.
    if args.last().map(String::as_str) == Some(name) {
        return Err(ArgsError::MissingValue { flag: name });
    }
    Err(ArgsError::MissingFlag { flag: name })
}

/// Presence-only boolean scan.
fn flag_bool(args: &[String], name: &'static str) -> bool {
    args.iter().any(|a| a == name)
}

/// Optional `--name value` string pair; returns `None` when absent.
fn flag_string_opt(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find_map(|pair| {
        if pair[0] == name && !pair[1].starts_with("--") {
            Some(pair[1].clone())
        } else {
            None
        }
    })
}

/// Reject any `--*` token not in `known`.
fn reject_unknown(args: &[String], known: &[&str]) -> Result<(), ArgsError> {
    for arg in args {
        if arg.starts_with("--") && !known.contains(&arg.as_str()) {
            return Err(ArgsError::UnknownFlag { flag: arg.clone() });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_owned()
    }
    fn sv(vs: &[&str]) -> Vec<String> {
        vs.iter().map(|v| v.to_string()).collect()
    }

    // -- parse top-level routing ------------------------------------------------

    #[test]
    fn parse_empty_returns_help() {
        assert_eq!(parse(&[]), Ok(ParsedCommand::Help));
    }

    #[test]
    fn parse_help_flag_returns_help() {
        assert_eq!(parse(&sv(&["--help"])), Ok(ParsedCommand::Help));
    }

    #[test]
    fn parse_help_word_returns_help() {
        assert_eq!(parse(&sv(&["help"])), Ok(ParsedCommand::Help));
    }

    #[test]
    fn parse_version_flag_returns_version() {
        assert_eq!(parse(&sv(&["--version"])), Ok(ParsedCommand::Version));
    }

    #[test]
    fn parse_version_word_returns_version() {
        assert_eq!(parse(&sv(&["version"])), Ok(ParsedCommand::Version));
    }

    #[test]
    fn parse_unknown_command_errors_2700() {
        let r = parse(&sv(&["bogus"]));
        assert!(r.is_err());
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_unknown_command_error_names_command() {
        let r = parse(&sv(&["bogus"]));
        assert!(r.err().map_or(false, |e| e.to_string().contains("bogus")));
    }

    // -- run sub-parser ---------------------------------------------------------

    #[test]
    fn parse_run_valid_returns_run_args() {
        let r = parse(&sv(&["run", "--workflow", "w.toml", "--ledger", "l.jsonl"]));
        assert!(matches!(r, Ok(ParsedCommand::Run(_))));
    }

    #[test]
    fn parse_run_workflow_path_correct() {
        let r = parse(&sv(&["run", "--workflow", "w.toml", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.workflow_path, std::path::PathBuf::from("w.toml"));
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_run_ledger_path_correct() {
        let r = parse(&sv(&["run", "--workflow", "w.toml", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.ledger_path, std::path::PathBuf::from("l.jsonl"));
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_run_json_false_by_default() {
        let r = parse(&sv(&["run", "--workflow", "w.toml", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert!(!a.json);
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_run_with_json_flag() {
        let r = parse(&sv(&[
            "run",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
            "--json",
        ]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert!(a.json);
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_run_missing_workflow_errors_2701() {
        let r = parse(&sv(&["run", "--ledger", "l.jsonl"]));
        assert!(r.is_err());
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_run_missing_ledger_errors_2701() {
        let r = parse(&sv(&["run", "--workflow", "w.toml"]));
        assert!(r.is_err());
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_run_unknown_flag_errors_2700() {
        let r = parse(&sv(&[
            "run",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
            "--bogus",
        ]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_run_unknown_flag_names_flag() {
        let r = parse(&sv(&[
            "run",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
            "--bogus",
        ]));
        assert!(r.err().map_or(false, |e| e.to_string().contains("--bogus")));
    }

    #[test]
    fn parse_run_workflow_missing_value_errors_2702() {
        // --workflow is the last token: missing value
        let r = parse(&sv(&["run", "--ledger", "l.jsonl", "--workflow"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2702));
    }

    // -- verify sub-parser ------------------------------------------------------

    #[test]
    fn parse_verify_valid_returns_verify_args() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl"]));
        assert!(matches!(r, Ok(ParsedCommand::Verify(_))));
    }

    #[test]
    fn parse_verify_ledger_path_correct() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert_eq!(a.ledger_path, std::path::PathBuf::from("l.jsonl"));
        } else {
            panic!("expected Verify");
        }
    }

    #[test]
    fn parse_verify_strict_sha_false_by_default() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert!(!a.strict_sha);
        } else {
            panic!("expected Verify");
        }
    }

    #[test]
    fn parse_verify_strict_sha_true_when_flag_present() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl", "--strict-sha"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert!(a.strict_sha);
        } else {
            panic!("expected Verify");
        }
    }

    #[test]
    fn parse_verify_strict_sha_with_json_flag() {
        let r = parse(&sv(&[
            "verify",
            "--ledger",
            "l.jsonl",
            "--strict-sha",
            "--json",
        ]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert!(a.strict_sha);
            assert!(a.json);
        } else {
            panic!("expected Verify");
        }
    }

    #[test]
    fn parse_verify_strict_sha_unknown_flag_still_rejected() {
        let r = parse(&sv(&[
            "verify",
            "--ledger",
            "l.jsonl",
            "--strict-sha",
            "--bogus",
        ]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_verify_missing_ledger_errors_2701() {
        let r = parse(&sv(&["verify"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_verify_unknown_flag_errors_2700() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl", "--bogus"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_verify_with_json_flag() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl", "--json"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert!(a.json);
        } else {
            panic!("expected Verify");
        }
    }

    #[test]
    fn parse_verify_json_false_by_default() {
        let r = parse(&sv(&["verify", "--ledger", "l.jsonl"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert!(!a.json);
        } else {
            panic!("expected Verify");
        }
    }

    // -- daemon sub-parser ------------------------------------------------------

    #[test]
    fn parse_daemon_valid_with_once_returns_daemon_args() {
        let r = parse(&sv(&[
            "daemon",
            "--once",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        assert!(matches!(r, Ok(ParsedCommand::DaemonOnce(_))));
    }

    #[test]
    fn parse_daemon_once_field_true() {
        let r = parse(&sv(&[
            "daemon",
            "--once",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        if let Ok(ParsedCommand::DaemonOnce(a)) = r {
            assert!(a.once);
        } else {
            panic!("expected DaemonOnce");
        }
    }

    #[test]
    fn parse_daemon_missing_once_errors_2701() {
        let r = parse(&sv(&[
            "daemon",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_daemon_missing_once_error_names_flag() {
        let r = parse(&sv(&[
            "daemon",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        assert!(r.err().map_or(false, |e| e.to_string().contains("--once")));
    }

    #[test]
    fn parse_daemon_missing_workflow_errors_2701() {
        let r = parse(&sv(&["daemon", "--once", "--ledger", "l.jsonl"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_daemon_missing_ledger_errors_2701() {
        let r = parse(&sv(&["daemon", "--once", "--workflow", "w.toml"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn parse_daemon_unknown_flag_errors_2700() {
        let r = parse(&sv(&[
            "daemon",
            "--once",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
            "--bogus",
        ]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_daemon_with_json_flag() {
        let r = parse(&sv(&[
            "daemon",
            "--once",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
            "--json",
        ]));
        if let Ok(ParsedCommand::DaemonOnce(a)) = r {
            assert!(a.json);
        } else {
            panic!("expected DaemonOnce");
        }
    }

    // -- status sub-parser ------------------------------------------------------

    #[test]
    fn parse_status_no_flags_returns_status_args() {
        let r = parse(&sv(&["status"]));
        assert!(matches!(r, Ok(ParsedCommand::Status(_))));
    }

    #[test]
    fn parse_status_json_false_by_default() {
        let r = parse(&sv(&["status"]));
        if let Ok(ParsedCommand::Status(a)) = r {
            assert!(!a.json);
        } else {
            panic!("expected Status");
        }
    }

    #[test]
    fn parse_status_with_json_flag() {
        let r = parse(&sv(&["status", "--json"]));
        if let Ok(ParsedCommand::Status(a)) = r {
            assert!(a.json);
        } else {
            panic!("expected Status");
        }
    }

    #[test]
    fn parse_status_unknown_flag_errors_2700() {
        let r = parse(&sv(&["status", "--bogus"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    // -- ArgsError display / code -----------------------------------------------

    #[test]
    fn args_error_unknown_flag_display_contains_flag_name() {
        let e = ArgsError::UnknownFlag { flag: s("--xyz") };
        assert!(e.to_string().contains("--xyz"));
    }

    #[test]
    fn args_error_missing_flag_display_contains_flag_name() {
        let e = ArgsError::MissingFlag { flag: "--abc" };
        assert!(e.to_string().contains("--abc"));
    }

    #[test]
    fn args_error_missing_value_display_contains_flag_name() {
        let e = ArgsError::MissingValue { flag: "--abc" };
        assert!(e.to_string().contains("--abc"));
    }

    #[test]
    fn args_error_conflict_display_contains_both_flag_names() {
        let e = ArgsError::Conflict {
            flag_a: "--aaa",
            flag_b: "--bbb",
        };
        let s = e.to_string();
        assert!(s.contains("--aaa") && s.contains("--bbb"));
    }

    #[test]
    fn args_error_code_unknown_flag_is_2700() {
        assert_eq!(ArgsError::UnknownFlag { flag: s("x") }.code(), 2700);
    }

    #[test]
    fn args_error_code_missing_flag_is_2701() {
        assert_eq!(ArgsError::MissingFlag { flag: "--x" }.code(), 2701);
    }

    #[test]
    fn args_error_code_missing_value_is_2702() {
        assert_eq!(ArgsError::MissingValue { flag: "--x" }.code(), 2702);
    }

    #[test]
    fn args_error_code_conflict_is_2703() {
        assert_eq!(
            ArgsError::Conflict {
                flag_a: "--a",
                flag_b: "--b"
            }
            .code(),
            2703
        );
    }

    #[test]
    fn args_error_into_hle_error_preserves_code_string() {
        let e = ArgsError::UnknownFlag { flag: s("--bad") };
        let hle: HleError = e.into();
        assert!(hle.to_string().contains("2700"));
    }

    // -- ParsedCommand Display --------------------------------------------------

    #[test]
    fn parsed_command_display_run() {
        assert_eq!(
            ParsedCommand::Run(RunArgs {
                workflow_path: PathBuf::from("w"),
                ledger_path: PathBuf::from("l"),
                json: false
            })
            .to_string(),
            "Run"
        );
    }

    #[test]
    fn parsed_command_display_help() {
        assert_eq!(ParsedCommand::Help.to_string(), "Help");
    }

    #[test]
    fn parsed_command_display_version() {
        assert_eq!(ParsedCommand::Version.to_string(), "Version");
    }

    // -- flag_path edge cases ---------------------------------------------------

    #[test]
    fn flag_path_finds_value() {
        let args = sv(&["--workflow", "demo.toml"]);
        let r = flag_path(&args, "--workflow");
        assert_eq!(r, Ok(PathBuf::from("demo.toml")));
    }

    #[test]
    fn flag_path_errors_when_absent() {
        let r = flag_path(&[], "--workflow");
        assert_eq!(r.err().map(|e| e.code()), Some(2701));
    }

    #[test]
    fn flag_path_errors_missing_value_when_last() {
        let args = sv(&["--workflow"]);
        let r = flag_path(&args, "--workflow");
        assert_eq!(r.err().map(|e| e.code()), Some(2702));
    }

    #[test]
    fn flag_path_errors_missing_value_when_next_is_flag() {
        let args = sv(&["--workflow", "--ledger"]);
        let r = flag_path(&args, "--workflow");
        assert_eq!(r.err().map(|e| e.code()), Some(2702));
    }

    #[test]
    fn flag_bool_true_when_present() {
        let args = sv(&["--json"]);
        assert!(flag_bool(&args, "--json"));
    }

    #[test]
    fn flag_bool_false_when_absent() {
        assert!(!flag_bool(&[], "--json"));
    }

    #[test]
    fn reject_unknown_passes_known_flags() {
        let args = sv(&["--workflow", "w.toml", "--ledger", "l.jsonl"]);
        assert!(reject_unknown(&args, &["--workflow", "--ledger"]).is_ok());
    }

    #[test]
    fn reject_unknown_rejects_unknown_flag() {
        let args = sv(&["--workflow", "w.toml", "--bogus"]);
        let r = reject_unknown(&args, &["--workflow"]);
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn reject_unknown_allows_non_flag_positionals_if_not_dash_dash() {
        // Values that don't start with `--` are not rejected by reject_unknown.
        let args = sv(&["--workflow", "w.toml"]);
        assert!(reject_unknown(&args, &["--workflow"]).is_ok());
    }

    // -- Unicode flag values ----------------------------------------------------

    #[test]
    fn parse_run_unicode_workflow_path_accepted() {
        let r = parse(&sv(&[
            "run",
            "--workflow",
            "wörk/flöw.toml",
            "--ledger",
            "l.jsonl",
        ]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.workflow_path, std::path::PathBuf::from("wörk/flöw.toml"));
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_verify_unicode_ledger_path_accepted() {
        let r = parse(&sv(&["verify", "--ledger", "données/répertoire.jsonl"]));
        if let Ok(ParsedCommand::Verify(a)) = r {
            assert_eq!(
                a.ledger_path,
                std::path::PathBuf::from("données/répertoire.jsonl")
            );
        } else {
            panic!("expected Verify");
        }
    }

    // -- Subcommand ordering edge cases -----------------------------------------

    #[test]
    fn parse_run_flags_reversed_order_still_accepted() {
        // --ledger before --workflow; both must be accepted regardless of order.
        let r = parse(&sv(&["run", "--ledger", "l.jsonl", "--workflow", "w.toml"]));
        assert!(matches!(r, Ok(ParsedCommand::Run(_))));
    }

    #[test]
    fn parse_run_flags_reversed_ledger_path_correct() {
        let r = parse(&sv(&["run", "--ledger", "l.jsonl", "--workflow", "w.toml"]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.ledger_path, std::path::PathBuf::from("l.jsonl"));
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_run_flags_reversed_workflow_path_correct() {
        let r = parse(&sv(&["run", "--ledger", "l.jsonl", "--workflow", "w.toml"]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.workflow_path, std::path::PathBuf::from("w.toml"));
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn parse_daemon_flags_different_order_accepted() {
        // --ledger before --workflow; --once after --workflow.
        let r = parse(&sv(&[
            "daemon",
            "--ledger",
            "l.jsonl",
            "--workflow",
            "w.toml",
            "--once",
        ]));
        assert!(matches!(r, Ok(ParsedCommand::DaemonOnce(_))));
    }

    // -- --workflow as last arg with no value -----------------------------------

    #[test]
    fn parse_run_ledger_missing_value_when_last() {
        let r = parse(&sv(&["run", "--workflow", "w.toml", "--ledger"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2702));
    }

    // -- Duplicate flags --------------------------------------------------------

    #[test]
    fn parse_run_duplicate_workflow_takes_first_occurrence() {
        // The windows(2) scan finds the first matching flag.
        let r = parse(&sv(&[
            "run",
            "--workflow",
            "first.toml",
            "--workflow",
            "second.toml",
            "--ledger",
            "l.jsonl",
        ]));
        if let Ok(ParsedCommand::Run(a)) = r {
            assert_eq!(a.workflow_path, std::path::PathBuf::from("first.toml"));
        } else {
            panic!("expected Run");
        }
    }

    // -- ArgsError From<HleError> chain -----------------------------------------

    #[test]
    fn args_error_missing_value_code_2702() {
        assert_eq!(ArgsError::MissingValue { flag: "--x" }.code(), 2702);
    }

    #[test]
    fn args_error_conflict_code_2703() {
        assert_eq!(
            ArgsError::Conflict {
                flag_a: "--a",
                flag_b: "--b"
            }
            .code(),
            2703
        );
    }

    // -- ParsedCommand Display completeness ------------------------------------

    #[test]
    fn parsed_command_display_verify() {
        assert_eq!(
            ParsedCommand::Verify(VerifyArgs {
                ledger_path: std::path::PathBuf::from("l"),
                json: false,
                strict_sha: false,
            })
            .to_string(),
            "Verify"
        );
    }

    #[test]
    fn parsed_command_display_daemon_once() {
        assert_eq!(
            ParsedCommand::DaemonOnce(DaemonOnceArgs {
                once: true,
                workflow_path: std::path::PathBuf::from("w"),
                ledger_path: std::path::PathBuf::from("l"),
                json: false,
            })
            .to_string(),
            "DaemonOnce"
        );
    }

    #[test]
    fn parsed_command_display_status() {
        assert_eq!(
            ParsedCommand::Status(StatusArgs { json: false }).to_string(),
            "Status"
        );
    }

    // -- run missing workflow only (not ledger) error specificity ---------------

    #[test]
    fn parse_run_missing_workflow_error_names_workflow_flag() {
        let r = parse(&sv(&["run", "--ledger", "l.jsonl"]));
        assert!(r
            .err()
            .map_or(false, |e| e.to_string().contains("--workflow")));
    }

    #[test]
    fn parse_run_missing_ledger_error_names_ledger_flag() {
        let r = parse(&sv(&["run", "--workflow", "w.toml"]));
        assert!(r
            .err()
            .map_or(false, |e| e.to_string().contains("--ledger")));
    }

    // -- verify missing ledger error specificity --------------------------------

    #[test]
    fn parse_verify_missing_ledger_error_names_ledger_flag() {
        let r = parse(&sv(&["verify"]));
        assert!(r
            .err()
            .map_or(false, |e| e.to_string().contains("--ledger")));
    }

    // -- daemon missing once error specificity -----------------------------------

    #[test]
    fn parse_daemon_missing_once_error_names_once_flag() {
        let r = parse(&sv(&[
            "daemon",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        assert!(r.err().map_or(false, |e| e.to_string().contains("--once")));
    }

    // -- flag_bool absent from empty argv ---------------------------------------

    #[test]
    fn flag_bool_absent_from_empty_args() {
        assert!(!flag_bool(&[], "--json"));
    }

    // -- reject_unknown with no args --------------------------------------------

    #[test]
    fn reject_unknown_empty_args_ok() {
        assert!(reject_unknown(&[], &["--workflow"]).is_ok());
    }

    // -- HleError conversion preserves message ---------------------------------

    #[test]
    fn args_error_hle_conversion_missing_flag_preserves_message() {
        let e = ArgsError::MissingFlag { flag: "--ledger" };
        let hle: HleError = e.into();
        assert!(hle.to_string().contains("--ledger"));
    }

    #[test]
    fn args_error_hle_conversion_conflict_preserves_both_flags() {
        let e = ArgsError::Conflict {
            flag_a: "--aaa",
            flag_b: "--bbb",
        };
        let hle: HleError = e.into();
        let msg = hle.to_string();
        assert!(msg.contains("--aaa") && msg.contains("--bbb"));
    }

    // -- -h and -v short aliases (should be unknown unless added) ---------------

    #[test]
    fn parse_short_help_alias_not_recognized_as_flag() {
        // -h is not in the recognized set; should route to UnknownFlag.
        // (The spec recognizes only --help and help, not -h.)
        let r = parse(&sv(&["-h"]));
        // It is an unknown flag at the top level (code 2700).
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_short_version_alias_not_recognized_as_flag() {
        let r = parse(&sv(&["-v"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    // -- verify: --ledger value is the last positional -------------------------

    #[test]
    fn parse_verify_ledger_missing_value_when_last_token() {
        let r = parse(&sv(&["verify", "--ledger"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2702));
    }

    // -- daemon: json field on DaemonOnceArgs defaults to false ----------------

    #[test]
    fn parse_daemon_json_false_by_default() {
        let r = parse(&sv(&[
            "daemon",
            "--once",
            "--workflow",
            "w.toml",
            "--ledger",
            "l.jsonl",
        ]));
        if let Ok(ParsedCommand::DaemonOnce(a)) = r {
            assert!(!a.json);
        } else {
            panic!("expected DaemonOnce");
        }
    }

    // -- Unknown subcommand near valid subcommand names ------------------------

    #[test]
    fn parse_runs_not_recognized_as_run() {
        let r = parse(&sv(&["runs"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }

    #[test]
    fn parse_daemon_subword_not_recognized() {
        let r = parse(&sv(&["daemons"]));
        assert_eq!(r.err().map(|e| e.code()), Some(2700));
    }
}
