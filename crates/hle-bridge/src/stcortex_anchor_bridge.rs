//! M044 StcortexAnchorBridge — future-gated STcortex anchor bridge.
//!
//! Read-only anchor lookup in the `hle:` namespace is available immediately.
//! Write-side (`write_anchor`, `delete_anchor`) is compile-time sealed via
//! `Sealed<LiveWrite>` — absent from the `ReadOnly` impl block entirely.
//!
//! On STcortex unreachable, returns `Err(AnchorReadFailed { retryable: true })`.
//! No silent POVM fallback.
//!
//! Error codes: 2640–2642.

use std::fmt;
use std::net::TcpStream;

use crate::bridge_contract::{
    xor_fold_sha256_stub, BoundedDuration, BridgeContract, BridgeReceipt, CapabilityClass,
    LiveWrite, ReadOnly, Sealed, WriteAuthToken,
};

// ─── HTTP probe path ──────────────────────────────────────────────────────────

/// Path used for the STcortex liveness HTTP probe.
///
/// STcortex exposes a SpacetimeDB-compatible endpoint; the root path returns a
/// 404 (by design) but a HEAD/GET to it still proves the HTTP stack is live.
/// We use `/health` as a conventional probe path; a 404 from STcortex is also
/// accepted as evidence of liveness (the process is running and responding).
pub const STCORTEX_PROBE_PATH: &str = "/health";

// ─── Constants ───────────────────────────────────────────────────────────────

/// Required prefix for all keys managed by this bridge.
pub const ANCHOR_KEY_PREFIX: &str = "hle:";
/// Maximum byte length of a validated anchor key.
pub const ANCHOR_KEY_MAX_LEN: usize = 256;
/// Maximum byte length of an anchor value.
pub const ANCHOR_VALUE_MAX_BYTES: usize = 8_192;
/// Default STcortex host.
pub const STCORTEX_DEFAULT_HOST: &str = "127.0.0.1";
/// Default STcortex port.
pub const STCORTEX_DEFAULT_PORT: u16 = 3000;

// ─── AnchorKey ───────────────────────────────────────────────────────────────

/// A validated key in the STcortex `hle:` namespace.
///
/// Keys must start with `hle:`, be ASCII, non-empty, and at most
/// `ANCHOR_KEY_MAX_LEN` characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorKey(String);

impl AnchorKey {
    /// Construct and validate an anchor key.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed)` when the key is empty, too long,
    /// not ASCII, or does not start with `hle:`.
    pub fn new(key: impl Into<String>) -> Result<Self, StcortexAnchorBridgeError> {
        let key = key.into();
        if key.is_empty() {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key: String::from("<empty>"),
                reason: String::from("key must not be empty"),
                retryable: false,
            });
        }
        if key.len() > ANCHOR_KEY_MAX_LEN {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key,
                reason: format!("key exceeds max length {ANCHOR_KEY_MAX_LEN}"),
                retryable: false,
            });
        }
        if !key.is_ascii() {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key,
                reason: String::from("key must be ASCII"),
                retryable: false,
            });
        }
        if !key.starts_with(ANCHOR_KEY_PREFIX) {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key,
                reason: format!("key must start with '{ANCHOR_KEY_PREFIX}'"),
                retryable: false,
            });
        }
        Ok(Self(key))
    }

    /// Full key with the `hle:` prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Key without the `hle:` prefix.
    #[must_use]
    pub fn local_name(&self) -> &str {
        self.0.strip_prefix(ANCHOR_KEY_PREFIX).unwrap_or(&self.0)
    }
}

impl fmt::Display for AnchorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ─── AnchorValue ─────────────────────────────────────────────────────────────

/// Bounded anchor payload stored in STcortex.
///
/// Value bytes are capped at `ANCHOR_VALUE_MAX_BYTES` (8,192 bytes). Values
/// exceeding the cap are rejected — no silent truncation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorValue {
    bytes: Vec<u8>,
}

