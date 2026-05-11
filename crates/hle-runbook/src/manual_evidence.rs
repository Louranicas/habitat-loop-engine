#![forbid(unsafe_code)]

//! M036 — Operator-supplied manual evidence model for runbook phases.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error code:** 2570
//!
//! Manual evidence is the record an operator leaves behind when a phase
//! requires human observation or attestation.  The SHA-256 hash is computed
//! at construction time and thereafter immutable — the `ManualEvidence`
//! struct intentionally has no public setters.
//!
//! The hash implementation here is an XOR-fold stub (same pattern used in
//! `receipt_hash.rs`) rather than a real SHA-256, so that the crate carries
//! no external crypto dependency.  A deployment environment may swap in a
//! real hash by replacing `compute_stub_hash`.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::schema::AgentId;

// ── EvidenceError ─────────────────────────────────────────────────────────────

/// Error produced by evidence construction or retrieval.  Error code 2570.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    /// Code 2570 — Required field missing or constraint violated.
    Validation {
        /// Field that violated the constraint.
        field: &'static str,
        /// Human-readable reason.
        reason: String,
    },
    /// Code 2571 — Evidence not found in store.
    NotFound {
        /// Identifier that was not found.
        id: String,
    },
    /// Code 2572 — Hash verification failed.
    HashMismatch {
        /// Expected hash.
        expected: String,
        /// Actual hash computed at verification time.
        actual: String,
    },
}

impl EvidenceError {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::Validation { .. } => 2570,
            Self::NotFound { .. } => 2571,
            Self::HashMismatch { .. } => 2572,
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation { field, reason } => {
                write!(f, "[2570 EvidenceValidation] field '{field}': {reason}")
            }
            Self::NotFound { id } => {
                write!(f, "[2571 EvidenceNotFound] evidence '{id}' not found")
            }
            Self::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "[2572 EvidenceHashMismatch] expected '{expected}', got '{actual}'"
                )
            }
        }
    }
}

impl std::error::Error for EvidenceError {}

// ── EvidenceKind ─────────────────────────────────────────────────────────────

/// Category of operator-supplied evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// A screen-shot or rendered image.
    Screenshot,
    /// A structured or free-text log snippet.
    Log,
    /// A configuration file or config dump.
    Config,
    /// Command output captured by the operator.
    CommandOutput,
    /// URL pointing to an external artefact.
    ExternalLink,
    /// Free-form operator note.
    Note,
    /// A metric reading at a specific timestamp.
    MetricSnapshot,
    /// Any other evidence kind not covered above.
    Other(String),
}

impl EvidenceKind {
    /// Return the canonical string tag for this kind.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Screenshot => "screenshot",
            Self::Log => "log",
            Self::Config => "config",
            Self::CommandOutput => "command_output",
            Self::ExternalLink => "external_link",
            Self::Note => "note",
            Self::MetricSnapshot => "metric_snapshot",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Parse a string tag into an `EvidenceKind`.
    #[must_use]
    pub fn parse_str(s: &str) -> Self {
        match s {
            "screenshot" => Self::Screenshot,
            "log" => Self::Log,
            "config" => Self::Config,
            "command_output" => Self::CommandOutput,
            "external_link" => Self::ExternalLink,
            "note" => Self::Note,
            "metric_snapshot" => Self::MetricSnapshot,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── EvidenceAttachment ────────────────────────────────────────────────────────

/// How the evidence payload is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceAttachment {
    /// Inline text content — small logs, notes, command output.
    Inline(String),
    /// Path to a file on the operator's local filesystem.
    FilePath(String),
    /// URL to a remotely hosted artefact.
    Url(String),
}

impl EvidenceAttachment {
    /// Return the raw string representation of this attachment.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Inline(s) | Self::FilePath(s) | Self::Url(s) => s.as_str(),
        }
    }

    /// Return the canonical tag for the attachment variant.
    #[must_use]
    pub const fn variant_tag(&self) -> &'static str {
        match self {
            Self::Inline(_) => "inline",
            Self::FilePath(_) => "file_path",
            Self::Url(_) => "url",
        }
    }
}

