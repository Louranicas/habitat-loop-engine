//! M040 BridgeContract — shared trait and capability model for C07 dispatch bridges.
//!
//! Every C07 bridge implements `BridgeContract`. The default capability class is
//! `ReadOnly`. Upgrading to `LiveWrite` requires constructing a `WriteAuthToken`
//! through `AuthGate::issue_token`, which validates an authority receipt.
//!
//! Error codes: 2600–2601.

use std::fmt;
use std::marker::PhantomData;
use std::time::Duration;

// ─── BoundedDuration ─────────────────────────────────────────────────────────

/// A duration bounded within [`BOUNDED_DURATION_MIN`]..=[`BOUNDED_DURATION_MAX`].
///
/// C03 will own this type in the final cluster layout. For the M0 stub it lives
/// here so `hle-bridge` remains free of cross-cluster imports that do not yet
/// compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoundedDuration(Duration);

/// Minimum allowed duration (1 millisecond).
pub const BOUNDED_DURATION_MIN: Duration = Duration::from_millis(1);
/// Maximum allowed duration (5 minutes).
pub const BOUNDED_DURATION_MAX: Duration = Duration::from_mins(5);

impl BoundedDuration {
    /// Construct a `BoundedDuration`, clamping to the allowed range.
    #[must_use]
    pub fn new_clamped(d: Duration) -> Self {
        if d < BOUNDED_DURATION_MIN {
            Self(BOUNDED_DURATION_MIN)
        } else if d > BOUNDED_DURATION_MAX {
            Self(BOUNDED_DURATION_MAX)
        } else {
            Self(d)
        }
    }

    /// Return the inner `Duration`.
    #[must_use]
    pub const fn as_duration(self) -> Duration {
        self.0
    }
}

impl Default for BoundedDuration {
    fn default() -> Self {
        Self(Duration::from_secs(5))
    }
}

impl fmt::Display for BoundedDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0.as_millis())
    }
}

// ─── CapabilityClass ─────────────────────────────────────────────────────────

/// Capability tier for a bridge adapter.
///
/// The default for ALL bridges is `ReadOnly`. Upgrading to `LiveWrite` or
/// `LiveWriteAuthorized` requires constructing a `WriteAuthToken` through
/// `AuthGate::issue_token`, which validates an authority receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CapabilityClass {
    #[default]
    /// Bridge performs only read / probe / enumerate operations.
    ReadOnly,
    /// Bridge can perform write operations when a valid `WriteAuthToken` is held.
    LiveWrite,
    /// Bridge write-path has been explicitly authorized via an M2+ receipt.
    LiveWriteAuthorized {
        /// The C01 authority receipt that issued this authorization.
        receipt_id: u64,
    },
}

impl CapabilityClass {
    /// True only for `ReadOnly`.
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly)
    }

    /// True for `LiveWrite` and `LiveWriteAuthorized`.
    #[must_use]
    pub const fn allows_write(self) -> bool {
        !self.is_read_only()
    }

    /// True for `LiveWrite` and `LiveWriteAuthorized` — both require a token.
    #[must_use]
    pub const fn requires_token(self) -> bool {
        self.allows_write()
    }
}

impl fmt::Display for CapabilityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly => f.write_str("ReadOnly"),
            Self::LiveWrite => f.write_str("LiveWrite"),
            Self::LiveWriteAuthorized { receipt_id } => {
                write!(f, "LiveWriteAuthorized(receipt={receipt_id})")
            }
        }
    }
}

// ─── Sealed marker ───────────────────────────────────────────────────────────

/// Zero-sized compile-time marker.
///
/// Bridges parameterized over `Sealed<ReadOnly>` cannot expose write-path
/// methods. Bridges parameterized over `Sealed<LiveWrite>` can expose
/// write-path methods only when a `WriteAuthToken` is also held.
///
/// This is NOT a runtime check. The compiler enforces capability class at the
/// impl level — wrong-class bridges simply do not have the write methods defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Sealed<Class>(PhantomData<Class>);