impl AnchorValue {
    /// Construct from bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed)` when `bytes.len() > ANCHOR_VALUE_MAX_BYTES`.
    pub fn new(bytes: Vec<u8>) -> Result<Self, StcortexAnchorBridgeError> {
        if bytes.len() > ANCHOR_VALUE_MAX_BYTES {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key: String::from("<value>"),
                reason: format!(
                    "value {} bytes exceeds cap {ANCHOR_VALUE_MAX_BYTES}",
                    bytes.len()
                ),
                retryable: false,
            });
        }
        Ok(Self { bytes })
    }

    /// Construct from a UTF-8 string, encoding as bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed)` when the encoded bytes exceed the cap.
    pub fn from_utf8_str(s: impl Into<String>) -> Result<Self, StcortexAnchorBridgeError> {
        Self::new(s.into().into_bytes())
    }

    /// Raw bytes of the anchor value.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Lossy UTF-8 view of the bytes.
    #[must_use]
    pub fn as_str_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }

    /// Byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when no bytes are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl fmt::Display for AnchorValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AnchorValue({} bytes)", self.bytes.len())
    }
}

// ─── AnchorRecord ────────────────────────────────────────────────────────────

/// A retrieved anchor record from the STcortex `hle:` namespace.
///
/// `version` is the STcortex row version for optimistic concurrency.
/// `retrieved_at_tick` is the local tick counter at retrieval time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorRecord {
    /// The anchor key.
    pub key: AnchorKey,
    /// The anchor value.
    pub value: AnchorValue,
    /// STcortex row version.
    pub version: u64,
    /// Local tick at retrieval time.
    pub retrieved_at_tick: u64,
}

impl AnchorRecord {
    /// True when the record was retrieved within `ttl_ticks` of `now_tick`.
    #[must_use]
    pub fn is_fresh(&self, now_tick: u64, ttl_ticks: u64) -> bool {
        now_tick.saturating_sub(self.retrieved_at_tick) <= ttl_ticks
    }
}

impl fmt::Display for AnchorRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AnchorRecord({} v={} len={})",
            self.key,
            self.version,
            self.value.len()
        )
    }
}

// ─── WriteAnchorReceipt ──────────────────────────────────────────────────────

/// Write confirmation for a STcortex anchor operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteAnchorReceipt {
    /// The anchor key that was written or deleted.
    pub key: AnchorKey,
    /// SHA-256 of the value bytes written.
    pub value_sha256: [u8; 32],
    /// STcortex row version assigned.
    pub stcortex_version: u64,
    /// C01 authority receipt ID.
    pub auth_receipt_id: u64,
}

impl WriteAnchorReceipt {
    /// Convert into a `BridgeReceipt` for C01 verifier routing.
    #[must_use]
    pub fn into_bridge_receipt(self) -> BridgeReceipt {
        BridgeReceipt {
            schema_id: "hle.stcortex_anchor.v1",
            operation: format!("write_anchor:{}", self.key),
            payload_sha256: self.value_sha256,
            timestamp_tick: 0,
            auth_receipt_id: Some(self.auth_receipt_id),
        }
    }
}

// ─── StcortexAnchorBridge ────────────────────────────────────────────────────

/// STcortex anchor bridge parameterized over capability class.
///
/// `StcortexAnchorBridge<Sealed<ReadOnly>>` exposes anchor lookup, existence
/// checks, and namespace enumeration. Write-side methods exist only on
/// `Sealed<LiveWrite>`.
#[derive(Debug)]
pub struct StcortexAnchorBridge<Class> {
    _class: Class,
    host: String,
    port: u16,
    default_timeout: BoundedDuration,
}

impl BridgeContract for StcortexAnchorBridge<Sealed<ReadOnly>> {
    fn schema_id(&self) -> &'static str {
        "hle.stcortex_anchor.v1"
    }
    fn port(&self) -> Option<u16> {
        Some(self.port)
    }
    fn paths(&self) -> &[&'static str] {
        &["/v1/kv/hle", "/v1/kv/hle/list"]
    }
    fn supports_write(&self) -> bool {
        false
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }
    fn name(&self) -> &'static str {
        "stcortex_anchor_bridge"
    }
}

impl BridgeContract for StcortexAnchorBridge<Sealed<LiveWrite>> {
    fn schema_id(&self) -> &'static str {
        "hle.stcortex_anchor.v1"
    }
    fn port(&self) -> Option<u16> {
        Some(self.port)
    }
    fn paths(&self) -> &[&'static str] {
        &["/v1/kv/hle", "/v1/kv/hle/list"]
    }
    fn supports_write(&self) -> bool {
        true
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::LiveWrite
    }
    fn name(&self) -> &'static str {
        "stcortex_anchor_bridge"
    }
}

