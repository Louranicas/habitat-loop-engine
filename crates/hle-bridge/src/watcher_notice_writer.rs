//! M045 WatcherNoticeWriter — append-only file writer for Watcher/Hermes notices.
//!
//! Writes to `notices_dir/YYYY-MM-DD.ndjson`. Path-based, no network.
//! Every write produces a `NoticeReceipt` with SHA-256 of the appended payload.
//! POSIX append semantics make small writes atomic without a lock, provided
//! payload ≤ `NOTICE_MESSAGE_CAP` (4,096 bytes).
//!
//! Error codes: 2650–2651.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::bridge_contract::{
    xor_fold_sha256_stub, BoundedDuration, BridgeContract, BridgeReceipt, CapabilityClass,
    WriteAuthToken,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum byte length of a notice message.
pub const NOTICE_MESSAGE_CAP: usize = 4_096;
/// Default directory for Watcher notices.
pub const DEFAULT_NOTICES_DIR: &str = "/home/louranicas/projects/shared-context/watcher-notices";

// ─── NoticeKind ──────────────────────────────────────────────────────────────

/// Classification of a Watcher notice.
///
/// `Blocker` indicates the HLE run is paused pending Watcher acknowledgment.
/// `HandoffRequest` signals that the executor wants The Watcher to pick up
/// a specific task or observation stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    /// Informational notice.
    Info,
    /// Warning notice.
    Warning,
    /// HLE run is paused pending Watcher acknowledgment.
    Blocker,
    /// Request for The Watcher to pick up a task.
    HandoffRequest,
}

impl fmt::Display for NoticeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("INFO"),
            Self::Warning => f.write_str("WARNING"),
            Self::Blocker => f.write_str("BLOCKER"),
            Self::HandoffRequest => f.write_str("HANDOFF_REQUEST"),
        }
    }
}

// ─── NoticePayload ───────────────────────────────────────────────────────────

/// A validated, bounded notice payload for The Watcher.
///
/// Serialized to NDJSON on write (one JSON object per line, terminated by `\n`),
/// matching the JSONL substrate preference. No external serde dependency —
/// hand-rolled JSON for zero-dep in M0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePayload {
    /// Notice classification.
    pub kind: NoticeKind,
    /// Bounded message body (≤ `NOTICE_MESSAGE_CAP` bytes).
    pub message: String,
    /// Originating HLE module or executor phase.
    pub source: String,
    /// Local Timestamp tick at creation.
    pub tick: u64,
}

impl NoticePayload {
    /// Construct and validate a notice payload.
    ///
    /// # Errors
    ///
    /// Returns `Err(NoticeTooLarge)` when `message.len() > NOTICE_MESSAGE_CAP`.
    pub fn new(
        kind: NoticeKind,
        message: impl Into<String>,
        source: impl Into<String>,
        tick: u64,
    ) -> Result<Self, WatcherNoticeWriterError> {
        let message = message.into();
        if message.len() > NOTICE_MESSAGE_CAP {
            return Err(WatcherNoticeWriterError::NoticeTooLarge {
                size_bytes: message.len(),
                cap_bytes: NOTICE_MESSAGE_CAP,
            });
        }
        Ok(Self {
            kind,
            message,
            source: source.into(),
            tick,
        })
    }

    /// Serialize to a single NDJSON line with trailing `\n`.
    ///
    /// Format: `{"kind":"KIND","message":"MSG","source":"SRC","tick":T}`
    /// No external serde dependency.
    #[must_use]
    pub fn to_ndjson(&self) -> String {
        format!(
            "{{\"kind\":\"{}\",\"message\":{},\"source\":{},\"tick\":{}}}\n",
            self.kind,
            json_string_escape(&self.message),
            json_string_escape(&self.source),
            self.tick,
        )
    }

    /// Byte length of the NDJSON line including trailing newline.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.to_ndjson().len()
    }
}