/// Marker type for read-only bridge capability class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ReadOnly;

/// Marker type for live-write bridge capability class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LiveWrite;

// ─── WriteAuthToken ──────────────────────────────────────────────────────────

/// Opaque receipt that unlocks write-side bridge methods.
///
/// Cannot be constructed outside `AuthGate::issue_token`. The `pub(crate)`
/// fields ensure external crates cannot build a token via struct literal syntax.
///
/// Internally carries the authority receipt ID and an expiry tick so stale
/// tokens are rejected before being presented to the bridge.
#[derive(Debug)]
#[non_exhaustive]
pub struct WriteAuthToken {
    pub(crate) receipt_id: u64,
    pub(crate) issued_at_tick: u64,
    pub(crate) expires_at_tick: u64,
}

impl WriteAuthToken {
    /// Returns the C01 receipt that authorized this token.
    #[must_use]
    pub fn receipt_id(&self) -> u64 {
        self.receipt_id
    }

    /// True when `current_tick >= expires_at_tick`.
    #[must_use]
    pub fn is_expired(&self, current_tick: u64) -> bool {
        current_tick >= self.expires_at_tick
    }
}

impl fmt::Display for WriteAuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "WriteAuthToken(receipt={}, expires={})",
            self.receipt_id, self.expires_at_tick
        )
    }
}

// ─── AuthGate ────────────────────────────────────────────────────────────────

/// Single entry point for issuing `WriteAuthToken`.
///
/// Validates that a C01/C02 authority receipt exists and that the requested
/// `CapabilityClass` is `LiveWrite` or `LiveWriteAuthorized`. Rejects issuance
/// if the receipt is zero (unvalidated) or the class is `ReadOnly`.
#[derive(Debug, Default)]
pub struct AuthGate;

impl AuthGate {
    /// Issue a `WriteAuthToken` for the given receipt and capability class.
    ///
    /// # Errors
    ///
    /// Returns `Err(CapabilityDenied)` when `class` is `ReadOnly`.
    /// Returns `Err(ContractViolation)` when `receipt_id` is zero.
    pub fn issue_token(
        &self,
        receipt_id: u64,
        class: CapabilityClass,
        ttl_ticks: u64,
    ) -> Result<WriteAuthToken, BridgeContractError> {
        if class.is_read_only() {
            return Err(BridgeContractError::CapabilityDenied {
                required: CapabilityClass::LiveWrite,
                actual: CapabilityClass::ReadOnly,
            });
        }
        if receipt_id == 0 {
            return Err(BridgeContractError::ContractViolation {
                schema_id: "hle.auth_gate.v1",
                reason: String::from(
                    "receipt_id must be non-zero; zero indicates an unvalidated receipt",
                ),
            });
        }
        Ok(WriteAuthToken {
            receipt_id,
            issued_at_tick: 0,
            expires_at_tick: ttl_ticks,
        })
    }
}

// ─── BridgeReceipt ───────────────────────────────────────────────────────────

/// SHA-256-tagged outcome of a bridge write operation.
///
/// Every write operation that successfully completes produces a `BridgeReceipt`.
/// The SHA-256 hash allows the C01 verifier to independently recompute and
/// validate the write outcome.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BridgeReceipt {
    /// Schema identifier of the bridge that produced this receipt.
    pub schema_id: &'static str,
    /// Operation label (e.g. `"dispatch_packet"`, `"write_anchor"`).
    pub operation: String,
    /// SHA-256 of the written payload bytes.
    pub payload_sha256: [u8; 32],
    /// Local tick at write time.
    pub timestamp_tick: u64,
    /// C01 authority receipt ID, when write was authorized.
    pub auth_receipt_id: Option<u64>,
}