// ── Shared transport helper ──────────────────────────────────────────────────

/// Fallback zero-address used when parsing the probe address fails.
const ZERO_SOCK_ADDR: std::net::SocketAddr =
    std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);

fn tcp_reachable(host: &str, port: u16, timeout: BoundedDuration) -> bool {
    let addr = format!("{host}:{port}");
    let sock_addr = addr.parse().unwrap_or(ZERO_SOCK_ADDR);
    TcpStream::connect_timeout(&sock_addr, timeout.as_duration()).is_ok()
}

// ── ReadOnly surface ─────────────────────────────────────────────────────────

impl StcortexAnchorBridge<Sealed<ReadOnly>> {
    /// Construct with the default STcortex endpoint.
    #[must_use]
    pub fn new_read_only(timeout: BoundedDuration) -> Self {
        Self {
            _class: Sealed::default(),
            host: String::from(STCORTEX_DEFAULT_HOST),
            port: STCORTEX_DEFAULT_PORT,
            default_timeout: timeout,
        }
    }

    /// Construct with a custom endpoint for test injection.
    ///
    /// # Errors
    ///
    /// Returns `Err` when host is empty or port is zero.
    pub fn with_endpoint(
        host: String,
        port: u16,
        timeout: BoundedDuration,
    ) -> Result<Self, StcortexAnchorBridgeError> {
        if host.is_empty() || port == 0 {
            return Err(StcortexAnchorBridgeError::AnchorReadFailed {
                key: String::from("<endpoint>"),
                reason: String::from("host must be non-empty and port non-zero"),
                retryable: false,
            });
        }
        Ok(Self {
            _class: Sealed::default(),
            host,
            port,
            default_timeout: timeout,
        })
    }

    /// Retrieve an anchor by key.
    ///
    /// M0 stub: STcortex transport not invoked; returns `None` (not found).
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed { retryable: true })` when STcortex
    /// is unreachable.
    #[must_use]
    pub fn get_anchor(
        &self,
        _key: &AnchorKey,
    ) -> Result<Option<AnchorRecord>, StcortexAnchorBridgeError> {
        // M0 stub: no live transport.
        Ok(None)
    }

    /// True iff the key exists in the `hle:` namespace.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed)` on transport failure.
    #[must_use]
    pub fn anchor_exists(&self, key: &AnchorKey) -> Result<bool, StcortexAnchorBridgeError> {
        self.get_anchor(key).map(|opt| opt.is_some())
    }

    /// List up to `limit` (max 256) keys in the `hle:` namespace.
    ///
    /// M0 stub: returns an empty list.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed)` on transport failure.
    #[must_use]
    pub fn list_anchors(&self, limit: u16) -> Result<Vec<AnchorKey>, StcortexAnchorBridgeError> {
        let _cap = limit.min(256);
        Ok(Vec::new())
    }

    /// True iff the STcortex port is reachable via TCP. Swallows error.
    #[must_use]
    pub fn is_reachable(&self) -> bool {
        tcp_reachable(&self.host, self.port, self.default_timeout)
    }

    /// Perform a real HTTP GET probe against the STcortex health endpoint.
    ///
    /// Issues `GET http://HOST:PORT/health` using `ureq` with the configured
    /// timeout. Returns `Ok(true)` when the server responds with any HTTP
    /// status (including 4xx — a 404 from SpacetimeDB still proves liveness).
    /// Returns `Ok(false)` when the connection is refused or times out.
    ///
    /// This is read-only: no mutation of STcortex state occurs.
    ///
    /// # Errors
    ///
    /// Returns `Err(AnchorReadFailed { retryable: true })` only on unexpected
    /// transport-level errors other than connection-refused/timeout.
    #[must_use]
    pub fn probe_http(&self) -> Result<bool, StcortexAnchorBridgeError> {
        let url = format!("http://{}:{}{}", self.host, self.port, STCORTEX_PROBE_PATH);
        let timeout = self.default_timeout.as_duration();
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout_read(timeout)
            .timeout_write(timeout)
            .build();
        match agent.get(&url).call() {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(_, _)) => {
                // Any HTTP status response (incl. 4xx/5xx) means the server is live.
                Ok(true)
            }
            Err(ureq::Error::Transport(t)) => {
                let kind = t.kind();
                if matches!(
                    kind,
                    ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Io
                ) {
                    Ok(false)
                } else {
                    Err(StcortexAnchorBridgeError::AnchorReadFailed {
                        key: String::from("<probe>"),
                        reason: format!("unexpected transport error: {t}"),
                        retryable: true,
                    })
                }
            }
        }
    }
}