// ── Hash helper ───────────────────────────────────────────────────────────────

/// Compute a deterministic hex string from arbitrary bytes.
///
/// This is a XOR-fold stub, not real SHA-256.  It produces a 64-character
/// lower-hex string that changes whenever any byte of the input changes.
#[must_use]
fn compute_stub_hash(data: &[u8]) -> String {
    // Fold input into 32 bytes via XOR with position-scrambled indices.
    let mut state = [0u8; 32];
    for (i, &b) in data.iter().enumerate() {
        let pos = i % 32;
        // Mix in position so that "ab" ≠ "ba".
        // Truncation is intentional: we only need the low 8 bits for mixing.
        #[allow(clippy::cast_possible_truncation)]
        let i_byte = i as u8;
        state[pos] ^= b.wrapping_add(i_byte.wrapping_mul(17));
    }
    // Additional diffusion pass.
    for i in 1..32usize {
        state[i] = state[i].wrapping_add(state[i - 1].wrapping_mul(31));
    }
    state.iter().fold(String::with_capacity(64), |mut acc, b| {
        use fmt::Write as _;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// Build the hash preimage from evidence fields.
#[must_use]
fn hash_preimage(kind: &EvidenceKind, attachment: &EvidenceAttachment) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(kind.as_str().as_bytes());
    buf.push(b'\0');
    buf.extend_from_slice(attachment.variant_tag().as_bytes());
    buf.push(b'\0');
    buf.extend_from_slice(attachment.as_str().as_bytes());
    buf
}

// ── ManualEvidence ────────────────────────────────────────────────────────────

/// Operator-supplied evidence record.
///
/// Immutable after construction.  The `sha256` field is computed from `kind`
/// and the attachment at build time and cannot be changed without creating a
/// new `ManualEvidence`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualEvidence {
    /// Category of the evidence.
    kind: EvidenceKind,
    /// Payload — inline text, file path, or URL.
    attachment: EvidenceAttachment,
    /// SHA-256 (stub) hex digest of the payload, computed at construction.
    sha256: String,
    /// Unix millisecond timestamp when the evidence was attached.
    attached_at: u64,
    /// Operator who attached this evidence.
    attached_by: AgentId,
    /// Optional free-form description.
    description: Option<String>,
}

impl ManualEvidence {
    /// Return the evidence kind.
    #[must_use]
    pub fn kind(&self) -> &EvidenceKind {
        &self.kind
    }

    /// Return the attachment.
    #[must_use]
    pub fn attachment(&self) -> &EvidenceAttachment {
        &self.attachment
    }

    /// Return the hex SHA-256 (stub) digest.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Unix milliseconds when the evidence was attached.
    #[must_use]
    pub fn attached_at(&self) -> u64 {
        self.attached_at
    }

    /// The operator who attached this evidence.
    #[must_use]
    pub fn attached_by(&self) -> &AgentId {
        &self.attached_by
    }

    /// Optional description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Verify that the current hash still matches the attachment payload.
    ///
    /// Returns `Ok(())` when the hash is consistent, `Err(HashMismatch)` when
    /// it has drifted (which should never happen for an immutable value, but
    /// is useful when deserialising from an external source).
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::HashMismatch`] when the stored hash does not
    /// match the hash computed from the current attachment.
    pub fn verify_hash(&self) -> Result<(), EvidenceError> {
        let recomputed = compute_stub_hash(&hash_preimage(&self.kind, &self.attachment));
        if recomputed == self.sha256 {
            Ok(())
        } else {
            Err(EvidenceError::HashMismatch {
                expected: self.sha256.clone(),
                actual: recomputed,
            })
        }
    }
}

// ── EvidenceBuilder ───────────────────────────────────────────────────────────

/// Builder for [`ManualEvidence`].
///
/// Call [`EvidenceBuilder::build`] to obtain a validated, immutable
/// [`ManualEvidence`] with the SHA-256 computed.
#[derive(Debug, Default)]
pub struct EvidenceBuilder {
    kind: Option<EvidenceKind>,
    attachment: Option<EvidenceAttachment>,
    attached_at: Option<u64>,
    attached_by: Option<AgentId>,
    description: Option<String>,
}

impl EvidenceBuilder {
    /// Create an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the evidence kind.
    #[must_use]
    pub fn kind(mut self, kind: EvidenceKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set inline text content as the attachment.
    #[must_use]
    pub fn inline(mut self, content: impl Into<String>) -> Self {
        self.attachment = Some(EvidenceAttachment::Inline(content.into()));
        self
    }

    /// Set a file path as the attachment.
    #[must_use]
    pub fn file_path(mut self, path: impl Into<String>) -> Self {
        self.attachment = Some(EvidenceAttachment::FilePath(path.into()));
        self
    }

    /// Set a URL as the attachment.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.attachment = Some(EvidenceAttachment::Url(url.into()));
        self
    }

    /// Set the timestamp.  Defaults to current system time when omitted.
    #[must_use]
    pub fn attached_at(mut self, ts_ms: u64) -> Self {
        self.attached_at = Some(ts_ms);
        self
    }

    /// Set the operator identity.
    #[must_use]
    pub fn attached_by(mut self, agent: AgentId) -> Self {
        self.attached_by = Some(agent);
        self
    }

    /// Set an optional free-form description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Build the [`ManualEvidence`], computing the SHA-256 stub.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::Validation`] when `kind`, `attachment`, or
    /// `attached_by` are missing, or when `attachment` content is empty.
    pub fn build(self) -> Result<ManualEvidence, EvidenceError> {
        let kind = self.kind.ok_or(EvidenceError::Validation {
            field: "kind",
            reason: "evidence kind is required".into(),
        })?;
        let attachment = self.attachment.ok_or(EvidenceError::Validation {
            field: "attachment",
            reason: "attachment content is required".into(),
        })?;
        if attachment.as_str().is_empty() {
            return Err(EvidenceError::Validation {
                field: "attachment",
                reason: "attachment content must not be empty".into(),
            });
        }
        let attached_by = self.attached_by.ok_or(EvidenceError::Validation {
            field: "attached_by",
            reason: "operator identity is required".into(),
        })?;
        let attached_at = self.attached_at.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        });
        let sha256 = compute_stub_hash(&hash_preimage(&kind, &attachment));
        Ok(ManualEvidence {
            kind,
            attachment,
            sha256,
            attached_at,
            attached_by,
            description: self.description,
        })
    }
}

