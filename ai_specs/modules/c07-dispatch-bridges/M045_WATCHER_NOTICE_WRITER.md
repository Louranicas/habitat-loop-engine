# M045 WatcherNoticeWriter — watcher_notice_writer.rs

> **File:** `crates/hle-bridge/src/watcher_notice_writer.rs` | **Target LOC:** ~260 | **Target Tests:** 50
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2650-2651
> **Role:** Append-only file writer that emits Watcher/Hermes notification receipts to `/home/louranicas/projects/shared-context/watcher-notices/`. Path-based, no live network. Every notice write produces a `BridgeReceipt` with SHA-256 of the appended payload. This bridge is the designated channel for HLE to notify The Watcher without routing through a Zellij relay.

---

## Context: Watcher Communication Protocol

The Watcher Communication Protocol (WCP) v1 (`synthex-v2/ai_docs/WATCHER_COMMUNICATION_PROTOCOL.md`) mandates that notices addressed to Weaver/The Watcher are written directly to `~/projects/shared-context/watcher-notices/` without asking the human to relay them (`feedback_wcp_notify_weaver.md`). M045 is the HLE implementation of this protocol's write leg. It uses filesystem append, not HTTP or Zellij dispatch.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `WatcherNoticeWriter` | struct | No | Append-only notice writer; no PhantomData gate — the write is always authorized at this level |
| `NoticePayload` | struct | No | Validated notice content: kind, message, source, timestamp |
| `NoticeKind` | enum | Yes | `Info` / `Warning` / `Blocker` / `HandoffRequest` |
| `NoticeReceipt` | struct | No | SHA-256-tagged confirmation of a notice write; includes filename |
| `WatcherNoticeWriterError` | enum | No | Errors 2650-2651 for write failure and payload-too-large |

---

## NoticeKind

```rust
/// Classification of a Watcher notice.
///
/// `Blocker` indicates the HLE run is paused pending Watcher acknowledgment.
/// `HandoffRequest` signals that the executor wants The Watcher to pick up
/// a specific task or observation stream. Both remain informational from
/// the file-write perspective — the write always succeeds if the filesystem
/// permits; semantic routing is The Watcher's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    Info,
    Warning,
    Blocker,
    HandoffRequest,
}
```

**Traits:** `Display` ("INFO" / "WARNING" / "BLOCKER" / "HANDOFF_REQUEST")

---

## NoticePayload

```rust
/// A validated, bounded notice payload for The Watcher.
///
/// `message` is capped at `NOTICE_MESSAGE_CAP` (4,096 bytes). Longer messages
/// must be split by the caller; there is no silent truncation. `source` is the
/// originating HLE module or executor phase (e.g. "hle.phase_executor.v1").
///
/// Serialized to NDJSON on write: one JSON object per line, terminated by `\n`.
/// This matches the JSONL substrate preference documented in `MEMORY.md`
/// and `reflection_jsonl_substrate_preference.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePayload {
    pub kind: NoticeKind,
    pub message: String,            // capped at NOTICE_MESSAGE_CAP
    pub source: String,             // e.g. "hle.phase_executor.v1"
    pub tick: u64,                  // local Timestamp tick at creation
}

pub const NOTICE_MESSAGE_CAP: usize = 4_096;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(kind: NoticeKind, message: impl Into<String>, source: impl Into<String>, tick: u64) -> Result<Self, WatcherNoticeWriterError>` | Validates message length. |
| `to_ndjson` | `fn(&self) -> String` | `#[must_use]`. Serializes to a single JSON object line with trailing `\n`. Format: `{"kind":"KIND","message":"MSG","source":"SRC","tick":T}`. No external serde dependency — hand-rolled for zero-dep in M0. |
| `byte_len` | `fn(&self) -> usize` | `#[must_use]`. Length of the NDJSON line including trailing newline. |

**Traits:** `Display` ("NoticePayload(KIND source=SRC len=N)")

