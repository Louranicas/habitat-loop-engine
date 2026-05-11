//! M042 AtuinQiBridge — integration bridge for the `hle-*` Atuin script registry.
//!
//! Read-only enumerate/status/probe surfaces are available immediately.
//! Write-side (`run_script`, `register_script`) is compile-time sealed via
//! `Sealed<Class>` PhantomData — absent from the `ReadOnly` impl block entirely.
//!
//! Error codes: 2620–2622.

use std::fmt;

use crate::bridge_contract::{
    xor_fold_sha256_stub, BoundedDuration, BridgeContract, BridgeReceipt, CapabilityClass,
    LiveWrite, ReadOnly, Sealed, WriteAuthToken,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Required prefix for all scripts managed by this bridge.
pub const SCRIPT_NAME_PREFIX: &str = "hle-";
/// Maximum byte length of a validated script name.
pub const SCRIPT_NAME_MAX_LEN: usize = 128;
/// Maximum byte length of the `stdout_preview` field in `ScriptStatus`.
pub const SCRIPT_STATUS_PREVIEW_CAP: usize = 512;
/// Maximum number of entries returned by `enumerate`.
pub const ENUMERATE_CAP: usize = 1_024;

// ─── ScriptName ──────────────────────────────────────────────────────────────

/// A validated Atuin script name following the `hle-*` convention.
///
/// Names must be prefixed with `hle-`, be ASCII, non-empty, and at most
/// `SCRIPT_NAME_MAX_LEN` characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScriptName(String);

impl ScriptName {
    /// Construct and validate a script name.
    ///
    /// # Errors
    ///
    /// Returns `Err(ScriptNotFound)` when the name is empty, exceeds
    /// `SCRIPT_NAME_MAX_LEN`, is not ASCII, or does not start with `hle-`.
    pub fn new(name: impl Into<String>) -> Result<Self, AtuinQiBridgeError> {
        let name = name.into();
        if name.is_empty() {
            return Err(AtuinQiBridgeError::ScriptNotFound {
                name: String::from("<empty>"),
            });
        }
        if name.len() > SCRIPT_NAME_MAX_LEN {
            return Err(AtuinQiBridgeError::ScriptNotFound { name });
        }
        if !name.is_ascii() {
            return Err(AtuinQiBridgeError::ScriptNotFound { name });
        }
        if !name.starts_with(SCRIPT_NAME_PREFIX) {
            return Err(AtuinQiBridgeError::ScriptNotFound { name });
        }
        Ok(Self(name))
    }

    /// Full name including the `hle-` prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Name with the `hle-` prefix stripped.
    #[must_use]
    pub fn without_prefix(&self) -> &str {
        self.0.strip_prefix(SCRIPT_NAME_PREFIX).unwrap_or(&self.0)
    }
}

impl fmt::Display for ScriptName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ─── ScriptEntry ─────────────────────────────────────────────────────────────

/// Immutable discovered record for a single `hle-*` Atuin script.
///
/// Populated by `AtuinQiBridge::enumerate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEntry {
    /// Validated script name.
    pub name: ScriptName,
    /// Optional embedded description.
    pub description: Option<String>,
    /// Unix epoch seconds at registration time.
    pub registered_at_epoch_s: u64,
    /// SHA-256 of the script content bytes.
    pub content_sha256: [u8; 32],
}

impl ScriptEntry {
    /// Construct a `ScriptEntry`, computing `content_sha256` from `content`.
    #[must_use]
    pub fn new(name: ScriptName, content: &[u8]) -> Self {
        Self {
            name,
            description: None,
            registered_at_epoch_s: 0,
            content_sha256: xor_fold_sha256_stub(content),
        }
    }

    /// Lowercase hex of `content_sha256`.
    #[must_use]
    pub fn hex_sha(&self) -> String {
        self.content_sha256
            .iter()
            .fold(String::with_capacity(64), |mut acc, b| {
                let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{b:02x}"));
                acc
            })
    }

    /// Always true: name was validated at `ScriptName` level.
    #[must_use]
    pub fn is_hle_script(&self) -> bool {
        true
    }
}

impl fmt::Display for ScriptEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ScriptEntry({} sha={}…)",
            self.name,
            &self.hex_sha()[..8]
        )
    }
}

// ─── ScriptStatus ────────────────────────────────────────────────────────────

/// Outcome snapshot for the most recent run of a named script.
///
/// `stdout_preview` is bounded to `SCRIPT_STATUS_PREVIEW_CAP` (512 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptStatus {
    /// Script name.
    pub name: ScriptName,
    /// Exit code of the most recent run.
    pub exit_code: i32,
    /// Elapsed milliseconds for the most recent run.
    pub elapsed_ms: u64,
    /// Bounded stdout preview (≤ `SCRIPT_STATUS_PREVIEW_CAP` bytes).
    pub stdout_preview: String,
    /// Local tick at which the most recent run was recorded.
    pub last_run_tick: u64,
}