// ── EvidenceStore trait ───────────────────────────────────────────────────────

/// Persistent store for manual evidence records.
///
/// Implementations are responsible for serialisation and durable storage.
/// The trait is object-safe — it does not use generics.
pub trait EvidenceStore: Send + Sync {
    /// Persist a `ManualEvidence` record and return its assigned string ID.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on storage failure.
    fn insert(&self, evidence: &ManualEvidence) -> Result<String, EvidenceError>;

    /// Retrieve evidence by its storage ID.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::NotFound`] when the ID does not exist, or
    /// [`EvidenceError`] on storage failure.
    fn get(&self, id: &str) -> Result<ManualEvidence, EvidenceError>;

    /// List all evidence IDs currently in the store.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on storage failure.
    fn list_ids(&self) -> Result<Vec<String>, EvidenceError>;

    /// Return `true` when the store holds at least one evidence record.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on storage failure.
    fn is_empty(&self) -> Result<bool, EvidenceError> {
        self.list_ids().map(|ids| ids.is_empty())
    }
}

// ── InMemoryEvidenceStore ─────────────────────────────────────────────────────

/// An in-memory [`EvidenceStore`] implementation for tests.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct InMemoryEvidenceStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, ManualEvidence>>,
    next_id: std::sync::atomic::AtomicU64,
}