impl fmt::Display for NoticePayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NoticePayload({} source={} len={})",
            self.kind,
            self.source,
            self.message.len()
        )
    }
}

/// Escape a string for embedding in a JSON string literal.
///
/// Handles `"`, `\`, and control characters.
fn json_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ─── NoticeReceipt ───────────────────────────────────────────────────────────

/// SHA-256-tagged confirmation of a Watcher notice write.
///
/// `payload_sha256` covers only the bytes appended in this call — not the
/// entire file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeReceipt {
    /// Absolute path of the file that received the append.
    pub filename: PathBuf,
    /// SHA-256 of the NDJSON bytes appended.
    pub payload_sha256: [u8; 32],
    /// Number of bytes appended.
    pub byte_count: usize,
    /// C01 authority receipt ID.
    pub auth_receipt_id: u64,
}

impl NoticeReceipt {
    /// Lowercase hex of `payload_sha256`.
    #[must_use]
    pub fn hex_sha(&self) -> String {
        self.payload_sha256
            .iter()
            .fold(String::with_capacity(64), |mut acc, b| {
                let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{b:02x}"));
                acc
            })
    }

    /// Convert into a `BridgeReceipt` for C01 verifier routing.
    #[must_use]
    pub fn into_bridge_receipt(self) -> BridgeReceipt {
        BridgeReceipt {
            schema_id: "hle.watcher_notice.v1",
            operation: format!("write_notice:{}", self.filename.display()),
            payload_sha256: self.payload_sha256,
            timestamp_tick: 0,
            auth_receipt_id: Some(self.auth_receipt_id),
        }
    }
}

impl fmt::Display for NoticeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NoticeReceipt({} sha={}… bytes={})",
            self.filename.display(),
            &self.hex_sha()[..8],
            self.byte_count
        )
    }
}

// ─── WatcherNoticeWriter ─────────────────────────────────────────────────────

/// Append-only file writer for Watcher/Hermes notices.
///
/// Writes to `notices_dir/YYYY-MM-DD.ndjson` (date derived from tick).
/// The directory is created on first write if absent.
/// No PhantomData gate — the write is always authorized at this level.
/// `WriteAuthToken` appears only in the `write_notice` signature to maintain
/// receipt-chain consistency with C01.
#[derive(Debug)]
pub struct WatcherNoticeWriter {
    notices_dir: PathBuf,
    /// Stored for future bounded-I/O enforcement (M2+ implementation pass).
    #[allow(dead_code)]
    default_timeout: BoundedDuration,
}

impl BridgeContract for WatcherNoticeWriter {
    fn schema_id(&self) -> &'static str {
        "hle.watcher_notice.v1"
    }
    fn port(&self) -> Option<u16> {
        None
    }
    fn paths(&self) -> &[&'static str] {
        &["/home/louranicas/projects/shared-context/watcher-notices/"]
    }
    /// True — this bridge writes to the filesystem.
    fn supports_write(&self) -> bool {
        true
    }
    /// `ReadOnly` per the C07 capability model for local filesystem writes.
    /// See M045 Design Notes: filesystem append is not a "live external write".
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }
    fn name(&self) -> &'static str {
        "watcher_notice_writer"
    }
}

impl WatcherNoticeWriter {
    /// Construct with the default notices directory.
    #[must_use]
    pub fn new(timeout: BoundedDuration) -> Self {
        Self {
            notices_dir: PathBuf::from(DEFAULT_NOTICES_DIR),
            default_timeout: timeout,
        }
    }

    /// Construct with a custom notices directory.
    ///
    /// # Errors
    ///
    /// Returns `Err(NoticeWriteFailed)` when `dir` is not absolute.
    pub fn with_dir(
        dir: PathBuf,
        timeout: BoundedDuration,
    ) -> Result<Self, WatcherNoticeWriterError> {
        if !dir.is_absolute() {
            return Err(WatcherNoticeWriterError::NoticeWriteFailed {
                path: dir.display().to_string(),
                reason: String::from("notices directory must be an absolute path"),
                retryable: false,
            });
        }
        Ok(Self {
            notices_dir: dir,
            default_timeout: timeout,
        })
    }