---

## WatcherNoticeWriter

```rust
/// Append-only file writer for Watcher/Hermes notices.
///
/// Writes to `notices_dir / YYYY-MM-DD.ndjson` using the local tick to
/// derive a date bucket. The directory is created on first write if absent.
/// All writes use `OpenOptions::append(true).create(true)` — POSIX append
/// semantics make small-payload writes atomic without an explicit lock.
///
/// This bridge is NOT parameterized over capability class. The write
/// operation is always authorized at this level — authorization to emit
/// a Watcher notice is embedded in the executor having a valid HLE run
/// context. The `WriteAuthToken` appears only in the `write_notice` call
/// signature to maintain receipt-chain consistency with C01.
///
/// No network calls are made. No background threads are spawned.
/// Every write is synchronous, foreground, and terminates within `timeout`.
#[derive(Debug)]
pub struct WatcherNoticeWriter {
    notices_dir: std::path::PathBuf,
    default_timeout: BoundedDuration,
}

pub const DEFAULT_NOTICES_DIR: &str =
    "/home/louranicas/projects/shared-context/watcher-notices";
pub const NOTICE_FILENAME_DATE_FORMAT: &str = "%Y-%m-%d.ndjson";
```

### BridgeContract impl

```rust
impl BridgeContract for WatcherNoticeWriter {
    fn schema_id(&self) -> &'static str { "hle.watcher_notice.v1" }
    fn port(&self) -> Option<u16> { None }   // filesystem bridge; no TCP port
    fn paths(&self) -> &[&'static str] {
        &["/home/louranicas/projects/shared-context/watcher-notices/"]
    }
    fn supports_write(&self) -> bool { true }
    fn capability_class(&self) -> CapabilityClass { CapabilityClass::ReadOnly } // see Design Notes
    fn name(&self) -> &'static str { "watcher_notice_writer" }
}
```

---

## Method Table

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(timeout: BoundedDuration) -> Self` | Uses `DEFAULT_NOTICES_DIR`. |
| `with_dir` | `fn(dir: std::path::PathBuf, timeout: BoundedDuration) -> Result<Self, WatcherNoticeWriterError>` | For test injection and CI environments. Path must be absolute. |
| `write_notice` | `fn(&self, payload: &NoticePayload, _token: &WriteAuthToken) -> Result<NoticeReceipt, WatcherNoticeWriterError>` | `#[must_use]`. Appends `payload.to_ndjson()` to the date-bucketed file. Creates `notices_dir` if absent. Returns `NoticeReceipt` with SHA-256 of the appended bytes and the resolved filename. Bounded by `self.default_timeout`. |
| `list_notice_files` | `fn(&self) -> Result<Vec<std::path::PathBuf>, WatcherNoticeWriterError>` | `#[must_use]`. Enumerates `*.ndjson` files in `notices_dir` sorted by name descending (most recent first). Returns `Ok(vec![])` when directory is empty. Returns `Err` only on I/O failure. |
| `notice_file_for_tick` | `fn(&self, tick: u64) -> std::path::PathBuf` | `#[must_use]`. Pure function — derives filename from tick without touching the filesystem. |
| `probe_dir` | `fn(&self) -> Result<bool, WatcherNoticeWriterError>` | `#[must_use]`. Returns true iff `notices_dir` exists and is writable. Does not create the directory. |

---

## NoticeReceipt