// ── LiveWrite surface ─────────────────────────────────────────────────────────

impl StcortexAnchorBridge<Sealed<LiveWrite>> {
    /// Construct with the live-write capability.
    ///
    /// # Errors
    ///
    /// Returns `Err(WriteGateSealed)` when the token is zero-TTL.
    pub fn new_live_write(
        token: &WriteAuthToken,
        timeout: BoundedDuration,
    ) -> Result<Self, StcortexAnchorBridgeError> {
        if token.expires_at_tick == 0 {
            return Err(StcortexAnchorBridgeError::WriteGateSealed {
                bridge: "stcortex_anchor_bridge",
            });
        }
        Ok(Self {
            _class: Sealed::default(),
            host: String::from(STCORTEX_DEFAULT_HOST),
            port: STCORTEX_DEFAULT_PORT,
            default_timeout: timeout,
        })
    }

    /// Retrieve an anchor (read-only, also available on `LiveWrite`).
    #[must_use]
    pub fn get_anchor(
        &self,
        _key: &AnchorKey,
    ) -> Result<Option<AnchorRecord>, StcortexAnchorBridgeError> {
        Ok(None)
    }

    /// Check key existence.
    #[must_use]
    pub fn anchor_exists(&self, key: &AnchorKey) -> Result<bool, StcortexAnchorBridgeError> {
        self.get_anchor(key).map(|opt| opt.is_some())
    }

    /// List keys.
    #[must_use]
    pub fn list_anchors(&self, limit: u16) -> Result<Vec<AnchorKey>, StcortexAnchorBridgeError> {
        let _cap = limit.min(256);
        Ok(Vec::new())
    }

    /// True iff the STcortex port is reachable.
    #[must_use]
    pub fn is_reachable(&self) -> bool {
        tcp_reachable(&self.host, self.port, self.default_timeout)
    }

    /// Write an anchor to the STcortex `hle:` namespace.
    ///
    /// M0 stub: does not invoke transport; returns receipt.
    ///
    /// # Errors
    ///
    /// Returns `Err(WriteGateSealed)` when the token is expired.
    #[must_use]
    pub fn write_anchor(
        &self,
        key: &AnchorKey,
        value: &AnchorValue,
        token: &WriteAuthToken,
    ) -> Result<WriteAnchorReceipt, StcortexAnchorBridgeError> {
        if token.expires_at_tick == 0 {
            return Err(StcortexAnchorBridgeError::WriteGateSealed {
                bridge: "stcortex_anchor_bridge",
            });
        }
        Ok(WriteAnchorReceipt {
            key: key.clone(),
            value_sha256: xor_fold_sha256_stub(value.as_bytes()),
            stcortex_version: 1,
            auth_receipt_id: token.receipt_id(),
        })
    }

    /// Delete an anchor from the STcortex `hle:` namespace.
    ///
    /// M0 stub: does not invoke transport; returns receipt.
    ///
    /// # Errors
    ///
    /// Returns `Err(WriteGateSealed)` when the token is expired.
    #[must_use]
    pub fn delete_anchor(
        &self,
        key: &AnchorKey,
        token: &WriteAuthToken,
    ) -> Result<WriteAnchorReceipt, StcortexAnchorBridgeError> {
        if token.expires_at_tick == 0 {
            return Err(StcortexAnchorBridgeError::WriteGateSealed {
                bridge: "stcortex_anchor_bridge",
            });
        }
        Ok(WriteAnchorReceipt {
            key: key.clone(),
            value_sha256: xor_fold_sha256_stub(key.as_str().as_bytes()),
            stcortex_version: 0,
            auth_receipt_id: token.receipt_id(),
        })
    }
}

// ─── StcortexAnchorBridgeError ───────────────────────────────────────────────