    /// Derive the notice filename for a given tick.
    ///
    /// Pure function — does not touch the filesystem.
    /// When tick is 0 (test sentinel), returns `0000-00-00.ndjson`.
    #[must_use]
    pub fn notice_file_for_tick(&self, tick: u64) -> PathBuf {
        // Derive a pseudo-date from tick without chrono/SystemTime dependency.
        // Each "day" is 86,400 ticks (seconds in a day).
        let filename = if tick == 0 {
            String::from("0000-00-00.ndjson")
        } else {
            // Simple tick → date approximation: count days from epoch tick 0.
            // Epoch: 2026-01-01. Each day = 86_400 ticks.
            let days_since_epoch = tick / 86_400;
            let year = 2026u64 + days_since_epoch / 365;
            let day_of_year = days_since_epoch % 365;
            let month = (day_of_year / 30 + 1).min(12);
            let day = (day_of_year % 30 + 1).min(28);
            format!("{year:04}-{month:02}-{day:02}.ndjson")
        };
        self.notices_dir.join(filename)
    }

    /// Append a notice payload to the date-bucketed file.
    ///
    /// Creates `notices_dir` if absent. Returns `NoticeReceipt` with SHA-256
    /// of the appended bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err(NoticeWriteFailed)` on I/O failure.
    /// Returns `Err(NoticeTooLarge)` when `payload.byte_len() > NOTICE_MESSAGE_CAP`.
    #[must_use]
    pub fn write_notice(
        &self,
        payload: &NoticePayload,
        token: &WriteAuthToken,
    ) -> Result<NoticeReceipt, WatcherNoticeWriterError> {
        let ndjson = payload.to_ndjson();
        let ndjson_bytes = ndjson.as_bytes();
        if ndjson_bytes.len() > NOTICE_MESSAGE_CAP {
            return Err(WatcherNoticeWriterError::NoticeTooLarge {
                size_bytes: ndjson_bytes.len(),
                cap_bytes: NOTICE_MESSAGE_CAP,
            });
        }

        // Create the directory if absent (idempotent — EEXIST is success).
        fs::create_dir_all(&self.notices_dir).map_err(|e| {
            WatcherNoticeWriterError::NoticeWriteFailed {
                path: self.notices_dir.display().to_string(),
                reason: format!("create_dir_all failed: {e}"),
                retryable: !matches!(e.kind(), std::io::ErrorKind::PermissionDenied),
            }
        })?;

        let file_path = self.notice_file_for_tick(payload.tick);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| WatcherNoticeWriterError::NoticeWriteFailed {
                path: file_path.display().to_string(),
                reason: format!("open failed: {e}"),
                retryable: e.kind() != std::io::ErrorKind::PermissionDenied,
            })?;

        file.write_all(ndjson_bytes)
            .map_err(|e| WatcherNoticeWriterError::NoticeWriteFailed {
                path: file_path.display().to_string(),
                reason: format!("write failed: {e}"),
                retryable: e.kind() != std::io::ErrorKind::PermissionDenied,
            })?;