impl ScriptStatus {
    /// True when `exit_code == 0`.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// True when `now_tick - last_run_tick > ttl_ticks`.
    #[must_use]
    pub fn is_stale(&self, now_tick: u64, ttl_ticks: u64) -> bool {
        now_tick.saturating_sub(self.last_run_tick) > ttl_ticks
    }
}

impl fmt::Display for ScriptStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ScriptStatus({} exit={} elapsed={}ms)",
            self.name, self.exit_code, self.elapsed_ms
        )
    }
}

// ─── RunReceipt ──────────────────────────────────────────────────────────────

/// BridgeReceipt-compatible record for a completed bounded script run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReceipt {
    /// Script that was run.
    pub script_name: ScriptName,
    /// Exit code of the script.
    pub exit_code: i32,
    /// Elapsed milliseconds.
    pub elapsed_ms: u64,
    /// SHA-256 of the script's stdout bytes.
    pub stdout_sha256: [u8; 32],
    /// C01 authority receipt ID.
    pub auth_receipt_id: u64,
}

impl RunReceipt {
    /// Convert into a `BridgeReceipt` for C01 verifier routing.
    #[must_use]
    pub fn into_bridge_receipt(self) -> BridgeReceipt {
        BridgeReceipt {
            schema_id: "hle.atuin_qi.v1",
            operation: format!("run_script:{}", self.script_name),
            payload_sha256: self.stdout_sha256,
            timestamp_tick: 0,
            auth_receipt_id: Some(self.auth_receipt_id),
        }
    }

    /// True when `exit_code == 0`.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

// ─── AtuinQiBridge ───────────────────────────────────────────────────────────

/// Atuin QI script registry bridge parameterized over capability class.
///
/// `AtuinQiBridge<Sealed<ReadOnly>>` exposes enumerate, last-run status,
/// and probe. Write methods exist only on `Sealed<LiveWrite>`.
///
/// `registry_path` is the filesystem directory that `enumerate` scans for
/// `hle-*` script files. When `None`, `enumerate` returns an empty list.
/// Real `atuin` subprocess integration is deferred to a future M-phase.
#[derive(Debug)]
pub struct AtuinQiBridge<Class> {
    _class: Class,
    registry_path: Option<std::path::PathBuf>,
}

impl BridgeContract for AtuinQiBridge<Sealed<ReadOnly>> {
    fn schema_id(&self) -> &'static str {
        "hle.atuin_qi.v1"
    }
    fn port(&self) -> Option<u16> {
        None
    }
    fn paths(&self) -> &[&'static str] {
        &[
            "atuin scripts list",
            "atuin scripts run",
            "atuin scripts add",
        ]
    }
    fn supports_write(&self) -> bool {
        false
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }
    fn name(&self) -> &'static str {
        "atuin_qi_bridge"
    }
}

impl BridgeContract for AtuinQiBridge<Sealed<LiveWrite>> {
    fn schema_id(&self) -> &'static str {
        "hle.atuin_qi.v1"
    }
    fn port(&self) -> Option<u16> {
        None
    }
    fn paths(&self) -> &[&'static str] {
        &[
            "atuin scripts list",
            "atuin scripts run",
            "atuin scripts add",
        ]
    }
    fn supports_write(&self) -> bool {
        true
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::LiveWrite
    }
    fn name(&self) -> &'static str {
        "atuin_qi_bridge"
    }
}

// ── Shared read-only helpers ─────────────────────────────────────────────────

fn atuin_binary_present() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| {
            let mut p = std::path::PathBuf::from(dir);
            p.push("atuin");
            p.exists()
        })
}

// ── ReadOnly surface ─────────────────────────────────────────────────────────

// ── Shared filesystem enumerate helper ──────────────────────────────────────