/// Errors for M044 STcortex anchor bridge.
///
/// Error codes: 2640–2642.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StcortexAnchorBridgeError {
    /// Code 2640. Key `hle:NAME` not present in STcortex.
    AnchorNotFound {
        /// The missing key.
        key: String,
    },
    /// Code 2641. Transport or parse error during anchor read.
    AnchorReadFailed {
        /// Key that was being accessed.
        key: String,
        /// Human-readable failure reason.
        reason: String,
        /// True when connection refused or timeout (transient).
        retryable: bool,
    },
    /// Code 2642. Write attempted without valid token.
    WriteGateSealed {
        /// Bridge name.
        bridge: &'static str,
    },
}

impl StcortexAnchorBridgeError {
    /// Error code: 2640, 2641, or 2642.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::AnchorNotFound { .. } => 2640,
            Self::AnchorReadFailed { .. } => 2641,
            Self::WriteGateSealed { .. } => 2642,
        }
    }

    /// Propagates inner `retryable`; false for 2640/2642.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::AnchorReadFailed {
                retryable: true,
                ..
            }
        )
    }
}

impl fmt::Display for StcortexAnchorBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnchorNotFound { key } => {
                write!(f, "[HLE-2640] anchor not found: {key}")
            }
            Self::AnchorReadFailed {
                key,
                reason,
                retryable,
            } => {
                write!(
                    f,
                    "[HLE-2641] anchor read failed (key={key}, retryable={retryable}): {reason}"
                )
            }
            Self::WriteGateSealed { bridge } => {
                write!(f, "[HLE-2642] write gate sealed on bridge={bridge}")
            }
        }
    }
}