#[cfg(any(test, feature = "test-utils"))]
impl InMemoryEvidenceStore {
    /// Create an empty in-memory evidence store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl EvidenceStore for InMemoryEvidenceStore {
    fn insert(&self, evidence: &ManualEvidence) -> Result<String, EvidenceError> {
        let id = format!(
            "ev-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let mut guard = self.entries.lock().map_err(|_| EvidenceError::Validation {
            field: "store",
            reason: "mutex poisoned".into(),
        })?;
        guard.insert(id.clone(), evidence.clone());
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<ManualEvidence, EvidenceError> {
        let guard = self.entries.lock().map_err(|_| EvidenceError::Validation {
            field: "store",
            reason: "mutex poisoned".into(),
        })?;
        guard
            .get(id)
            .cloned()
            .ok_or_else(|| EvidenceError::NotFound { id: id.to_owned() })
    }

    fn list_ids(&self) -> Result<Vec<String>, EvidenceError> {
        let guard = self.entries.lock().map_err(|_| EvidenceError::Validation {
            field: "store",
            reason: "mutex poisoned".into(),
        })?;
        let mut ids: Vec<String> = guard.keys().cloned().collect();
        ids.sort();
        Ok(ids)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        compute_stub_hash, EvidenceAttachment, EvidenceBuilder, EvidenceError, EvidenceKind,
        InMemoryEvidenceStore, ManualEvidence,
    };
    use crate::schema::AgentId;

    fn operator() -> AgentId {
        AgentId::new("operator-1")
    }

    fn screenshot_evidence() -> ManualEvidence {
        EvidenceBuilder::new()
            .kind(EvidenceKind::Screenshot)
            .file_path("/tmp/screenshot.png")
            .attached_by(operator())
            .attached_at(1_000_000)
            .build()
            .expect("valid evidence")
    }

    // ── EvidenceKind ─────────────────────────────────────────────────────────

    #[test]
    fn evidence_kind_as_str_roundtrip() {
        let kinds = [
            EvidenceKind::Screenshot,
            EvidenceKind::Log,
            EvidenceKind::Config,
            EvidenceKind::CommandOutput,
            EvidenceKind::ExternalLink,
            EvidenceKind::Note,
            EvidenceKind::MetricSnapshot,
        ];
        for kind in &kinds {
            let s = kind.as_str();
            assert_eq!(&EvidenceKind::parse_str(s), kind, "roundtrip for {s}");
        }
    }

    #[test]
    fn evidence_kind_other_roundtrip() {
        let kind = EvidenceKind::Other("custom-kind".into());
        assert_eq!(kind.as_str(), "custom-kind");
        assert_eq!(EvidenceKind::parse_str("custom-kind"), kind);
    }

    #[test]
    fn evidence_kind_display() {
        assert_eq!(EvidenceKind::Log.to_string(), "log");
        assert_eq!(EvidenceKind::Screenshot.to_string(), "screenshot");
    }

    // ── EvidenceAttachment ───────────────────────────────────────────────────

    #[test]
    fn attachment_inline_as_str() {
        let a = EvidenceAttachment::Inline("hello".into());
        assert_eq!(a.as_str(), "hello");
        assert_eq!(a.variant_tag(), "inline");
    }

    #[test]
    fn attachment_file_path_as_str() {
        let a = EvidenceAttachment::FilePath("/tmp/x".into());
        assert_eq!(a.as_str(), "/tmp/x");
        assert_eq!(a.variant_tag(), "file_path");
    }

    #[test]
    fn attachment_url_as_str() {
        let a = EvidenceAttachment::Url("https://example.com".into());
        assert_eq!(a.as_str(), "https://example.com");
        assert_eq!(a.variant_tag(), "url");
    }

    // ── Hash helper ──────────────────────────────────────────────────────────