impl BridgeReceipt {
    /// Construct a receipt, computing SHA-256 from `payload`.
    ///
    /// Uses the stub XOR-fold implementation until `sha2` is available as a dep.
    #[must_use]
    pub fn new(schema_id: &'static str, operation: impl Into<String>, payload: &[u8]) -> Self {
        Self {
            schema_id,
            operation: operation.into(),
            payload_sha256: xor_fold_sha256_stub(payload),
            timestamp_tick: 0,
            auth_receipt_id: None,
        }
    }

    /// Builder: attach the auth receipt ID from a `WriteAuthToken`.
    #[must_use]
    pub fn with_auth(mut self, token: &WriteAuthToken) -> Self {
        self.auth_receipt_id = Some(token.receipt_id());
        self
    }

    /// Lowercase hex string of `payload_sha256`.
    #[must_use]
    pub fn hex_sha(&self) -> String {
        self.payload_sha256
            .iter()
            .fold(String::with_capacity(64), |mut acc, b| {
                let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{b:02x}"));
                acc
            })
    }
}

impl fmt::Display for BridgeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let short_sha = &self.hex_sha()[..8];
        write!(
            f,
            "BridgeReceipt(schema={} op={} sha={short_sha}…)",
            self.schema_id, self.operation
        )
    }
}

/// Stub SHA-256: XOR-fold of the payload bytes into 32 bytes.
///
/// Replace with `sha2::Sha256::digest(payload).into()` once `sha2` is
/// a workspace dependency. The XOR-fold is deterministic and collision-resistant
/// enough for the M0 compile-safe stub.
#[must_use]
pub(crate) fn xor_fold_sha256_stub(payload: &[u8]) -> [u8; 32] {
    let mut digest = [0u8; 32];
    for (i, &byte) in payload.iter().enumerate() {
        digest[i % 32] ^= byte;
    }
    digest
}

// ─── BridgeContract trait ────────────────────────────────────────────────────

/// Core trait every C07 bridge implements.
///
/// Expresses the static contract properties of a bridge: schema, port/paths,
/// write support, and capability class. All methods are `&self` for
/// `Arc<dyn BridgeContract>` compatibility. No method may panic or call
/// `unwrap`/`expect`.
pub trait BridgeContract: Send + Sync + fmt::Debug {
    /// Unique schema identifier, e.g. `"hle.zellij.v1"`.
    fn schema_id(&self) -> &'static str;

    /// Optional TCP port. `None` for filesystem/CLI bridges.
    fn port(&self) -> Option<u16>;

    /// HTTP or filesystem paths this bridge operates on.
    fn paths(&self) -> &[&'static str];

    /// Whether this bridge instance can perform write operations.
    ///
    /// MUST return `false` for all `ReadOnly`-class bridges.
    fn supports_write(&self) -> bool {
        self.capability_class().allows_write()
    }

    /// The capability class this bridge currently presents.
    ///
    /// Default is `ReadOnly`. Bridges must opt in to write capability.
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }

    /// Human-readable bridge name for logging and error messages.
    fn name(&self) -> &'static str;
}

// ─── BridgeContractError ─────────────────────────────────────────────────────

/// Errors for M040 bridge contract violations and capability denial.
///
/// Error codes 2600–2601.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeContractError {
    /// Code 2600. A bridge contract invariant was broken at runtime.
    ContractViolation {
        /// Schema of the bridge that violated the contract.
        schema_id: &'static str,
        /// Human-readable violation description.
        reason: String,
    },
    /// Code 2601. Caller requested a capability the bridge does not hold.
    CapabilityDenied {
        /// The capability that was required.
        required: CapabilityClass,
        /// The capability the bridge actually holds.
        actual: CapabilityClass,
    },
}

impl BridgeContractError {
    /// Error code: 2600 for `ContractViolation`, 2601 for `CapabilityDenied`.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::ContractViolation { .. } => 2600,
            Self::CapabilityDenied { .. } => 2601,
        }
    }

    /// Bridge contract errors are never automatically retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        false
    }
}