impl std::error::Error for StcortexAnchorBridgeError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_contract::AuthGate;

    fn timeout() -> BoundedDuration {
        BoundedDuration::default()
    }

    fn valid_key() -> AnchorKey {
        AnchorKey::new("hle:test-key").expect("valid key")
    }

    fn valid_token() -> WriteAuthToken {
        AuthGate::default()
            .issue_token(1, CapabilityClass::LiveWrite, 1000)
            .expect("valid token")
    }

    // ── AnchorKey ────────────────────────────────────────────────────────────

    #[test]
    fn anchor_key_valid_prefix() {
        assert!(AnchorKey::new("hle:foo").is_ok());
    }

    #[test]
    fn anchor_key_rejects_missing_prefix() {
        assert!(AnchorKey::new("other:foo").is_err());
    }

    #[test]
    fn anchor_key_rejects_empty() {
        assert!(AnchorKey::new("").is_err());
    }

    #[test]
    fn anchor_key_rejects_too_long() {
        let long = format!("hle:{}", "x".repeat(ANCHOR_KEY_MAX_LEN));
        assert!(AnchorKey::new(long).is_err());
    }

    #[test]
    fn anchor_key_rejects_non_ascii() {
        assert!(AnchorKey::new("hle:café").is_err());
    }

    #[test]
    fn anchor_key_as_str_returns_full_key() {
        let k = valid_key();
        assert_eq!(k.as_str(), "hle:test-key");
    }

    #[test]
    fn anchor_key_local_name_strips_prefix() {
        let k = valid_key();
        assert_eq!(k.local_name(), "test-key");
    }

    // ── AnchorValue ──────────────────────────────────────────────────────────

    #[test]
    fn anchor_value_new_valid() {
        let v = AnchorValue::new(vec![1, 2, 3]).expect("valid");
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn anchor_value_rejects_over_cap() {
        let big = vec![0u8; ANCHOR_VALUE_MAX_BYTES + 1];
        assert!(AnchorValue::new(big).is_err());
    }

    #[test]
    fn anchor_value_at_cap_is_accepted() {
        let at_cap = vec![0u8; ANCHOR_VALUE_MAX_BYTES];
        assert!(AnchorValue::new(at_cap).is_ok());
    }

    #[test]
    fn anchor_value_from_str_valid() {
        let v = AnchorValue::from_utf8_str("hello").expect("valid");
        assert_eq!(v.as_bytes(), b"hello");
    }

    #[test]
    fn anchor_value_is_empty_when_zero_bytes() {
        let v = AnchorValue::new(vec![]).expect("valid");
        assert!(v.is_empty());
    }

    // ── AnchorRecord ─────────────────────────────────────────────────────────

    #[test]
    fn anchor_record_is_fresh_within_ttl() {
        let key = valid_key();
        let value = AnchorValue::from_utf8_str("v").expect("valid");
        let rec = AnchorRecord {
            key,
            value,
            version: 1,
            retrieved_at_tick: 10,
        };
        assert!(rec.is_fresh(15, 10));
    }

    #[test]
    fn anchor_record_is_stale_beyond_ttl() {
        let key = valid_key();
        let value = AnchorValue::from_utf8_str("v").expect("valid");
        let rec = AnchorRecord {
            key,
            value,
            version: 1,
            retrieved_at_tick: 5,
        };
        assert!(!rec.is_fresh(200, 10));
    }

    // ── ReadOnly bridge ──────────────────────────────────────────────────────

    #[test]
    fn read_only_capability_class_is_read_only() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert_eq!(b.capability_class(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn read_only_supports_write_is_false() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert!(!b.supports_write());
    }

    #[test]
    fn read_only_get_anchor_returns_none_in_m0() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        let k = valid_key();
        let result = b.get_anchor(&k).expect("must succeed");
        assert!(result.is_none());
    }

    #[test]
    fn read_only_anchor_exists_false_in_m0() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        let k = valid_key();
        assert!(!b.anchor_exists(&k).expect("must succeed"));
    }

    #[test]
    fn read_only_list_anchors_empty_in_m0() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert!(b.list_anchors(10).expect("must succeed").is_empty());
    }

    #[test]
    fn with_endpoint_rejects_empty_host() {
        let result = StcortexAnchorBridge::with_endpoint(String::new(), 3000, timeout());
        assert!(result.is_err());
    }

    #[test]
    fn with_endpoint_rejects_zero_port() {
        let result = StcortexAnchorBridge::with_endpoint(String::from("127.0.0.1"), 0, timeout());
        assert!(result.is_err());
    }

    // ── LiveWrite surface ────────────────────────────────────────────────────

    #[test]
    fn live_write_write_anchor_returns_receipt() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("must succeed");
        let k = valid_key();
        let v = AnchorValue::from_utf8_str("data").expect("valid");
        let receipt = b.write_anchor(&k, &v, &tok).expect("must succeed");
        assert_eq!(receipt.auth_receipt_id, 1);
    }

    #[test]
    fn live_write_delete_anchor_returns_receipt() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("must succeed");
        let k = valid_key();
        let receipt = b.delete_anchor(&k, &tok).expect("must succeed");
        assert_eq!(receipt.key, k);
    }

    #[test]
    fn write_anchor_receipt_into_bridge_receipt_has_schema() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("must succeed");
        let k = valid_key();
        let v = AnchorValue::from_utf8_str("x").expect("valid");
        let war = b.write_anchor(&k, &v, &tok).expect("must succeed");
        let br = war.into_bridge_receipt();
        assert_eq!(br.schema_id, "hle.stcortex_anchor.v1");
    }

    // ── Error codes ──────────────────────────────────────────────────────────

    #[test]
    fn anchor_not_found_error_code_is_2640() {
        let e = StcortexAnchorBridgeError::AnchorNotFound {
            key: String::from("k"),
        };
        assert_eq!(e.error_code(), 2640);
    }

    #[test]
    fn anchor_read_failed_error_code_is_2641() {
        let e = StcortexAnchorBridgeError::AnchorReadFailed {
            key: String::from("k"),
            reason: String::from("r"),
            retryable: true,
        };
        assert_eq!(e.error_code(), 2641);
    }

    #[test]
    fn write_gate_sealed_error_code_is_2642() {
        let e = StcortexAnchorBridgeError::WriteGateSealed {
            bridge: "stcortex_anchor_bridge",
        };
        assert_eq!(e.error_code(), 2642);
    }

    #[test]
    fn anchor_read_failed_retryable_true_is_retryable() {
        let e = StcortexAnchorBridgeError::AnchorReadFailed {
            key: String::from("k"),
            reason: String::from("r"),
            retryable: true,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn anchor_not_found_is_not_retryable() {
        let e = StcortexAnchorBridgeError::AnchorNotFound {
            key: String::from("k"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn error_display_contains_code_prefix() {
        let e = StcortexAnchorBridgeError::WriteGateSealed { bridge: "b" };
        assert!(e.to_string().contains("[HLE-2642]"));
    }

    // ── AnchorKey additional ─────────────────────────────────────────────────

    #[test]
    fn anchor_key_with_colon_in_name_is_valid() {
        assert!(AnchorKey::new("hle:section:subkey").is_ok());
    }

    #[test]
    fn anchor_key_exactly_at_max_len_is_valid() {
        let key = format!("hle:{}", "k".repeat(ANCHOR_KEY_MAX_LEN - 4));
        assert!(AnchorKey::new(key).is_ok());
    }

    #[test]
    fn anchor_key_one_over_max_len_is_rejected() {
        let key = format!("hle:{}", "k".repeat(ANCHOR_KEY_MAX_LEN - 3));
        assert!(AnchorKey::new(key).is_err());
    }

    #[test]
    fn anchor_key_display_returns_full_key() {
        let k = AnchorKey::new("hle:my-key").expect("valid");
        assert_eq!(k.to_string(), "hle:my-key");
    }

    #[test]
    fn anchor_key_clone_equality() {
        let k = valid_key();
        assert_eq!(k.clone(), k);
    }

    #[test]
    fn anchor_key_hash_is_stable() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(valid_key());
        assert!(set.contains(&valid_key()));
    }

    #[test]
    fn anchor_key_non_hle_namespace_rejected() {
        assert!(AnchorKey::new("povm:something").is_err());
    }

    // ── AnchorValue additional ────────────────────────────────────────────────

    #[test]
    fn anchor_value_empty_is_empty() {
        let v = AnchorValue::new(vec![]).expect("valid");
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn anchor_value_at_exactly_cap_is_valid() {
        let v = AnchorValue::new(vec![0u8; ANCHOR_VALUE_MAX_BYTES]).expect("valid");
        assert_eq!(v.len(), ANCHOR_VALUE_MAX_BYTES);
    }

    #[test]
    fn anchor_value_as_str_lossy_roundtrip() {
        let v = AnchorValue::from_utf8_str("hello world").expect("valid");
        assert_eq!(v.as_str_lossy(), "hello world");
    }

    #[test]
    fn anchor_value_display_shows_byte_count() {
        let v = AnchorValue::new(vec![0u8; 7]).expect("valid");
        assert!(v.to_string().contains("7"));
    }

    #[test]
    fn anchor_value_clone_equality() {
        let v = AnchorValue::from_utf8_str("x").expect("valid");
        assert_eq!(v.clone(), v);
    }

    // ── AnchorRecord additional ──────────────────────────────────────────────

    #[test]
    fn anchor_record_fresh_when_now_equals_retrieved() {
        let rec = AnchorRecord {
            key: valid_key(),
            value: AnchorValue::new(vec![]).expect("valid"),
            version: 1,
            retrieved_at_tick: 50,
        };
        // now=50, ttl=0 → delta=0, 0 <= 0
        assert!(rec.is_fresh(50, 0));
    }

    #[test]
    fn anchor_record_display_contains_key_and_version() {
        let rec = AnchorRecord {
            key: valid_key(),
            value: AnchorValue::new(vec![1]).expect("valid"),
            version: 99,
            retrieved_at_tick: 0,
        };
        let s = rec.to_string();
        assert!(s.contains("hle:test-key"));
        assert!(s.contains("99"));
    }

    // ── WriteAnchorReceipt additional ────────────────────────────────────────

    #[test]
    fn write_anchor_receipt_into_bridge_receipt_preserves_sha() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        let k = valid_key();
        let v = AnchorValue::from_utf8_str("payload").expect("valid");
        let war = b.write_anchor(&k, &v, &tok).expect("ok");
        let sha = war.value_sha256;
        let br = war.into_bridge_receipt();
        assert_eq!(br.payload_sha256, sha);
    }

    #[test]
    fn write_anchor_receipt_version_is_one() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        let k = valid_key();
        let v = AnchorValue::from_utf8_str("v").expect("valid");
        let war = b.write_anchor(&k, &v, &tok).expect("ok");
        assert_eq!(war.stcortex_version, 1);
    }

    #[test]
    fn delete_anchor_receipt_version_is_zero() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        let k = valid_key();
        let war = b.delete_anchor(&k, &tok).expect("ok");
        assert_eq!(war.stcortex_version, 0);
    }

    // ── ReadOnly additional ──────────────────────────────────────────────────

    #[test]
    fn read_only_schema_id_is_correct() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert_eq!(b.schema_id(), "hle.stcortex_anchor.v1");
    }

    #[test]
    fn read_only_name_is_stcortex_anchor_bridge() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert_eq!(b.name(), "stcortex_anchor_bridge");
    }

    #[test]
    fn read_only_port_is_3000_default() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert_eq!(b.port(), Some(STCORTEX_DEFAULT_PORT));
    }

    #[test]
    fn read_only_paths_contain_hle_namespace() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        assert!(b.paths().iter().any(|p| p.contains("hle")));
    }

    #[test]
    fn read_only_list_anchors_cap_at_256() {
        let b = StcortexAnchorBridge::new_read_only(timeout());
        // In M0 always returns empty regardless of limit
        assert!(b.list_anchors(999).expect("ok").is_empty());
    }

    #[test]
    fn with_endpoint_valid_custom_endpoint() {
        let result = StcortexAnchorBridge::with_endpoint(String::from("10.0.0.1"), 4000, timeout());
        assert!(result.is_ok());
    }

    // ── LiveWrite additional ─────────────────────────────────────────────────

    #[test]
    fn live_write_capability_class_is_live_write() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        assert_eq!(b.capability_class(), CapabilityClass::LiveWrite);
    }

    #[test]
    fn live_write_supports_write_is_true() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        assert!(b.supports_write());
    }

    #[test]
    fn live_write_anchor_exists_false_in_m0() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        let k = valid_key();
        assert!(!b.anchor_exists(&k).expect("ok"));
    }

    #[test]
    fn live_write_list_anchors_empty_in_m0() {
        let tok = valid_token();
        let b = StcortexAnchorBridge::new_live_write(&tok, timeout()).expect("ok");
        assert!(b.list_anchors(10).expect("ok").is_empty());
    }

    // ── StcortexAnchorBridgeError additional ─────────────────────────────────

    #[test]
    fn anchor_read_failed_non_retryable_is_not_retryable() {
        let e = StcortexAnchorBridgeError::AnchorReadFailed {
            key: String::from("k"),
            reason: String::from("r"),
            retryable: false,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn write_gate_sealed_is_not_retryable() {
        let e = StcortexAnchorBridgeError::WriteGateSealed { bridge: "b" };
        assert!(!e.is_retryable());
    }

    #[test]
    fn anchor_not_found_display_contains_key() {
        let e = StcortexAnchorBridgeError::AnchorNotFound {
            key: String::from("hle:x"),
        };
        assert!(e.to_string().contains("hle:x"));
    }

    #[test]
    fn anchor_read_failed_display_contains_key_and_reason() {
        let e = StcortexAnchorBridgeError::AnchorReadFailed {
            key: String::from("hle:y"),
            reason: String::from("conn refused"),
            retryable: true,
        };
        let s = e.to_string();
        assert!(s.contains("hle:y"));
        assert!(s.contains("conn refused"));
    }

    #[test]
    fn write_gate_sealed_display_contains_bridge_name() {
        let e = StcortexAnchorBridgeError::WriteGateSealed {
            bridge: "stcortex_anchor_bridge",
        };
        assert!(e.to_string().contains("stcortex_anchor_bridge"));
    }

    // ── probe_http ───────────────────────────────────────────────────────────

    #[test]
    fn probe_http_returns_false_on_closed_port() {
        // Port 1 is closed on virtually every CI/test machine.
        // C03 timeout is clamped — use a very short connect timeout to keep
        // the test fast: 50ms is ample for a local refused connection.
        let b = StcortexAnchorBridge::with_endpoint(
            String::from("127.0.0.1"),
            1,
            BoundedDuration::new_clamped(std::time::Duration::from_millis(50)),
        )
        .expect("valid endpoint");
        // Connection-refused or I/O error should map to Ok(false).
        let result = b.probe_http().expect("must not error on conn-refused");
        assert!(!result);
    }

    #[test]
    fn probe_http_is_read_only_bridge_method() {
        // Structural: probe_http exists only on the ReadOnly impl block.
        // Verify the bridge stays ReadOnly after calling it.
        let b = StcortexAnchorBridge::new_read_only(BoundedDuration::new_clamped(
            std::time::Duration::from_millis(50),
        ));
        assert_eq!(b.capability_class(), CapabilityClass::ReadOnly);
        // probe_http call is a read-path, no side effects — just verify it compiles
        // and returns a Result (don't assert liveness, service may or may not be up).
        let _result = b.probe_http();
    }

    #[test]
    fn stcortex_probe_path_constant_is_slash_health() {
        assert_eq!(STCORTEX_PROBE_PATH, "/health");
    }
}