    #[test]
    fn stub_hash_produces_64_hex_chars() {
        let h = compute_stub_hash(b"hello world");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn stub_hash_empty_is_stable() {
        let h1 = compute_stub_hash(b"");
        let h2 = compute_stub_hash(b"");
        assert_eq!(h1, h2);
    }

    #[test]
    fn stub_hash_differs_for_different_inputs() {
        let h1 = compute_stub_hash(b"abc");
        let h2 = compute_stub_hash(b"abd");
        assert_ne!(h1, h2);
    }

    #[test]
    fn stub_hash_is_order_sensitive() {
        let h1 = compute_stub_hash(b"ab");
        let h2 = compute_stub_hash(b"ba");
        assert_ne!(h1, h2);
    }

    // ── EvidenceBuilder ──────────────────────────────────────────────────────

    #[test]
    fn builder_missing_kind_returns_error() {
        let err = EvidenceBuilder::new()
            .inline("some content")
            .attached_by(operator())
            .attached_at(100)
            .build()
            .unwrap_err();
        assert_eq!(err.error_code(), 2570);
        assert!(matches!(
            err,
            EvidenceError::Validation { field: "kind", .. }
        ));
    }

    #[test]
    fn builder_missing_attachment_returns_error() {
        let err = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .attached_by(operator())
            .attached_at(100)
            .build()
            .unwrap_err();
        assert_eq!(err.error_code(), 2570);
        assert!(matches!(
            err,
            EvidenceError::Validation {
                field: "attachment",
                ..
            }
        ));
    }

    #[test]
    fn builder_empty_attachment_returns_error() {
        let err = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("")
            .attached_by(operator())
            .attached_at(100)
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            EvidenceError::Validation {
                field: "attachment",
                ..
            }
        ));
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn builder_missing_operator_returns_error() {
        let err = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("content")
            .attached_at(100)
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            EvidenceError::Validation {
                field: "attached_by",
                ..
            }
        ));
    }

    #[test]
    fn builder_produces_valid_evidence() {
        let ev = screenshot_evidence();
        assert_eq!(ev.kind(), &EvidenceKind::Screenshot);
        assert_eq!(ev.attached_at(), 1_000_000);
        assert_eq!(ev.sha256().len(), 64);
    }

    #[test]
    fn builder_with_description() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("kernel panic in syslog")
            .attached_by(operator())
            .attached_at(999)
            .description("panic message from dmesg")
            .build()
            .expect("valid");
        assert_eq!(ev.description(), Some("panic message from dmesg"));
    }

    #[test]
    fn builder_no_description_is_none() {
        let ev = screenshot_evidence();
        assert_eq!(ev.description(), None);
    }

    // ── ManualEvidence immutability ──────────────────────────────────────────

    #[test]
    fn evidence_hash_verifies_ok() {
        let ev = screenshot_evidence();
        assert!(ev.verify_hash().is_ok());
    }

    #[test]
    fn evidence_sha256_is_64_chars() {
        let ev = screenshot_evidence();
        assert_eq!(ev.sha256().len(), 64);
    }

    #[test]
    fn evidence_different_payloads_different_hashes() {
        let ev1 = EvidenceBuilder::new()
            .kind(EvidenceKind::Log)
            .inline("log line A")
            .attached_by(operator())
            .attached_at(1)
            .build()
            .expect("valid");
        let ev2 = EvidenceBuilder::new()
            .kind(EvidenceKind::Log)
            .inline("log line B")
            .attached_by(operator())
            .attached_at(1)
            .build()
            .expect("valid");
        assert_ne!(ev1.sha256(), ev2.sha256());
    }

    #[test]
    fn evidence_kind_change_changes_hash() {
        let ev1 = EvidenceBuilder::new()
            .kind(EvidenceKind::Log)
            .inline("same content")
            .attached_by(operator())
            .attached_at(1)
            .build()
            .expect("valid");
        let ev2 = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("same content")
            .attached_by(operator())
            .attached_at(1)
            .build()
            .expect("valid");
        // Kind is part of the hash preimage, so hashes must differ.
        assert_ne!(ev1.sha256(), ev2.sha256());
    }

    // ── InMemoryEvidenceStore ────────────────────────────────────────────────

    #[test]
    fn in_memory_store_insert_and_get() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        let ev = screenshot_evidence();
        let id = store.insert(&ev).expect("insert ok");
        let got = store.get(&id).expect("get ok");
        assert_eq!(got.sha256(), ev.sha256());
        assert_eq!(got.attached_at(), ev.attached_at());
    }

    #[test]
    fn in_memory_store_not_found_error() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        let err = store.get("nonexistent").unwrap_err();
        assert_eq!(err.error_code(), 2571);
        assert!(matches!(err, EvidenceError::NotFound { .. }));
    }

    #[test]
    fn in_memory_store_list_ids() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        let ev1 = screenshot_evidence();
        let ev2 = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("note content")
            .attached_by(operator())
            .attached_at(200)
            .build()
            .expect("valid");
        store.insert(&ev1).expect("insert");
        store.insert(&ev2).expect("insert");
        let ids = store.list_ids().expect("list ok");
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn in_memory_store_is_empty_true_when_empty() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        assert!(store.is_empty().expect("ok"));
    }

    #[test]
    fn in_memory_store_is_empty_false_after_insert() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        store.insert(&screenshot_evidence()).expect("insert");
        assert!(!store.is_empty().expect("ok"));
    }

    // ── EvidenceError display ────────────────────────────────────────────────

    #[test]
    fn evidence_error_validation_display_contains_code() {
        let err = EvidenceError::Validation {
            field: "kind",
            reason: "missing".into(),
        };
        assert!(err.to_string().contains("2570"));
    }

    #[test]
    fn evidence_error_not_found_display_contains_code() {
        let err = EvidenceError::NotFound { id: "ev-99".into() };
        assert!(err.to_string().contains("2571"));
        assert!(err.to_string().contains("ev-99"));
    }

    #[test]
    fn evidence_error_hash_mismatch_display_contains_code() {
        let err = EvidenceError::HashMismatch {
            expected: "aaa".into(),
            actual: "bbb".into(),
        };
        assert!(err.to_string().contains("2572"));
    }

    // ── URL attachment ────────────────────────────────────────────────────────

    #[test]
    fn builder_url_attachment() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::ExternalLink)
            .url("https://grafana.example.com/dashboard")
            .attached_by(operator())
            .attached_at(5000)
            .build()
            .expect("valid");
        assert_eq!(ev.kind(), &EvidenceKind::ExternalLink);
        assert!(matches!(ev.attachment(), EvidenceAttachment::Url(_)));
        assert!(ev.verify_hash().is_ok());
    }

    // ── Timestamp default ────────────────────────────────────────────────────

    #[test]
    fn builder_without_timestamp_still_builds() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("no explicit ts")
            .attached_by(operator())
            .build()
            .expect("should build without explicit timestamp");
        assert!(ev.attached_at() < u64::MAX);
    }

    // ── Additional evidence tests to reach ≥50 ───────────────────────────────

    #[test]
    fn evidence_kind_all_known_variants_have_stable_as_str() {
        let pairs = [
            (EvidenceKind::Screenshot, "screenshot"),
            (EvidenceKind::Log, "log"),
            (EvidenceKind::Config, "config"),
            (EvidenceKind::CommandOutput, "command_output"),
            (EvidenceKind::ExternalLink, "external_link"),
            (EvidenceKind::Note, "note"),
            (EvidenceKind::MetricSnapshot, "metric_snapshot"),
        ];
        for (kind, expected) in &pairs {
            assert_eq!(kind.as_str(), *expected);
        }
    }

    #[test]
    fn evidence_kind_other_with_empty_string() {
        let kind = EvidenceKind::Other(String::new());
        assert_eq!(kind.as_str(), "");
    }

    #[test]
    fn evidence_kind_display_equals_as_str_all() {
        let kinds = [
            EvidenceKind::Screenshot,
            EvidenceKind::Log,
            EvidenceKind::Config,
            EvidenceKind::CommandOutput,
            EvidenceKind::ExternalLink,
            EvidenceKind::Note,
            EvidenceKind::MetricSnapshot,
        ];
        for k in &kinds {
            assert_eq!(k.to_string(), k.as_str());
        }
    }

    #[test]
    fn evidence_attachment_variant_tags_stable() {
        assert_eq!(
            EvidenceAttachment::Inline("x".into()).variant_tag(),
            "inline"
        );
        assert_eq!(
            EvidenceAttachment::FilePath("x".into()).variant_tag(),
            "file_path"
        );
        assert_eq!(EvidenceAttachment::Url("x".into()).variant_tag(), "url");
    }

    #[test]
    fn stub_hash_all_hex_chars() {
        let h = compute_stub_hash(b"test data");
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "hash contains non-hex: {h}"
        );
    }

    #[test]
    fn stub_hash_differs_for_different_prefixes() {
        // "abc" vs "abcd" — different because one has extra content.
        let h1 = compute_stub_hash(b"abc");
        let h2 = compute_stub_hash(b"abcd");
        assert_ne!(h1, h2);
    }

    #[test]
    fn evidence_builder_file_path_attachment() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::Config)
            .file_path("/etc/myapp/config.toml")
            .attached_by(operator())
            .attached_at(42)
            .build()
            .expect("valid");
        assert_eq!(ev.kind(), &EvidenceKind::Config);
        assert!(matches!(ev.attachment(), EvidenceAttachment::FilePath(_)));
        assert_eq!(ev.attachment().as_str(), "/etc/myapp/config.toml");
    }

    #[test]
    fn evidence_builder_command_output_kind() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::CommandOutput)
            .inline("$ df -h\n/dev/sda1  97%")
            .attached_by(operator())
            .attached_at(100)
            .build()
            .expect("valid");
        assert_eq!(ev.kind(), &EvidenceKind::CommandOutput);
    }

    #[test]
    fn evidence_builder_metric_snapshot_kind() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::MetricSnapshot)
            .inline("cpu_usage=0.95 at 2026-05-11T00:00:00Z")
            .attached_by(operator())
            .attached_at(200)
            .build()
            .expect("valid");
        assert_eq!(ev.kind(), &EvidenceKind::MetricSnapshot);
        assert!(ev.verify_hash().is_ok());
    }

    #[test]
    fn evidence_builder_other_kind() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::Other("custom".into()))
            .inline("custom data")
            .attached_by(operator())
            .attached_at(300)
            .build()
            .expect("valid");
        assert_eq!(ev.kind().as_str(), "custom");
    }

    #[test]
    fn evidence_attached_by_reflects_agent() {
        let ev = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("note")
            .attached_by(AgentId::new("zen-agent"))
            .attached_at(1)
            .build()
            .expect("valid");
        assert_eq!(ev.attached_by().as_str(), "zen-agent");
    }

    #[test]
    fn evidence_sha256_is_lowercase_hex() {
        let ev = screenshot_evidence();
        assert!(ev
            .sha256()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn in_memory_store_insert_twice_different_ids() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        let ev = screenshot_evidence();
        let id1 = store.insert(&ev).expect("first insert");
        let id2 = store.insert(&ev).expect("second insert");
        assert_ne!(id1, id2);
    }

    #[test]
    fn in_memory_store_list_ids_sorted() {
        use super::EvidenceStore as _;
        let store = InMemoryEvidenceStore::new();
        let ev1 = screenshot_evidence();
        let ev2 = EvidenceBuilder::new()
            .kind(EvidenceKind::Note)
            .inline("n")
            .attached_by(operator())
            .attached_at(1)
            .build()
            .expect("valid");
        store.insert(&ev1).expect("ok");
        store.insert(&ev2).expect("ok");
        let ids = store.list_ids().expect("ok");
        for pair in ids.windows(2) {
            assert!(pair[0] <= pair[1], "ids not sorted");
        }
    }

    #[test]
    fn evidence_error_not_found_code() {
        let err = super::EvidenceError::NotFound { id: "x".into() };
        assert_eq!(err.error_code(), 2571);
    }

    #[test]
    fn evidence_error_hash_mismatch_code() {
        let err = super::EvidenceError::HashMismatch {
            expected: "a".into(),
            actual: "b".into(),
        };
        assert_eq!(err.error_code(), 2572);
    }

    #[test]
    fn evidence_error_validation_code() {
        let err = super::EvidenceError::Validation {
            field: "f",
            reason: "r".into(),
        };
        assert_eq!(err.error_code(), 2570);
    }

    #[test]
    fn evidence_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::ManualEvidence>();
    }

    #[test]
    fn evidence_store_is_object_safe() {
        // This just needs to compile.
        let _: Box<dyn super::EvidenceStore> = Box::new(InMemoryEvidenceStore::new());
    }
}