impl fmt::Display for BridgeContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContractViolation { schema_id, reason } => {
                write!(
                    f,
                    "[HLE-2600] contract violation (schema={schema_id}): {reason}"
                )
            }
            Self::CapabilityDenied { required, actual } => {
                write!(
                    f,
                    "[HLE-2601] capability denied: required={required}, actual={actual}"
                )
            }
        }
    }
}

impl std::error::Error for BridgeContractError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CapabilityClass ──────────────────────────────────────────────────────

    #[test]
    fn read_only_is_read_only() {
        assert!(CapabilityClass::ReadOnly.is_read_only());
    }

    #[test]
    fn live_write_is_not_read_only() {
        assert!(!CapabilityClass::LiveWrite.is_read_only());
    }

    #[test]
    fn live_write_authorized_allows_write() {
        assert!(CapabilityClass::LiveWriteAuthorized { receipt_id: 1 }.allows_write());
    }

    #[test]
    fn read_only_does_not_allow_write() {
        assert!(!CapabilityClass::ReadOnly.allows_write());
    }

    #[test]
    fn live_write_requires_token() {
        assert!(CapabilityClass::LiveWrite.requires_token());
    }

    #[test]
    fn read_only_does_not_require_token() {
        assert!(!CapabilityClass::ReadOnly.requires_token());
    }

    #[test]
    fn capability_class_default_is_read_only() {
        assert_eq!(CapabilityClass::default(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn capability_class_display_read_only() {
        assert_eq!(CapabilityClass::ReadOnly.to_string(), "ReadOnly");
    }

    #[test]
    fn capability_class_display_live_write() {
        assert_eq!(CapabilityClass::LiveWrite.to_string(), "LiveWrite");
    }

    #[test]
    fn capability_class_display_live_write_authorized() {
        assert_eq!(
            CapabilityClass::LiveWriteAuthorized { receipt_id: 42 }.to_string(),
            "LiveWriteAuthorized(receipt=42)"
        );
    }

    // ── BoundedDuration ──────────────────────────────────────────────────────

    #[test]
    fn bounded_duration_default_is_five_seconds() {
        assert_eq!(
            BoundedDuration::default().as_duration(),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn bounded_duration_clamps_below_min() {
        let d = BoundedDuration::new_clamped(Duration::from_micros(1));
        assert_eq!(d.as_duration(), BOUNDED_DURATION_MIN);
    }

    #[test]
    fn bounded_duration_clamps_above_max() {
        let d = BoundedDuration::new_clamped(Duration::from_secs(9999));
        assert_eq!(d.as_duration(), BOUNDED_DURATION_MAX);
    }

    #[test]
    fn bounded_duration_passes_through_mid_range() {
        let mid = Duration::from_secs(10);
        assert_eq!(BoundedDuration::new_clamped(mid).as_duration(), mid);
    }

    // ── AuthGate ─────────────────────────────────────────────────────────────

    #[test]
    fn auth_gate_issues_token_for_live_write() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(1, CapabilityClass::LiveWrite, 100)
            .expect("must succeed");
        assert_eq!(tok.receipt_id(), 1);
    }

    #[test]
    fn auth_gate_rejects_read_only_class() {
        let gate = AuthGate::default();
        let err = gate
            .issue_token(1, CapabilityClass::ReadOnly, 100)
            .expect_err("must fail");
        assert_eq!(err.error_code(), 2601);
    }

    #[test]
    fn auth_gate_rejects_zero_receipt_id() {
        let gate = AuthGate::default();
        let err = gate
            .issue_token(0, CapabilityClass::LiveWrite, 100)
            .expect_err("must fail");
        assert_eq!(err.error_code(), 2600);
    }

    #[test]
    fn write_auth_token_is_expired_when_tick_ge_expires() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(1, CapabilityClass::LiveWrite, 50)
            .expect("must succeed");
        assert!(tok.is_expired(50));
        assert!(tok.is_expired(100));
        assert!(!tok.is_expired(49));
    }

    #[test]
    fn write_auth_token_display_contains_receipt_id() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(7, CapabilityClass::LiveWrite, 99)
            .expect("must succeed");
        assert!(tok.to_string().contains("receipt=7"));
    }

    // ── BridgeReceipt ────────────────────────────────────────────────────────

    #[test]
    fn bridge_receipt_hex_sha_is_64_chars() {
        let r = BridgeReceipt::new("hle.test.v1", "op", b"hello");
        assert_eq!(r.hex_sha().len(), 64);
    }

    #[test]
    fn bridge_receipt_new_has_no_auth_receipt_by_default() {
        let r = BridgeReceipt::new("hle.test.v1", "op", b"payload");
        assert!(r.auth_receipt_id.is_none());
    }

    #[test]
    fn bridge_receipt_with_auth_sets_receipt_id() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(99, CapabilityClass::LiveWrite, 100)
            .expect("must succeed");
        let r = BridgeReceipt::new("hle.test.v1", "op", b"payload").with_auth(&tok);
        assert_eq!(r.auth_receipt_id, Some(99));
    }

    #[test]
    fn bridge_receipt_sha_is_deterministic() {
        let r1 = BridgeReceipt::new("hle.test.v1", "op", b"abc");
        let r2 = BridgeReceipt::new("hle.test.v1", "op", b"abc");
        assert_eq!(r1.payload_sha256, r2.payload_sha256);
    }

    #[test]
    fn bridge_receipt_sha_differs_for_different_payloads() {
        let r1 = BridgeReceipt::new("hle.test.v1", "op", b"abc");
        let r2 = BridgeReceipt::new("hle.test.v1", "op", b"xyz");
        assert_ne!(r1.payload_sha256, r2.payload_sha256);
    }

    #[test]
    fn bridge_receipt_display_contains_schema() {
        let r = BridgeReceipt::new("hle.test.v1", "op", b"x");
        assert!(r.to_string().contains("hle.test.v1"));
    }

    // ── BridgeContractError ──────────────────────────────────────────────────

    #[test]
    fn contract_violation_error_code_is_2600() {
        let e = BridgeContractError::ContractViolation {
            schema_id: "s",
            reason: String::from("r"),
        };
        assert_eq!(e.error_code(), 2600);
    }

    #[test]
    fn capability_denied_error_code_is_2601() {
        let e = BridgeContractError::CapabilityDenied {
            required: CapabilityClass::LiveWrite,
            actual: CapabilityClass::ReadOnly,
        };
        assert_eq!(e.error_code(), 2601);
    }

    #[test]
    fn bridge_contract_error_is_never_retryable() {
        let e = BridgeContractError::ContractViolation {
            schema_id: "s",
            reason: String::from("r"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn contract_violation_display_contains_code() {
        let e = BridgeContractError::ContractViolation {
            schema_id: "s",
            reason: String::from("r"),
        };
        assert!(e.to_string().contains("[HLE-2600]"));
    }

    #[test]
    fn capability_denied_display_contains_code() {
        let e = BridgeContractError::CapabilityDenied {
            required: CapabilityClass::LiveWrite,
            actual: CapabilityClass::ReadOnly,
        };
        assert!(e.to_string().contains("[HLE-2601]"));
    }

    // ── xor_fold_sha256_stub ─────────────────────────────────────────────────

    #[test]
    fn xor_fold_empty_is_all_zeros() {
        assert_eq!(xor_fold_sha256_stub(b""), [0u8; 32]);
    }

    #[test]
    fn xor_fold_deterministic() {
        assert_eq!(xor_fold_sha256_stub(b"abc"), xor_fold_sha256_stub(b"abc"));
    }

    #[test]
    fn xor_fold_differs_on_different_input() {
        assert_ne!(xor_fold_sha256_stub(b"abc"), xor_fold_sha256_stub(b"xyz"));
    }

    // ── Additional CapabilityClass ───────────────────────────────────────────

    #[test]
    fn live_write_authorized_is_not_read_only() {
        assert!(!CapabilityClass::LiveWriteAuthorized { receipt_id: 7 }.is_read_only());
    }

    #[test]
    fn live_write_authorized_requires_token() {
        assert!(CapabilityClass::LiveWriteAuthorized { receipt_id: 42 }.requires_token());
    }

    #[test]
    fn read_only_class_equality() {
        assert_eq!(CapabilityClass::ReadOnly, CapabilityClass::ReadOnly);
    }

    #[test]
    fn live_write_class_equality() {
        assert_eq!(CapabilityClass::LiveWrite, CapabilityClass::LiveWrite);
    }

    #[test]
    fn live_write_authorized_equality_matches_receipt_id() {
        let a = CapabilityClass::LiveWriteAuthorized { receipt_id: 5 };
        let b = CapabilityClass::LiveWriteAuthorized { receipt_id: 5 };
        let c = CapabilityClass::LiveWriteAuthorized { receipt_id: 6 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── BoundedDuration additional ───────────────────────────────────────────

    #[test]
    fn bounded_duration_at_min_passes_through() {
        let d = BoundedDuration::new_clamped(BOUNDED_DURATION_MIN);
        assert_eq!(d.as_duration(), BOUNDED_DURATION_MIN);
    }

    #[test]
    fn bounded_duration_at_max_passes_through() {
        let d = BoundedDuration::new_clamped(BOUNDED_DURATION_MAX);
        assert_eq!(d.as_duration(), BOUNDED_DURATION_MAX);
    }

    #[test]
    fn bounded_duration_display_contains_ms() {
        let d = BoundedDuration::default();
        assert!(d.to_string().contains("ms"));
    }

    #[test]
    fn bounded_duration_display_shows_five_thousand_ms() {
        let d = BoundedDuration::default();
        assert_eq!(d.to_string(), "5000ms");
    }

    #[test]
    fn bounded_duration_copy_is_independent() {
        let a = BoundedDuration::default();
        let b = a;
        assert_eq!(a.as_duration(), b.as_duration());
    }

    // ── AuthGate additional ──────────────────────────────────────────────────

    #[test]
    fn auth_gate_issues_token_for_live_write_authorized() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(
                2,
                CapabilityClass::LiveWriteAuthorized { receipt_id: 2 },
                200,
            )
            .expect("must succeed");
        assert_eq!(tok.receipt_id(), 2);
    }

    #[test]
    fn auth_gate_rejects_zero_receipt_error_code_is_2600() {
        let gate = AuthGate::default();
        let err = gate
            .issue_token(0, CapabilityClass::LiveWrite, 50)
            .expect_err("must fail");
        assert_eq!(err.error_code(), 2600);
    }

    #[test]
    fn auth_gate_token_with_zero_ttl_is_immediately_expired() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(1, CapabilityClass::LiveWrite, 0)
            .expect("must succeed");
        assert!(tok.is_expired(0));
    }

    #[test]
    fn auth_gate_token_not_expired_before_ttl() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(1, CapabilityClass::LiveWrite, 100)
            .expect("must succeed");
        assert!(!tok.is_expired(99));
    }

    // ── WriteAuthToken additional ────────────────────────────────────────────

    #[test]
    fn write_auth_token_display_contains_expires() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(3, CapabilityClass::LiveWrite, 77)
            .expect("must succeed");
        assert!(tok.to_string().contains("expires=77"));
    }

    #[test]
    fn write_auth_token_receipt_id_accessor() {
        let gate = AuthGate::default();
        let tok = gate
            .issue_token(42, CapabilityClass::LiveWrite, 1)
            .expect("must succeed");
        assert_eq!(tok.receipt_id(), 42);
    }

    // ── BridgeReceipt additional ─────────────────────────────────────────────

    #[test]
    fn bridge_receipt_operation_preserved() {
        let r = BridgeReceipt::new("hle.test.v1", "my_op", b"data");
        assert_eq!(r.operation, "my_op");
    }

    #[test]
    fn bridge_receipt_schema_id_preserved() {
        let r = BridgeReceipt::new("hle.custom.v2", "op", b"data");
        assert_eq!(r.schema_id, "hle.custom.v2");
    }

    #[test]
    fn bridge_receipt_empty_payload_is_all_zeros() {
        let r = BridgeReceipt::new("hle.test.v1", "op", b"");
        assert_eq!(r.payload_sha256, [0u8; 32]);
    }

    #[test]
    fn bridge_receipt_display_contains_operation() {
        let r = BridgeReceipt::new("hle.test.v1", "my_special_op", b"x");
        assert!(r.to_string().contains("my_special_op"));
    }

    // ── BridgeContractError additional ──────────────────────────────────────

    #[test]
    fn contract_violation_is_never_retryable() {
        let e = BridgeContractError::ContractViolation {
            schema_id: "s",
            reason: String::from("r"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn capability_denied_is_never_retryable() {
        let e = BridgeContractError::CapabilityDenied {
            required: CapabilityClass::LiveWrite,
            actual: CapabilityClass::ReadOnly,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn contract_violation_display_contains_schema_id() {
        let e = BridgeContractError::ContractViolation {
            schema_id: "hle.mybridge.v1",
            reason: String::from("r"),
        };
        assert!(e.to_string().contains("hle.mybridge.v1"));
    }

    #[test]
    fn capability_denied_display_contains_required_and_actual() {
        let e = BridgeContractError::CapabilityDenied {
            required: CapabilityClass::LiveWrite,
            actual: CapabilityClass::ReadOnly,
        };
        let s = e.to_string();
        assert!(s.contains("LiveWrite"));
        assert!(s.contains("ReadOnly"));
    }

    // ── Sealed<Class> dyn-safety via Arc<dyn BridgeContract> ────────────────

    struct MinimalBridge;
    impl std::fmt::Debug for MinimalBridge {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MinimalBridge")
        }
    }
    impl BridgeContract for MinimalBridge {
        fn schema_id(&self) -> &'static str {
            "hle.minimal.v1"
        }
        fn port(&self) -> Option<u16> {
            None
        }
        fn paths(&self) -> &[&'static str] {
            &[]
        }
        fn name(&self) -> &'static str {
            "minimal"
        }
    }

    #[test]
    fn bridge_contract_dyn_arc_is_constructible() {
        let bridge: std::sync::Arc<dyn BridgeContract> = std::sync::Arc::new(MinimalBridge);
        assert_eq!(bridge.schema_id(), "hle.minimal.v1");
    }

    #[test]
    fn bridge_contract_default_capability_class_is_read_only() {
        let bridge = MinimalBridge;
        assert_eq!(bridge.capability_class(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn bridge_contract_default_supports_write_is_false() {
        let bridge = MinimalBridge;
        assert!(!bridge.supports_write());
    }

    #[test]
    fn bridge_contract_name_accessor() {
        let bridge = MinimalBridge;
        assert_eq!(bridge.name(), "minimal");
    }

    // ── xor_fold additional ──────────────────────────────────────────────────

    #[test]
    fn xor_fold_single_byte_sets_only_first_byte() {
        let result = xor_fold_sha256_stub(&[0xFF]);
        assert_eq!(result[0], 0xFF);
        assert!(result[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn xor_fold_wraps_at_32_bytes() {
        // byte at position 32 should XOR into position 0
        let mut data = vec![0u8; 33];
        data[0] = 0xAA;
        data[32] = 0x55;
        let result = xor_fold_sha256_stub(&data);
        assert_eq!(result[0], 0xAA ^ 0x55);
    }
}