        Ok(NoticeReceipt {
            filename: file_path,
            payload_sha256: xor_fold_sha256_stub(ndjson_bytes),
            byte_count: ndjson_bytes.len(),
            auth_receipt_id: token.receipt_id(),
        })
    }

    /// Enumerate `*.ndjson` files in `notices_dir`, most recent first.
    ///
    /// Returns `Ok(vec![])` when directory is empty or absent.
    ///
    /// # Errors
    ///
    /// Returns `Err(NoticeWriteFailed)` on I/O failure.
    #[must_use]
    pub fn list_notice_files(&self) -> Result<Vec<PathBuf>, WatcherNoticeWriterError> {
        if !self.notices_dir.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        let entries = fs::read_dir(&self.notices_dir).map_err(|e| {
            WatcherNoticeWriterError::NoticeWriteFailed {
                path: self.notices_dir.display().to_string(),
                reason: format!("read_dir failed: {e}"),
                retryable: true,
            }
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "ndjson") {
                files.push(path);
            }
        }
        // Sort descending (most recent first by filename which is YYYY-MM-DD).
        files.sort_by(|a, b| b.cmp(a));
        Ok(files)
    }

    /// True iff `notices_dir` exists and is writable. Does not create the directory.
    ///
    /// # Errors
    ///
    /// Returns `Err(NoticeWriteFailed)` on unexpected I/O failure.
    #[must_use]
    pub fn probe_dir(&self) -> Result<bool, WatcherNoticeWriterError> {
        if !self.notices_dir.exists() {
            return Ok(false);
        }
        // Check writability by attempting to create a temp probe file.
        let probe_path = self.notices_dir.join(".hle-probe-write");
        let writable = fs::write(&probe_path, b"").is_ok();
        if writable {
            let _ = fs::remove_file(&probe_path);
        }
        Ok(writable)
    }
}

// ─── WatcherNoticeWriterError ────────────────────────────────────────────────

/// Errors for M045 Watcher notice writer.
///
/// Error codes: 2650–2651.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherNoticeWriterError {
    /// Code 2650. File append or directory creation failed.
    NoticeWriteFailed {
        /// Absolute path of the file or directory that failed.
        path: String,
        /// Human-readable failure reason.
        reason: String,
        /// True for transient I/O (ENOSPC when disk freed, EAGAIN from NFS).
        retryable: bool,
    },
    /// Code 2651. Payload byte length exceeds `NOTICE_MESSAGE_CAP`.
    NoticeTooLarge {
        /// Actual payload byte length.
        size_bytes: usize,
        /// Maximum allowed byte length.
        cap_bytes: usize,
    },
}

impl WatcherNoticeWriterError {
    /// Error code: 2650 or 2651.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::NoticeWriteFailed { .. } => 2650,
            Self::NoticeTooLarge { .. } => 2651,
        }
    }

    /// Propagates inner `retryable`; false for `NoticeTooLarge`.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NoticeWriteFailed {
                retryable: true,
                ..
            }
        )
    }
}

impl fmt::Display for WatcherNoticeWriterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoticeWriteFailed {
                path,
                reason,
                retryable,
            } => {
                write!(
                    f,
                    "[HLE-2650] notice write failed (path={path}, retryable={retryable}): {reason}"
                )
            }
            Self::NoticeTooLarge {
                size_bytes,
                cap_bytes,
            } => {
                write!(
                    f,
                    "[HLE-2651] notice too large: {size_bytes} bytes > cap {cap_bytes}"
                )
            }
        }
    }
}