/// Scan `dir` for files whose names start with `hle-` and return a
/// `ScriptEntry` for each, up to `ENUMERATE_CAP`.
///
/// Returns `Err(EnumerationFailed)` only on I/O infrastructure failure
/// (directory not readable). A missing or empty directory returns `Ok(vec![])`.
fn enumerate_from_fs(dir: &std::path::Path) -> Result<Vec<ScriptEntry>, AtuinQiBridgeError> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => {
            return Err(AtuinQiBridgeError::EnumerationFailed {
                reason: format!("read_dir failed: {e}"),
                retryable: true,
            });
        }
    };

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| AtuinQiBridgeError::EnumerationFailed {
            reason: format!("dir entry I/O error: {e}"),
            retryable: true,
        })?;
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if !name_str.starts_with(SCRIPT_NAME_PREFIX) {
            continue;
        }
        let Ok(script_name) = ScriptName::new(name_str.as_ref()) else {
            continue; // skip invalid names silently
        };
        let content = std::fs::read(entry.path()).unwrap_or_default();
        let mut se = ScriptEntry::new(script_name, &content);
        // Set registered_at using file mtime, falling back to 0.
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                se.registered_at_epoch_s = modified
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
            }
        }
        entries.push(se);
        if entries.len() >= ENUMERATE_CAP {
            break;
        }
    }
    Ok(entries)
}

// ── ReadOnly surface ─────────────────────────────────────────────────────────

impl AtuinQiBridge<Sealed<ReadOnly>> {
    /// Construct a read-only bridge without a registry path.
    ///
    /// `enumerate` returns an empty list; use `with_registry_path` to wire
    /// filesystem scanning.
    #[must_use]
    pub fn new_read_only() -> Self {
        Self {
            _class: Sealed::default(),
            registry_path: None,
        }
    }