```rust
/// SHA-256-tagged confirmation of a Watcher notice write.
///
/// `filename` is the absolute path of the file that received the append.
/// `payload_sha256` is the SHA-256 of the NDJSON bytes appended (not the
/// entire file — only the bytes written this call). The SHA chain routes
/// through C01 verifier via `into_bridge_receipt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeReceipt {
    pub filename: std::path::PathBuf,
    pub payload_sha256: [u8; 32],
    pub byte_count: usize,
    pub auth_receipt_id: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `hex_sha` | `fn(&self) -> String` | `#[must_use]`. Lowercase hex of `payload_sha256`. |
| `into_bridge_receipt` | `fn(self) -> BridgeReceipt` | Converts for C01 verifier routing. |

**Traits:** `Display` ("NoticeReceipt(FILE sha=HEXSHORT bytes=N)")

---

## WatcherNoticeWriterError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherNoticeWriterError {
    /// Code 2650. File append or directory creation failed.
    /// `retryable` true for transient I/O conditions (ENOSPC when disk freed,
    /// EAGAIN from NFS). False for permission errors.
    NoticWriteFailed { path: String, reason: String, retryable: bool },
    /// Code 2651. `payload.byte_len()` exceeds `NOTICE_MESSAGE_CAP`.
    NoticeTooLarge { size_bytes: usize, cap_bytes: usize },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2650 or 2651 |
| `is_retryable` | `fn(&self) -> bool` — propagates inner `retryable`; false for 2651 |

**Traits:** `Display` ("[HLE-265N] ..."), `std::error::Error`

---

## Design Notes

- `capability_class()` returns `ReadOnly` in the `BridgeContract` impl despite `supports_write()` returning `true`. This is intentional: the `BridgeContract` capability model describes access to external services; filesystem writes to a local notices directory are not classified as "live external writes" requiring M2+ authorization. The `WriteAuthToken` parameter on `write_notice` preserves receipt-chain consistency without elevating the bridge's capability class.
- Date bucketing uses the local tick for the filename (`YYYY-MM-DD.ndjson`). When the tick counter cannot map to a wall-clock date (e.g. in test environments using `Timestamp::from_raw`), the method falls back to `0000-00-00.ndjson`. This avoids pulling `chrono` or `SystemTime` into M045.
- POSIX `O_APPEND` semantics guarantee that concurrent small writes to the same file do not interleave within a single `write(2)` syscall, provided the payload fits within `PIPE_BUF` (typically 4,096 bytes on Linux). `NOTICE_MESSAGE_CAP` is set to match this limit, preserving atomicity without an explicit file lock.
- The `notices_dir` creation on first write uses `std::fs::create_dir_all`. If concurrent writers race to create the directory, the loser receives `EEXIST`, which the bridge treats as success (idempotent).
- `list_notice_files` is a read-only enumerate operation. It does not require `WriteAuthToken`. Callers use it to inspect notice history from the CLI surface (C08) without needing write authorization.

---

## Cluster Invariants (C07) Enforced by M045

- **I-C07-4:** No `hle-executor` import in `Cargo.toml`.
- **I-C07-5:** File write bounded by `self.default_timeout` via `BoundedDuration`; no unbounded blocking.
- **I-C07-6:** `write_notice` returns `NoticeReceipt` (converts to `BridgeReceipt`); SHA chain is produced unconditionally.

---

## Test Targets (50 minimum)

| Group | Count | Coverage Focus |
|---|---|---|
| `NoticeKind` display variants | 4 | all four kinds, round-trip |
| `NoticePayload` construction | 8 | valid, too-long message, empty source, ndjson format |
| `notice_file_for_tick` pure fn | 4 | tick-to-filename, zero-tick fallback |
| `WatcherNoticeWriter::new` and `with_dir` | 4 | default dir, custom dir, non-absolute-path error |
| `probe_dir` | 4 | exists-writable, absent, permission-denied |
| `write_notice` to tmp dir | 10 | success-SHA, dir-created, payload-sha-matches, second-append |
| `list_notice_files` | 5 | empty, one file, multiple files, sorted order |
| `NoticeReceipt` into_bridge_receipt | 4 | field mapping, hex-sha format |
| Error retryability | 5 | 2650-retryable, 2650-non-retryable, 2651-not-retryable |
| Concurrent-append safety (single-threaded) | 2 | two sequential writes read back as two NDJSON lines |

---

*M045 WatcherNoticeWriter Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