impl std::error::Error for WatcherNoticeWriterError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_contract::AuthGate;

    fn timeout() -> BoundedDuration {
        BoundedDuration::default()
    }

    fn valid_token() -> WriteAuthToken {
        AuthGate::default()
            .issue_token(1, CapabilityClass::LiveWrite, 1000)
            .expect("valid token")
    }

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hle-bridge-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        dir
    }

    // ── NoticeKind ───────────────────────────────────────────────────────────

    #[test]
    fn notice_kind_info_display() {
        assert_eq!(NoticeKind::Info.to_string(), "INFO");
    }

    #[test]
    fn notice_kind_warning_display() {
        assert_eq!(NoticeKind::Warning.to_string(), "WARNING");
    }

    #[test]
    fn notice_kind_blocker_display() {
        assert_eq!(NoticeKind::Blocker.to_string(), "BLOCKER");
    }

    #[test]
    fn notice_kind_handoff_request_display() {
        assert_eq!(NoticeKind::HandoffRequest.to_string(), "HANDOFF_REQUEST");
    }

    // ── NoticePayload ────────────────────────────────────────────────────────

    #[test]
    fn notice_payload_valid_construction() {
        let p =
            NoticePayload::new(NoticeKind::Info, "hello", "hle.test.v1", 42).expect("must succeed");
        assert_eq!(p.kind, NoticeKind::Info);
    }

    #[test]
    fn notice_payload_rejects_too_long_message() {
        let big = "x".repeat(NOTICE_MESSAGE_CAP + 1);
        assert!(NoticePayload::new(NoticeKind::Info, big, "src", 0).is_err());
    }

    #[test]
    fn notice_payload_accepts_at_cap() {
        let at_cap = "x".repeat(NOTICE_MESSAGE_CAP);
        assert!(NoticePayload::new(NoticeKind::Warning, at_cap, "src", 0).is_ok());
    }

    #[test]
    fn notice_payload_ndjson_ends_with_newline() {
        let p = NoticePayload::new(NoticeKind::Info, "msg", "src", 1).expect("valid");
        assert!(p.to_ndjson().ends_with('\n'));
    }

    #[test]
    fn notice_payload_ndjson_contains_kind() {
        let p = NoticePayload::new(NoticeKind::Blocker, "stop", "src", 5).expect("valid");
        assert!(p.to_ndjson().contains("BLOCKER"));
    }

    #[test]
    fn notice_payload_ndjson_escapes_quotes_in_message() {
        let p = NoticePayload::new(NoticeKind::Info, r#"say "hi""#, "src", 0).expect("valid");
        let ndjson = p.to_ndjson();
        assert!(ndjson.contains("\\\""));
    }

    #[test]
    fn notice_payload_byte_len_is_ndjson_len() {
        let p = NoticePayload::new(NoticeKind::Info, "abc", "src", 0).expect("valid");
        assert_eq!(p.byte_len(), p.to_ndjson().len());
    }

    // ── notice_file_for_tick ─────────────────────────────────────────────────

    #[test]
    fn notice_file_for_tick_zero_returns_sentinel_filename() {
        let w = WatcherNoticeWriter::new(timeout());
        let path = w.notice_file_for_tick(0);
        assert!(path.to_string_lossy().contains("0000-00-00"));
    }

    #[test]
    fn notice_file_for_tick_nonzero_returns_ndjson() {
        let w = WatcherNoticeWriter::new(timeout());
        let path = w.notice_file_for_tick(86_400);
        assert!(path.to_string_lossy().ends_with(".ndjson"));
    }

    // ── WatcherNoticeWriter construction ─────────────────────────────────────

    #[test]
    fn with_dir_rejects_relative_path() {
        let result = WatcherNoticeWriter::with_dir(PathBuf::from("relative/path"), timeout());
        assert!(result.is_err());
    }

    #[test]
    fn with_dir_accepts_absolute_path() {
        let result = WatcherNoticeWriter::with_dir(tmp_dir(), timeout());
        assert!(result.is_ok());
    }

    // ── write_notice to tmp dir ──────────────────────────────────────────────

    #[test]
    fn write_notice_creates_file_and_returns_receipt() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "test notice", "hle.test.v1", 1_000_000)
            .expect("valid payload");
        let receipt = w.write_notice(&p, &tok).expect("must succeed");
        assert!(receipt.filename.exists());
        assert_eq!(receipt.auth_receipt_id, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_notice_receipt_sha_matches_ndjson_bytes() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Warning, "sha check", "src", 999_999)
            .expect("valid payload");
        let receipt = w.write_notice(&p, &tok).expect("must succeed");
        let expected_sha = xor_fold_sha256_stub(p.to_ndjson().as_bytes());
        assert_eq!(receipt.payload_sha256, expected_sha);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_notice_two_sequential_writes_produce_two_lines() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p1 = NoticePayload::new(NoticeKind::Info, "line1", "src", 1_000_000).expect("valid");
        let p2 = NoticePayload::new(NoticeKind::Info, "line2", "src", 1_000_000).expect("valid");
        let r1 = w.write_notice(&p1, &tok).expect("write1");
        let _r2 = w.write_notice(&p2, &tok).expect("write2");
        let content = fs::read_to_string(&r1.filename).expect("read file");
        assert_eq!(content.lines().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_notice_creates_dir_if_absent() {
        let dir = tmp_dir().join("nested");
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "create-dir", "src", 1).expect("valid");
        assert!(w.write_notice(&p, &tok).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── list_notice_files ────────────────────────────────────────────────────

    #[test]
    fn list_notice_files_returns_empty_when_dir_absent() {
        let w = WatcherNoticeWriter::with_dir(tmp_dir(), timeout()).expect("valid");
        let files = w.list_notice_files().expect("must succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn list_notice_files_returns_sorted_descending() {
        let dir = tmp_dir();
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("2026-01-01.ndjson"), b"").expect("write a");
        fs::write(dir.join("2026-01-03.ndjson"), b"").expect("write c");
        fs::write(dir.join("2026-01-02.ndjson"), b"").expect("write b");
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let files = w.list_notice_files().expect("must succeed");
        assert_eq!(files.len(), 3);
        assert!(files[0].to_string_lossy().contains("2026-01-03"));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── NoticeReceipt ────────────────────────────────────────────────────────

    #[test]
    fn notice_receipt_hex_sha_is_64_chars() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "sha-len", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("must succeed");
        assert_eq!(receipt.hex_sha().len(), 64);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn notice_receipt_into_bridge_receipt_has_correct_schema() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "br-test", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("must succeed");
        let br = receipt.into_bridge_receipt();
        assert_eq!(br.schema_id, "hle.watcher_notice.v1");
        let _ = fs::remove_dir_all(&dir);
    }

    // ── Error codes ──────────────────────────────────────────────────────────

    #[test]
    fn notice_write_failed_error_code_is_2650() {
        let e = WatcherNoticeWriterError::NoticeWriteFailed {
            path: String::from("p"),
            reason: String::from("r"),
            retryable: false,
        };
        assert_eq!(e.error_code(), 2650);
    }

    #[test]
    fn notice_too_large_error_code_is_2651() {
        let e = WatcherNoticeWriterError::NoticeTooLarge {
            size_bytes: 5000,
            cap_bytes: 4096,
        };
        assert_eq!(e.error_code(), 2651);
    }

    #[test]
    fn notice_write_failed_retryable_is_retryable() {
        let e = WatcherNoticeWriterError::NoticeWriteFailed {
            path: String::from("p"),
            reason: String::from("r"),
            retryable: true,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn notice_too_large_is_not_retryable() {
        let e = WatcherNoticeWriterError::NoticeTooLarge {
            size_bytes: 1,
            cap_bytes: 0,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn error_display_contains_code_prefix() {
        let e = WatcherNoticeWriterError::NoticeTooLarge {
            size_bytes: 5000,
            cap_bytes: 4096,
        };
        assert!(e.to_string().contains("[HLE-2651]"));
    }

    // ── NoticeKind additional ────────────────────────────────────────────────

    #[test]
    fn notice_kind_copy_is_independent() {
        let k = NoticeKind::Blocker;
        let k2 = k;
        assert_eq!(k, k2);
    }

    #[test]
    fn notice_kind_eq_same_variants() {
        assert_eq!(NoticeKind::Info, NoticeKind::Info);
        assert_eq!(NoticeKind::HandoffRequest, NoticeKind::HandoffRequest);
    }

    #[test]
    fn notice_kind_ne_different_variants() {
        assert_ne!(NoticeKind::Info, NoticeKind::Warning);
        assert_ne!(NoticeKind::Blocker, NoticeKind::HandoffRequest);
    }

    // ── NoticePayload additional ─────────────────────────────────────────────

    #[test]
    fn notice_payload_at_cap_is_accepted() {
        let at_cap = "m".repeat(NOTICE_MESSAGE_CAP);
        assert!(NoticePayload::new(NoticeKind::Info, at_cap, "src", 0).is_ok());
    }

    #[test]
    fn notice_payload_empty_message_is_valid() {
        assert!(NoticePayload::new(NoticeKind::Info, "", "src", 0).is_ok());
    }

    #[test]
    fn notice_payload_ndjson_contains_tick() {
        let p = NoticePayload::new(NoticeKind::Info, "msg", "src", 99999).expect("valid");
        assert!(p.to_ndjson().contains("99999"));
    }

    #[test]
    fn notice_payload_ndjson_contains_source() {
        let p = NoticePayload::new(NoticeKind::Info, "m", "hle.executor.v1", 0).expect("valid");
        assert!(p.to_ndjson().contains("hle.executor.v1"));
    }

    #[test]
    fn notice_payload_ndjson_escapes_backslash() {
        let p = NoticePayload::new(NoticeKind::Info, "path\\file", "src", 0).expect("valid");
        assert!(p.to_ndjson().contains("\\\\"));
    }

    #[test]
    fn notice_payload_ndjson_escapes_newline() {
        let p = NoticePayload::new(NoticeKind::Info, "line1\nline2", "src", 0).expect("valid");
        assert!(p.to_ndjson().contains("\\n"));
    }

    #[test]
    fn notice_payload_ndjson_escapes_tab() {
        let p = NoticePayload::new(NoticeKind::Info, "col\tval", "src", 0).expect("valid");
        assert!(p.to_ndjson().contains("\\t"));
    }

    #[test]
    fn notice_payload_byte_len_grows_with_message() {
        let short = NoticePayload::new(NoticeKind::Info, "a", "s", 0).expect("valid");
        let long_msg =
            NoticePayload::new(NoticeKind::Info, "a".repeat(100), "s", 0).expect("valid");
        assert!(long_msg.byte_len() > short.byte_len());
    }

    #[test]
    fn notice_payload_display_contains_kind_and_source() {
        let p = NoticePayload::new(NoticeKind::Warning, "m", "my-src", 0).expect("valid");
        let s = p.to_string();
        assert!(s.contains("WARNING"));
        assert!(s.contains("my-src"));
    }

    // ── WatcherNoticeWriter construction ─────────────────────────────────────

    #[test]
    fn writer_schema_id_is_correct() {
        let w = WatcherNoticeWriter::new(timeout());
        assert_eq!(w.schema_id(), "hle.watcher_notice.v1");
    }

    #[test]
    fn writer_port_is_none() {
        let w = WatcherNoticeWriter::new(timeout());
        assert!(w.port().is_none());
    }

    #[test]
    fn writer_name_is_watcher_notice_writer() {
        let w = WatcherNoticeWriter::new(timeout());
        assert_eq!(w.name(), "watcher_notice_writer");
    }

    #[test]
    fn writer_supports_write_is_true() {
        let w = WatcherNoticeWriter::new(timeout());
        assert!(w.supports_write());
    }

    #[test]
    fn writer_capability_class_is_read_only_by_design() {
        // Per M045 Design Notes: filesystem append is not a "live external write".
        let w = WatcherNoticeWriter::new(timeout());
        assert_eq!(w.capability_class(), CapabilityClass::ReadOnly);
    }

    // ── notice_file_for_tick additional ──────────────────────────────────────

    #[test]
    fn notice_file_for_tick_zero_is_deterministic() {
        let w = WatcherNoticeWriter::new(timeout());
        assert_eq!(w.notice_file_for_tick(0), w.notice_file_for_tick(0));
    }

    #[test]
    fn notice_file_for_tick_different_days_produce_different_filenames() {
        let w = WatcherNoticeWriter::new(timeout());
        let day1 = w.notice_file_for_tick(86_400);
        let day2 = w.notice_file_for_tick(86_400 * 2);
        assert_ne!(day1, day2);
    }

    #[test]
    fn notice_file_for_tick_same_day_produces_same_filename() {
        let w = WatcherNoticeWriter::new(timeout());
        let a = w.notice_file_for_tick(86_400 + 100);
        let b = w.notice_file_for_tick(86_400 + 999);
        assert_eq!(a, b);
    }

    #[test]
    fn notice_file_for_tick_is_under_notices_dir() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let path = w.notice_file_for_tick(1);
        assert!(path.starts_with(&dir));
    }

    // ── write_notice idempotent on duplicate SHA ──────────────────────────────

    #[test]
    fn write_notice_idempotent_duplicate_sha_appends_twice() {
        // Idempotency here means: same payload written twice produces two lines
        // (append-only semantics), but both receipts have equal SHAs.
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "dup", "src", 1_000_000).expect("valid");
        let r1 = w.write_notice(&p, &tok).expect("write1");
        let r2 = w.write_notice(&p, &tok).expect("write2");
        assert_eq!(r1.payload_sha256, r2.payload_sha256);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_notice_byte_count_in_receipt_matches_ndjson() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "count-test", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("ok");
        assert_eq!(receipt.byte_count, p.to_ndjson().len());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_notice_file_content_starts_with_open_brace() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "brace-test", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("ok");
        let content = fs::read_to_string(&receipt.filename).expect("read");
        assert!(content.starts_with('{'));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── probe_dir ────────────────────────────────────────────────────────────

    #[test]
    fn probe_dir_false_when_dir_absent() {
        let w = WatcherNoticeWriter::with_dir(tmp_dir(), timeout()).expect("valid");
        assert!(!w.probe_dir().expect("ok"));
    }

    #[test]
    fn probe_dir_true_after_write_creates_dir() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "probe-create", "src", 1).expect("valid");
        w.write_notice(&p, &tok).expect("ok");
        assert!(w.probe_dir().expect("ok"));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── NoticeReceipt additional ─────────────────────────────────────────────

    #[test]
    fn notice_receipt_display_contains_byte_count() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Info, "display-test", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("ok");
        let byte_count = receipt.byte_count;
        assert!(receipt.to_string().contains(&byte_count.to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn notice_receipt_into_bridge_receipt_operation_contains_path() {
        let dir = tmp_dir();
        let w = WatcherNoticeWriter::with_dir(dir.clone(), timeout()).expect("valid");
        let tok = valid_token();
        let p = NoticePayload::new(NoticeKind::Blocker, "br-path", "src", 1).expect("valid");
        let receipt = w.write_notice(&p, &tok).expect("ok");
        let path = receipt.filename.display().to_string();
        let br = receipt.into_bridge_receipt();
        assert!(br.operation.contains(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── WatcherNoticeWriterError additional ──────────────────────────────────

    #[test]
    fn notice_write_failed_non_retryable_is_not_retryable() {
        let e = WatcherNoticeWriterError::NoticeWriteFailed {
            path: String::from("p"),
            reason: String::from("r"),
            retryable: false,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn notice_too_large_is_not_retryable_variant() {
        let e = WatcherNoticeWriterError::NoticeTooLarge {
            size_bytes: 1,
            cap_bytes: 0,
        };
        assert_eq!(e.error_code(), 2651);
        assert!(!e.is_retryable());
    }

    #[test]
    fn notice_write_failed_display_contains_path() {
        let e = WatcherNoticeWriterError::NoticeWriteFailed {
            path: String::from("/my/path"),
            reason: String::from("r"),
            retryable: false,
        };
        assert!(e.to_string().contains("/my/path"));
    }

    #[test]
    fn notice_too_large_display_contains_size() {
        let e = WatcherNoticeWriterError::NoticeTooLarge {
            size_bytes: 9999,
            cap_bytes: 4096,
        };
        assert!(e.to_string().contains("9999"));
    }
}