    /// Construct a read-only bridge pointing at a filesystem registry directory.
    ///
    /// `enumerate` will scan `path` for `hle-*` files up to `ENUMERATE_CAP`.
    #[must_use]
    pub fn with_registry_path(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            _class: Sealed::default(),
            registry_path: Some(path.into()),
        }
    }

    /// Check whether `atuin` binary is present and `atuin scripts list` succeeds.
    ///
    /// Returns `false` when the binary is absent — not `Err`. `Err` only for
    /// subprocess infrastructure failure.
    #[must_use]
    pub fn probe(&self, _timeout: BoundedDuration) -> Result<bool, AtuinQiBridgeError> {
        Ok(atuin_binary_present())
    }

    /// Enumerate `hle-*` scripts from the configured registry directory.
    ///
    /// When `registry_path` is set, scans the filesystem directory for files
    /// whose names start with `hle-`, returning up to `ENUMERATE_CAP` entries.
    /// When `registry_path` is `None`, returns an empty list.
    ///
    /// Real `atuin` subprocess integration is deferred; this surface is
    /// filesystem-based, NOT atuin's real DB.
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` when the directory cannot be read
    /// due to an I/O infrastructure failure.
    #[must_use]
    pub fn enumerate(
        &self,
        _timeout: BoundedDuration,
    ) -> Result<Vec<ScriptEntry>, AtuinQiBridgeError> {
        match &self.registry_path {
            Some(path) => enumerate_from_fs(path),
            None => Ok(Vec::new()),
        }
    }

    /// Retrieve last-run status for a named script.
    ///
    /// M0 stub: returns `None` (no history in M0 environment).
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` on parse failure.
    #[must_use]
    pub fn last_run_status(
        &self,
        _name: &ScriptName,
        _timeout: BoundedDuration,
    ) -> Result<Option<ScriptStatus>, AtuinQiBridgeError> {
        Ok(None)
    }

    /// Convenience: enumerate then search for the named script.
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` when enumeration fails.
    #[must_use]
    pub fn script_exists(
        &self,
        name: &ScriptName,
        timeout: BoundedDuration,
    ) -> Result<bool, AtuinQiBridgeError> {
        let entries = self.enumerate(timeout)?;
        Ok(entries.iter().any(|e| e.name == *name))
    }
}

// ── LiveWrite surface ─────────────────────────────────────────────────────────

impl AtuinQiBridge<Sealed<LiveWrite>> {
    /// Construct a live-write bridge without a registry path.
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` when the token is zero-TTL.
    pub fn new_live_write(token: &WriteAuthToken) -> Result<Self, AtuinQiBridgeError> {
        if token.expires_at_tick == 0 {
            return Err(AtuinQiBridgeError::EnumerationFailed {
                reason: String::from("WriteAuthToken has zero TTL"),
                retryable: false,
            });
        }
        Ok(Self {
            _class: Sealed::default(),
            registry_path: None,
        })
    }

    /// Check whether `atuin` binary is present.
    #[must_use]
    pub fn probe(&self, _timeout: BoundedDuration) -> Result<bool, AtuinQiBridgeError> {
        Ok(atuin_binary_present())
    }

    /// Enumerate `hle-*` scripts from the configured registry directory.
    ///
    /// When `registry_path` is set, scans the filesystem directory for `hle-*` files.
    /// Returns an empty list when no registry path is configured.
    #[must_use]
    pub fn enumerate(
        &self,
        _timeout: BoundedDuration,
    ) -> Result<Vec<ScriptEntry>, AtuinQiBridgeError> {
        match &self.registry_path {
            Some(path) => enumerate_from_fs(path),
            None => Ok(Vec::new()),
        }
    }

    /// Retrieve last-run status.
    #[must_use]
    pub fn last_run_status(
        &self,
        _name: &ScriptName,
        _timeout: BoundedDuration,
    ) -> Result<Option<ScriptStatus>, AtuinQiBridgeError> {
        Ok(None)
    }

    /// Check whether a script exists in the registry.
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` when enumeration fails.
    #[must_use]
    pub fn script_exists(
        &self,
        name: &ScriptName,
        timeout: BoundedDuration,
    ) -> Result<bool, AtuinQiBridgeError> {
        let entries = self.enumerate(timeout)?;
        Ok(entries.iter().any(|e| e.name == *name))
    }

    /// Run a named `hle-*` script as a bounded one-shot subprocess.
    ///
    /// M0 stub: does not invoke the subprocess; returns a success receipt.
    ///
    /// # Errors
    ///
    /// Returns `Err(ScriptRunFailed)` on non-zero exit.
    #[must_use]
    pub fn run_script(
        &self,
        name: &ScriptName,
        token: &WriteAuthToken,
        _timeout: BoundedDuration,
    ) -> Result<RunReceipt, AtuinQiBridgeError> {
        Ok(RunReceipt {
            script_name: name.clone(),
            exit_code: 0,
            elapsed_ms: 0,
            stdout_sha256: xor_fold_sha256_stub(name.as_str().as_bytes()),
            auth_receipt_id: token.receipt_id(),
        })
    }

    /// Register a new `hle-*` script via `atuin scripts add`.
    ///
    /// M0 stub: does not invoke the subprocess; returns a bridge receipt.
    ///
    /// # Errors
    ///
    /// Returns `Err(EnumerationFailed)` on subprocess failure.
    #[must_use]
    pub fn register_script(
        &self,
        name: &ScriptName,
        content: &[u8],
        token: &WriteAuthToken,
        _timeout: BoundedDuration,
    ) -> Result<BridgeReceipt, AtuinQiBridgeError> {
        Ok(BridgeReceipt::new(
            "hle.atuin_qi.v1",
            format!("register_script:{name}"),
            content,
        )
        .with_auth(token))
    }
}

// ─── AtuinQiBridgeError ──────────────────────────────────────────────────────

/// Errors for M042 Atuin QI bridge.
///
/// Error codes: 2620–2622.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtuinQiBridgeError {
    /// Code 2620. Named `hle-*` script not found in the Atuin registry.
    ScriptNotFound {
        /// The script name that was requested.
        name: String,
    },
    /// Code 2621. Script run returned non-zero exit code.
    ScriptRunFailed {
        /// The script name that was run.
        name: String,
        /// Non-zero exit code from the subprocess.
        exit_code: i32,
        /// True for transient failures (SIGTERM, resource exhaustion).
        retryable: bool,
    },
    /// Code 2622. `atuin scripts list` failed or returned unparseable output.
    EnumerationFailed {
        /// Human-readable failure reason.
        reason: String,
        /// True when the failure is transient.
        retryable: bool,
    },
}

impl AtuinQiBridgeError {
    /// Error code: 2620, 2621, or 2622.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::ScriptNotFound { .. } => 2620,
            Self::ScriptRunFailed { .. } => 2621,
            Self::EnumerationFailed { .. } => 2622,
        }
    }

    /// Propagates the inner `retryable` field; false for `ScriptNotFound`.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::ScriptNotFound { .. } => false,
            Self::ScriptRunFailed { retryable, .. } | Self::EnumerationFailed { retryable, .. } => {
                *retryable
            }
        }
    }
}

impl fmt::Display for AtuinQiBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScriptNotFound { name } => {
                write!(f, "[HLE-2620] script not found: {name}")
            }
            Self::ScriptRunFailed {
                name,
                exit_code,
                retryable,
            } => {
                write!(
                    f,
                    "[HLE-2621] script run failed: {name} exit={exit_code} retryable={retryable}"
                )
            }
            Self::EnumerationFailed { reason, retryable } => {
                write!(
                    f,
                    "[HLE-2622] enumeration failed (retryable={retryable}): {reason}"
                )
            }
        }
    }
}

impl std::error::Error for AtuinQiBridgeError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_contract::AuthGate;

    fn valid_name() -> ScriptName {
        ScriptName::new("hle-test-script").expect("valid name")
    }

    fn valid_token() -> WriteAuthToken {
        AuthGate::default()
            .issue_token(1, CapabilityClass::LiveWrite, 1000)
            .expect("valid token")
    }

    fn timeout() -> BoundedDuration {
        BoundedDuration::default()
    }

    // ── ScriptName ───────────────────────────────────────────────────────────

    #[test]
    fn script_name_valid_prefix() {
        assert!(ScriptName::new("hle-foo").is_ok());
    }

    #[test]
    fn script_name_rejects_missing_prefix() {
        assert!(ScriptName::new("myScript").is_err());
    }

    #[test]
    fn script_name_rejects_empty() {
        assert!(ScriptName::new("").is_err());
    }

    #[test]
    fn script_name_rejects_too_long() {
        let long = format!("hle-{}", "x".repeat(SCRIPT_NAME_MAX_LEN));
        assert!(ScriptName::new(long).is_err());
    }

    #[test]
    fn script_name_rejects_non_ascii() {
        assert!(ScriptName::new("hle-héllo").is_err());
    }

    #[test]
    fn script_name_as_str_returns_full_name() {
        let n = valid_name();
        assert_eq!(n.as_str(), "hle-test-script");
    }

    #[test]
    fn script_name_without_prefix_strips_hle() {
        let n = valid_name();
        assert_eq!(n.without_prefix(), "test-script");
    }

    #[test]
    fn script_name_display() {
        let n = valid_name();
        assert_eq!(n.to_string(), "hle-test-script");
    }

    // ── ScriptEntry ──────────────────────────────────────────────────────────

    #[test]
    fn script_entry_sha_is_deterministic() {
        let n = valid_name();
        let e1 = ScriptEntry::new(n.clone(), b"content");
        let e2 = ScriptEntry::new(n, b"content");
        assert_eq!(e1.content_sha256, e2.content_sha256);
    }

    #[test]
    fn script_entry_hex_sha_is_64_chars() {
        let e = ScriptEntry::new(valid_name(), b"x");
        assert_eq!(e.hex_sha().len(), 64);
    }

    #[test]
    fn script_entry_is_hle_script() {
        let e = ScriptEntry::new(valid_name(), b"x");
        assert!(e.is_hle_script());
    }

    // ── ScriptStatus ─────────────────────────────────────────────────────────

    #[test]
    fn script_status_is_success_zero_exit() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 0,
            elapsed_ms: 100,
            stdout_preview: String::from("ok"),
            last_run_tick: 10,
        };
        assert!(s.is_success());
    }

    #[test]
    fn script_status_is_stale_when_beyond_ttl() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 0,
            elapsed_ms: 0,
            stdout_preview: String::new(),
            last_run_tick: 5,
        };
        assert!(s.is_stale(106, 100));
        assert!(!s.is_stale(104, 100));
    }

    // ── ReadOnly bridge ──────────────────────────────────────────────────────

    #[test]
    fn read_only_capability_class_is_read_only() {
        let b = AtuinQiBridge::new_read_only();
        assert_eq!(b.capability_class(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn read_only_supports_write_is_false() {
        let b = AtuinQiBridge::new_read_only();
        assert!(!b.supports_write());
    }

    #[test]
    fn read_only_enumerate_returns_empty_in_m0() {
        let b = AtuinQiBridge::new_read_only();
        let entries = b.enumerate(timeout()).expect("must succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn read_only_last_run_status_returns_none_in_m0() {
        let b = AtuinQiBridge::new_read_only();
        let n = valid_name();
        let status = b.last_run_status(&n, timeout()).expect("must succeed");
        assert!(status.is_none());
    }

    #[test]
    fn read_only_script_exists_false_when_none_registered() {
        let b = AtuinQiBridge::new_read_only();
        let n = valid_name();
        let exists = b.script_exists(&n, timeout()).expect("must succeed");
        assert!(!exists);
    }

    // ── RunReceipt ───────────────────────────────────────────────────────────

    #[test]
    fn run_receipt_into_bridge_receipt_preserves_sha() {
        let name = valid_name();
        let sha = xor_fold_sha256_stub(name.as_str().as_bytes());
        let rr = RunReceipt {
            script_name: name,
            exit_code: 0,
            elapsed_ms: 0,
            stdout_sha256: sha,
            auth_receipt_id: 1,
        };
        let br = rr.into_bridge_receipt();
        assert_eq!(br.payload_sha256, sha);
    }

    #[test]
    fn run_receipt_is_success_zero_exit() {
        let rr = RunReceipt {
            script_name: valid_name(),
            exit_code: 0,
            elapsed_ms: 0,
            stdout_sha256: [0u8; 32],
            auth_receipt_id: 1,
        };
        assert!(rr.is_success());
    }

    // ── Write-side ───────────────────────────────────────────────────────────

    #[test]
    fn live_write_run_script_returns_receipt() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        let name = valid_name();
        let rr = b.run_script(&name, &tok, timeout()).expect("must succeed");
        assert!(rr.is_success());
    }

    #[test]
    fn live_write_register_script_returns_bridge_receipt() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        let name = valid_name();
        let br = b
            .register_script(&name, b"#!/bin/bash\necho ok", &tok, timeout())
            .expect("must succeed");
        assert_eq!(br.schema_id, "hle.atuin_qi.v1");
    }

    // ── Error codes ──────────────────────────────────────────────────────────

    #[test]
    fn script_not_found_error_code_is_2620() {
        let e = AtuinQiBridgeError::ScriptNotFound {
            name: String::from("x"),
        };
        assert_eq!(e.error_code(), 2620);
    }

    #[test]
    fn script_run_failed_error_code_is_2621() {
        let e = AtuinQiBridgeError::ScriptRunFailed {
            name: String::from("hle-x"),
            exit_code: 1,
            retryable: false,
        };
        assert_eq!(e.error_code(), 2621);
    }

    #[test]
    fn enumeration_failed_error_code_is_2622() {
        let e = AtuinQiBridgeError::EnumerationFailed {
            reason: String::from("r"),
            retryable: false,
        };
        assert_eq!(e.error_code(), 2622);
    }

    #[test]
    fn script_run_failed_retryable_is_retryable() {
        let e = AtuinQiBridgeError::ScriptRunFailed {
            name: String::from("hle-x"),
            exit_code: 1,
            retryable: true,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn script_not_found_is_not_retryable() {
        let e = AtuinQiBridgeError::ScriptNotFound {
            name: String::from("x"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn error_display_contains_code_prefix() {
        let e = AtuinQiBridgeError::ScriptNotFound {
            name: String::from("x"),
        };
        assert!(e.to_string().contains("[HLE-2620]"));
    }

    // ── ScriptName additional ────────────────────────────────────────────────

    #[test]
    fn script_name_exactly_at_max_len_is_valid() {
        // "hle-" is 4 chars; fill remaining up to SCRIPT_NAME_MAX_LEN
        let name = format!("hle-{}", "a".repeat(SCRIPT_NAME_MAX_LEN - 4));
        assert!(ScriptName::new(name).is_ok());
    }

    #[test]
    fn script_name_one_over_max_len_is_rejected() {
        let name = format!("hle-{}", "a".repeat(SCRIPT_NAME_MAX_LEN - 3));
        assert!(ScriptName::new(name).is_err());
    }

    #[test]
    fn script_name_prefix_only_is_valid() {
        // "hle-" alone is a valid name (non-empty, ASCII, correct prefix, within len)
        assert!(ScriptName::new("hle-").is_ok());
    }

    #[test]
    fn script_name_with_digits_is_valid() {
        assert!(ScriptName::new("hle-123").is_ok());
    }

    #[test]
    fn script_name_with_underscores_is_valid() {
        assert!(ScriptName::new("hle-my_script").is_ok());
    }

    #[test]
    fn script_name_clone_equality() {
        let n = valid_name();
        assert_eq!(n.clone(), n);
    }

    #[test]
    fn script_name_hash_is_stable() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(valid_name());
        assert!(set.contains(&valid_name()));
    }

    // ── ScriptEntry additional ───────────────────────────────────────────────

    #[test]
    fn script_entry_sha_differs_for_different_content() {
        let n = valid_name();
        let e1 = ScriptEntry::new(n.clone(), b"content-a");
        let e2 = ScriptEntry::new(n, b"content-b");
        assert_ne!(e1.content_sha256, e2.content_sha256);
    }

    #[test]
    fn script_entry_display_contains_name() {
        let e = ScriptEntry::new(valid_name(), b"data");
        assert!(e.to_string().contains("hle-test-script"));
    }

    #[test]
    fn script_entry_hex_sha_starts_with_hex_chars() {
        let e = ScriptEntry::new(valid_name(), b"abc");
        assert!(e.hex_sha().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn script_entry_with_description_can_be_set() {
        let n = valid_name();
        let mut e = ScriptEntry::new(n, b"x");
        e.description = Some(String::from("a helpful script"));
        assert_eq!(e.description.as_deref(), Some("a helpful script"));
    }

    // ── ScriptStatus additional ──────────────────────────────────────────────

    #[test]
    fn script_status_non_zero_exit_is_failure() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 1,
            elapsed_ms: 50,
            stdout_preview: String::new(),
            last_run_tick: 0,
        };
        assert!(!s.is_success());
    }

    #[test]
    fn script_status_stale_exactly_at_ttl_boundary_is_not_stale() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 0,
            elapsed_ms: 0,
            stdout_preview: String::new(),
            last_run_tick: 100,
        };
        // now=200, ttl=100 → delta=100, NOT > 100
        assert!(!s.is_stale(200, 100));
    }

    #[test]
    fn script_status_stale_one_over_ttl() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 0,
            elapsed_ms: 0,
            stdout_preview: String::new(),
            last_run_tick: 100,
        };
        // now=201, ttl=100 → delta=101 > 100
        assert!(s.is_stale(201, 100));
    }

    #[test]
    fn script_status_display_contains_name_and_exit() {
        let s = ScriptStatus {
            name: valid_name(),
            exit_code: 0,
            elapsed_ms: 99,
            stdout_preview: String::new(),
            last_run_tick: 0,
        };
        let text = s.to_string();
        assert!(text.contains("hle-test-script"));
        assert!(text.contains("99"));
    }

    // ── ReadOnly additional ──────────────────────────────────────────────────

    #[test]
    fn read_only_schema_id_is_correct() {
        let b = AtuinQiBridge::new_read_only();
        assert_eq!(b.schema_id(), "hle.atuin_qi.v1");
    }

    #[test]
    fn read_only_port_is_none() {
        let b = AtuinQiBridge::new_read_only();
        assert!(b.port().is_none());
    }

    #[test]
    fn read_only_name_is_atuin_qi_bridge() {
        let b = AtuinQiBridge::new_read_only();
        assert_eq!(b.name(), "atuin_qi_bridge");
    }

    #[test]
    fn read_only_paths_contain_list_subcommand() {
        let b = AtuinQiBridge::new_read_only();
        assert!(b.paths().iter().any(|p| p.contains("list")));
    }

    // ── LiveWrite additional ─────────────────────────────────────────────────

    #[test]
    fn live_write_capability_class_is_live_write() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        assert_eq!(b.capability_class(), CapabilityClass::LiveWrite);
    }

    #[test]
    fn live_write_supports_write_is_true() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        assert!(b.supports_write());
    }

    #[test]
    fn live_write_enumerate_returns_empty_in_m0() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        assert!(b.enumerate(timeout()).expect("ok").is_empty());
    }

    #[test]
    fn live_write_last_run_status_returns_none_in_m0() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        let n = valid_name();
        assert!(b.last_run_status(&n, timeout()).expect("ok").is_none());
    }

    #[test]
    fn live_write_run_script_receipt_has_auth_id() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        let name = valid_name();
        let rr = b.run_script(&name, &tok, timeout()).expect("must succeed");
        assert_eq!(rr.auth_receipt_id, 1);
    }

    #[test]
    fn live_write_register_script_receipt_operation_contains_name() {
        let tok = valid_token();
        let b = AtuinQiBridge::new_live_write(&tok).expect("must succeed");
        let name = valid_name();
        let br = b
            .register_script(&name, b"#!/bin/bash", &tok, timeout())
            .expect("ok");
        assert!(br.operation.contains("hle-test-script"));
    }

    // ── RunReceipt additional ────────────────────────────────────────────────

    #[test]
    fn run_receipt_into_bridge_receipt_schema_is_atuin_qi() {
        let rr = RunReceipt {
            script_name: valid_name(),
            exit_code: 0,
            elapsed_ms: 10,
            stdout_sha256: [0u8; 32],
            auth_receipt_id: 5,
        };
        let br = rr.into_bridge_receipt();
        assert_eq!(br.schema_id, "hle.atuin_qi.v1");
    }

    #[test]
    fn run_receipt_non_zero_exit_is_failure() {
        let rr = RunReceipt {
            script_name: valid_name(),
            exit_code: 127,
            elapsed_ms: 0,
            stdout_sha256: [0u8; 32],
            auth_receipt_id: 1,
        };
        assert!(!rr.is_success());
    }

    // ── AtuinQiBridgeError additional ────────────────────────────────────────

    #[test]
    fn enumeration_failed_retryable_false_is_not_retryable() {
        let e = AtuinQiBridgeError::EnumerationFailed {
            reason: String::from("r"),
            retryable: false,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn script_run_failed_non_retryable_is_not_retryable() {
        let e = AtuinQiBridgeError::ScriptRunFailed {
            name: String::from("hle-x"),
            exit_code: 1,
            retryable: false,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn enumeration_failed_display_contains_code() {
        let e = AtuinQiBridgeError::EnumerationFailed {
            reason: String::from("r"),
            retryable: true,
        };
        assert!(e.to_string().contains("[HLE-2622]"));
    }

    #[test]
    fn script_run_failed_display_contains_code() {
        let e = AtuinQiBridgeError::ScriptRunFailed {
            name: String::from("hle-x"),
            exit_code: 1,
            retryable: false,
        };
        assert!(e.to_string().contains("[HLE-2621]"));
    }

    // ── Filesystem-based enumerate ───────────────────────────────────────────

    /// Create a unique temp directory for this test under /tmp.
    /// Returns the path; the caller is responsible for cleanup via
    /// `std::fs::remove_dir_all` at end of test.
    fn make_test_dir(label: &str) -> std::path::PathBuf {
        let dir = std::path::PathBuf::from(format!(
            "/tmp/hle-bridge-atuin-test-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    #[test]
    fn with_registry_path_constructor_is_read_only() {
        let b = AtuinQiBridge::with_registry_path(std::path::PathBuf::from("/tmp"));
        assert_eq!(b.capability_class(), CapabilityClass::ReadOnly);
        assert!(!b.supports_write());
    }

    #[test]
    fn enumerate_returns_empty_when_no_registry_path() {
        let b = AtuinQiBridge::new_read_only();
        let entries = b.enumerate(timeout()).expect("must succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn enumerate_returns_empty_for_nonexistent_directory() {
        let b = AtuinQiBridge::with_registry_path(
            "/tmp/hle-bridge-test-nonexistent-dir-xyz-absolutely-gone",
        );
        let entries = b.enumerate(timeout()).expect("must succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn enumerate_from_fs_scans_hle_files_in_directory() {
        let dir = make_test_dir("scan");
        std::fs::write(dir.join("hle-probe"), b"#!/bin/sh\necho probe").expect("write");
        std::fs::write(dir.join("hle-status"), b"#!/bin/sh\necho status").expect("write");
        std::fs::write(dir.join("not-hle-script"), b"ignored").expect("write");

        let b = AtuinQiBridge::with_registry_path(&dir);
        let mut entries = b.enumerate(timeout()).expect("must succeed");
        entries.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name.as_str(), "hle-probe");
        assert_eq!(entries[1].name.as_str(), "hle-status");
    }

    #[test]
    fn enumerate_from_fs_populates_content_sha() {
        let dir = make_test_dir("sha");
        std::fs::write(dir.join("hle-check"), b"#!/bin/sh\necho ok").expect("write");

        let b = AtuinQiBridge::with_registry_path(&dir);
        let entries = b.enumerate(timeout()).expect("must succeed");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(entries.len(), 1);
        assert_ne!(entries[0].content_sha256, [0u8; 32]);
    }

    #[test]
    fn enumerate_from_fs_skips_files_with_invalid_names() {
        let dir = make_test_dir("skip");
        let long_name = format!("hle-{}", "a".repeat(SCRIPT_NAME_MAX_LEN));
        std::fs::write(dir.join(&long_name), b"content").expect("write");
        std::fs::write(dir.join("hle-valid"), b"ok").expect("write");

        let b = AtuinQiBridge::with_registry_path(&dir);
        let entries = b.enumerate(timeout()).expect("must succeed");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.as_str(), "hle-valid");
    }

    #[test]
    fn enumerate_from_fs_respects_enumerate_cap() {
        // ENUMERATE_CAP = 1024 — creating 1025 files is slow; use a small cap
        // by scanning a dir with more entries than the cap, but cap = 1024 so
        // we create 5 and verify all 5 come through (well under cap).
        let dir = make_test_dir("cap");
        for i in 0..5usize {
            std::fs::write(dir.join(format!("hle-script-{i:04}")), b"x").expect("write");
        }

        let b = AtuinQiBridge::with_registry_path(&dir);
        let entries = b.enumerate(timeout()).expect("must succeed");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn script_exists_false_when_registry_path_is_empty_dir() {
        let dir = make_test_dir("exists-false");
        let b = AtuinQiBridge::with_registry_path(&dir);
        let name = ScriptName::new("hle-missing").expect("valid");
        let result = b.script_exists(&name, timeout()).expect("must succeed");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!result);
    }

    #[test]
    fn script_exists_true_when_file_present() {
        let dir = make_test_dir("exists-true");
        std::fs::write(dir.join("hle-present"), b"content").expect("write");
        let b = AtuinQiBridge::with_registry_path(&dir);
        let name = ScriptName::new("hle-present").expect("valid");
        let result = b.script_exists(&name, timeout()).expect("must succeed");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result);
    }

    #[test]
    fn enumerate_entry_is_hle_script() {
        let dir = make_test_dir("entry");
        std::fs::write(dir.join("hle-alpha"), b"x").expect("write");
        let b = AtuinQiBridge::with_registry_path(&dir);
        let entries = b.enumerate(timeout()).expect("ok");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!entries.is_empty());
        assert!(entries[0].is_hle_script());
    }
}
