//! M041 ZellijDispatch — typed adapter for Zellij pane dispatch packets.
//!
//! Write-side is compile-time sealed via `Sealed<Class>` PhantomData:
//! `ZellijDispatch<Sealed<ReadOnly>>` does not define `dispatch_packet`.
//! Calling it is a compile error, not a runtime denial.
//!
//! Error codes: 2610–2612.

use std::fmt;

use crate::bridge_contract::{
    BoundedDuration, BridgeContract, BridgeReceipt, CapabilityClass, LiveWrite, ReadOnly, Sealed,
    WriteAuthToken,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum byte length of `ZellijPacket::chars`.
pub const ZELLIJ_PACKET_CHAR_CAP: usize = 4_096;
/// Maximum tab index (inclusive).
pub const ZELLIJ_MAX_TAB: u8 = 63;

// ─── PaneTarget ──────────────────────────────────────────────────────────────

/// Validated `(tab-index, pane-label)` coordinate pair.
///
/// Tab index is 0-based and must be ≤ `ZELLIJ_MAX_TAB` (63). Pane label is a
/// non-empty ASCII identifier matching the label set in the Zellij KDL layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaneTarget {
    /// 0-based tab index.
    pub tab: u8,
    /// Non-empty ASCII pane label from the KDL layout.
    pub pane_label: String,
}

impl PaneTarget {
    /// Construct and validate a pane target.
    ///
    /// # Errors
    ///
    /// Returns `Err(DispatchFailed)` when `tab > ZELLIJ_MAX_TAB` or
    /// `pane_label` is empty or not ASCII.
    pub fn new(tab: u8, pane_label: impl Into<String>) -> Result<Self, ZellijDispatchError> {
        let pane_label = pane_label.into();
        if tab > ZELLIJ_MAX_TAB {
            return Err(ZellijDispatchError::DispatchFailed {
                tab,
                pane_label,
                reason: format!("tab index {tab} exceeds maximum {ZELLIJ_MAX_TAB}"),
                retryable: false,
            });
        }
        if pane_label.is_empty() {
            return Err(ZellijDispatchError::DispatchFailed {
                tab,
                pane_label,
                reason: String::from("pane_label must not be empty"),
                retryable: false,
            });
        }
        if !pane_label.is_ascii() {
            return Err(ZellijDispatchError::DispatchFailed {
                tab,
                pane_label,
                reason: String::from("pane_label must be ASCII"),
                retryable: false,
            });
        }
        Ok(Self { tab, pane_label })
    }

    /// The 0-based tab index.
    #[must_use]
    pub fn tab(&self) -> u8 {
        self.tab
    }

    /// The pane label string.
    #[must_use]
    pub fn pane_label(&self) -> &str {
        &self.pane_label
    }
}

impl fmt::Display for PaneTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tab={}:{}", self.tab, self.pane_label)
    }
}

// ─── ZellijPacket ────────────────────────────────────────────────────────────

/// A bounded, validated dispatch payload for a single Zellij pane.
///
/// Constructed via `ZellijPacket::new`. The `chars` field is capped at
/// `ZELLIJ_PACKET_CHAR_CAP` (4,096 bytes). Exceeding the cap is a construction
/// error — callers must split large payloads before dispatch.
///
/// The `trailing_cr` flag appends ASCII 0x0D (CR byte 13) after `chars`,
/// required to submit input in fleet CC panes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZellijPacket {
    /// Target pane coordinate.
    pub target: PaneTarget,
    /// Dispatch payload (capped at `ZELLIJ_PACKET_CHAR_CAP`).
    pub chars: String,
    /// When true, a CR byte (0x0D) is appended after `chars`.
    pub trailing_cr: bool,
    /// Call-site timeout for the `zellij` subprocess.
    pub timeout: BoundedDuration,
}

impl ZellijPacket {
    /// Construct and validate a packet.
    ///
    /// # Errors
    ///
    /// Returns `Err(PacketTooLarge)` when `chars.len() > ZELLIJ_PACKET_CHAR_CAP`.
    pub fn new(
        target: PaneTarget,
        chars: impl Into<String>,
        trailing_cr: bool,
    ) -> Result<Self, ZellijDispatchError> {
        let chars = chars.into();
        if chars.len() > ZELLIJ_PACKET_CHAR_CAP {
            return Err(ZellijDispatchError::PacketTooLarge {
                size_bytes: chars.len(),
                cap_bytes: ZELLIJ_PACKET_CHAR_CAP,
            });
        }
        Ok(Self {
            target,
            chars,
            trailing_cr,
            timeout: BoundedDuration::default(),
        })
    }

    /// Byte length of the dispatch payload including the optional CR byte.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.chars.len() + usize::from(self.trailing_cr)
    }

    /// Validate that this packet is within bounds.
    ///
    /// # Errors
    ///
    /// Returns `Err(PacketTooLarge)` when `chars.len() > ZELLIJ_PACKET_CHAR_CAP`.
    pub fn validate(&self) -> Result<(), ZellijDispatchError> {
        if self.chars.len() > ZELLIJ_PACKET_CHAR_CAP {
            return Err(ZellijDispatchError::PacketTooLarge {
                size_bytes: self.chars.len(),
                cap_bytes: ZELLIJ_PACKET_CHAR_CAP,
            });
        }
        Ok(())
    }
}

impl fmt::Display for ZellijPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ZellijPacket(tab={} pane={} len={})",
            self.target.tab,
            self.target.pane_label,
            self.byte_len()
        )
    }
}

// ─── DispatchOutcome ─────────────────────────────────────────────────────────

/// Reason a dispatch was skipped without error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The `zellij` binary was not found on PATH.
    BinaryAbsent,
    /// Dry-run mode is active; no subprocess was executed.
    DryRun,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryAbsent => f.write_str("binary-absent"),
            Self::DryRun => f.write_str("dry-run"),
        }
    }
}

/// Outcome of a Zellij dispatch attempt.
///
/// Never panics on failure — all error conditions produce
/// `Err(ZellijDispatchError)` not a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Dispatch completed; `bytes` is the byte count written.
    Sent {
        /// Number of bytes written to the pane.
        bytes: usize,
    },
    /// Dispatch skipped for a non-error reason.
    Skipped {
        /// Why the dispatch was skipped.
        reason: SkipReason,
    },
}

impl fmt::Display for DispatchOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sent { bytes } => write!(f, "Sent({bytes} bytes)"),
            Self::Skipped { reason } => write!(f, "Skipped({reason})"),
        }
    }
}

// ─── ZellijDispatch ──────────────────────────────────────────────────────────

/// Zellij pane dispatch bridge parameterized over capability class.
///
/// `ZellijDispatch<Sealed<ReadOnly>>` exposes only `probe` and `validate_packet`.
/// `ZellijDispatch<Sealed<LiveWrite>>` additionally exposes `dispatch_packet`,
/// which is absent from the `ReadOnly` impl block entirely — no runtime gate.
#[derive(Debug)]
pub struct ZellijDispatch<Class> {
    _class: Class,
}

impl BridgeContract for ZellijDispatch<Sealed<ReadOnly>> {
    fn schema_id(&self) -> &'static str {
        "hle.zellij.v1"
    }
    fn port(&self) -> Option<u16> {
        None
    }
    fn paths(&self) -> &[&'static str] {
        &["zellij action write-chars", "zellij action write"]
    }
    fn supports_write(&self) -> bool {
        false
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }
    fn name(&self) -> &'static str {
        "zellij_dispatch"
    }
}

impl BridgeContract for ZellijDispatch<Sealed<LiveWrite>> {
    fn schema_id(&self) -> &'static str {
        "hle.zellij.v1"
    }
    fn port(&self) -> Option<u16> {
        None
    }
    fn paths(&self) -> &[&'static str] {
        &["zellij action write-chars", "zellij action write"]
    }
    fn supports_write(&self) -> bool {
        true
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::LiveWrite
    }
    fn name(&self) -> &'static str {
        "zellij_dispatch"
    }
}

// ── ReadOnly surface (available on both class variants) ──────────────────────

impl ZellijDispatch<Sealed<ReadOnly>> {
    /// Construct a read-only Zellij dispatch bridge. No authorization required.
    #[must_use]
    pub fn new_read_only() -> Self {
        Self {
            _class: Sealed::default(),
        }
    }

    /// Check whether the `zellij` binary is present on PATH.
    ///
    /// Returns `Sent { bytes: 0 }` when reachable, `Skipped { BinaryAbsent }`
    /// when absent. Infallible in M0 — no subprocess is invoked.
    #[must_use]
    pub fn probe(&self) -> DispatchOutcome {
        zellij_binary_probe()
    }

    /// Pure validation of a packet — no subprocess.
    ///
    /// # Errors
    ///
    /// Returns `Err(PacketTooLarge)` when the packet exceeds the char cap.
    pub fn validate_packet(&self, packet: &ZellijPacket) -> Result<(), ZellijDispatchError> {
        packet.validate()
    }
}

impl ZellijDispatch<Sealed<LiveWrite>> {
    /// Construct a live-write Zellij dispatch bridge.
    ///
    /// Validates that the token is not already expired.
    ///
    /// # Errors
    ///
    /// Returns `Err(WriteGateSealed)` when the token is expired (expiry tick == 0
    /// and issued tick == 0 indicates a zero-TTL token).
    pub fn new_live_write(token: &WriteAuthToken) -> Result<Self, ZellijDispatchError> {
        // A token with expires_at_tick == 0 (and issued_at_tick == 0) came in with
        // ttl_ticks == 0, which means it is immediately expired.
        if token.is_expired(token.issued_at_tick) && token.expires_at_tick == 0 {
            return Err(ZellijDispatchError::WriteGateSealed {
                bridge: "zellij_dispatch",
            });
        }
        Ok(Self {
            _class: Sealed::default(),
        })
    }

    /// Check whether the `zellij` binary is present on PATH.
    #[must_use]
    pub fn probe(&self) -> DispatchOutcome {
        zellij_binary_probe()
    }

    /// Pure validation of a packet.
    ///
    /// # Errors
    ///
    /// Returns `Err(PacketTooLarge)` when the packet exceeds the char cap.
    pub fn validate_packet(&self, packet: &ZellijPacket) -> Result<(), ZellijDispatchError> {
        packet.validate()
    }

    /// Dispatch a packet to a Zellij pane.
    ///
    /// In the M0 stub the subprocess is not actually invoked. Returns a
    /// `BridgeReceipt` with SHA-256 of `packet.chars` bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err(WriteGateSealed)` when the presented token is expired.
    /// Returns `Err(PacketTooLarge)` when the packet exceeds the cap.
    #[must_use]
    pub fn dispatch_packet(
        &self,
        packet: &ZellijPacket,
        token: &WriteAuthToken,
    ) -> Result<BridgeReceipt, ZellijDispatchError> {
        if token.is_expired(token.issued_at_tick) && token.expires_at_tick == 0 {
            return Err(ZellijDispatchError::WriteGateSealed {
                bridge: "zellij_dispatch",
            });
        }
        packet.validate()?;
        // M0 stub: compute receipt without executing subprocess.
        let receipt =
            BridgeReceipt::new("hle.zellij.v1", "dispatch_packet", packet.chars.as_bytes())
                .with_auth(token);
        Ok(receipt)
    }
}

// ─── Shared probe helper ─────────────────────────────────────────────────────

fn zellij_binary_probe() -> DispatchOutcome {
    // Check whether `zellij` exists anywhere on PATH by querying `which`-style.
    // std::process::Command::new("zellij").arg("--version") would actually fork;
    // for M0 we use a lightweight PATH scan instead.
    let found = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| {
            let mut p = std::path::PathBuf::from(dir);
            p.push("zellij");
            p.exists()
        });
    if found {
        DispatchOutcome::Sent { bytes: 0 }
    } else {
        DispatchOutcome::Skipped {
            reason: SkipReason::BinaryAbsent,
        }
    }
}

// ─── ZellijDispatchError ─────────────────────────────────────────────────────

/// Errors for M041 Zellij dispatch.
///
/// Error codes: 2610–2612.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZellijDispatchError {
    /// Code 2610. `zellij action write-chars` subprocess returned non-zero.
    DispatchFailed {
        /// Tab index of the failed pane.
        tab: u8,
        /// Pane label of the failed pane.
        pane_label: String,
        /// Human-readable failure reason.
        reason: String,
        /// True when the error is transient (e.g. EAGAIN).
        retryable: bool,
    },
    /// Code 2611. Packet `chars` exceeds `ZELLIJ_PACKET_CHAR_CAP`.
    PacketTooLarge {
        /// Actual byte length of `chars`.
        size_bytes: usize,
        /// Maximum allowed byte length.
        cap_bytes: usize,
    },
    /// Code 2612. Write attempted without a valid token, or token was expired.
    WriteGateSealed {
        /// Name of the bridge that rejected the write.
        bridge: &'static str,
    },
}

impl ZellijDispatchError {
    /// Error code: 2610, 2611, or 2612.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::DispatchFailed { .. } => 2610,
            Self::PacketTooLarge { .. } => 2611,
            Self::WriteGateSealed { .. } => 2612,
        }
    }

    /// True only for `DispatchFailed { retryable: true }`.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::DispatchFailed {
                retryable: true,
                ..
            }
        )
    }
}

impl fmt::Display for ZellijDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DispatchFailed {
                tab,
                pane_label,
                reason,
                retryable,
            } => {
                write!(
                    f,
                    "[HLE-2610] dispatch failed (tab={tab}, pane={pane_label}, \
                     retryable={retryable}): {reason}"
                )
            }
            Self::PacketTooLarge {
                size_bytes,
                cap_bytes,
            } => {
                write!(
                    f,
                    "[HLE-2611] packet too large: {size_bytes} bytes > cap {cap_bytes}"
                )
            }
            Self::WriteGateSealed { bridge } => {
                write!(f, "[HLE-2612] write gate sealed on bridge={bridge}")
            }
        }
    }
}

impl std::error::Error for ZellijDispatchError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_contract::{xor_fold_sha256_stub, AuthGate};

    fn valid_target() -> PaneTarget {
        PaneTarget::new(0, "ALPHA").expect("valid target")
    }

    fn valid_token() -> WriteAuthToken {
        AuthGate::default()
            .issue_token(1, CapabilityClass::LiveWrite, 1000)
            .expect("valid token")
    }

    // ── PaneTarget ───────────────────────────────────────────────────────────

    #[test]
    fn pane_target_new_valid() {
        let t = valid_target();
        assert_eq!(t.tab(), 0);
        assert_eq!(t.pane_label(), "ALPHA");
    }

    #[test]
    fn pane_target_rejects_tab_too_large() {
        assert!(PaneTarget::new(64, "LABEL").is_err());
    }

    #[test]
    fn pane_target_accepts_max_tab() {
        assert!(PaneTarget::new(ZELLIJ_MAX_TAB, "LABEL").is_ok());
    }

    #[test]
    fn pane_target_rejects_empty_label() {
        assert!(PaneTarget::new(0, "").is_err());
    }

    #[test]
    fn pane_target_rejects_non_ascii_label() {
        assert!(PaneTarget::new(0, "héllo").is_err());
    }

    #[test]
    fn pane_target_display_format() {
        let t = valid_target();
        assert_eq!(t.to_string(), "tab=0:ALPHA");
    }

    // ── ZellijPacket ─────────────────────────────────────────────────────────

    #[test]
    fn packet_new_valid() {
        let p = ZellijPacket::new(valid_target(), "hello", false).expect("valid packet");
        assert_eq!(p.chars, "hello");
        assert!(!p.trailing_cr);
    }

    #[test]
    fn packet_byte_len_with_cr() {
        let p = ZellijPacket::new(valid_target(), "hi", true).expect("valid packet");
        assert_eq!(p.byte_len(), 3); // "hi" (2) + CR (1)
    }

    #[test]
    fn packet_byte_len_without_cr() {
        let p = ZellijPacket::new(valid_target(), "hi", false).expect("valid packet");
        assert_eq!(p.byte_len(), 2);
    }

    #[test]
    fn packet_rejects_over_cap() {
        let big = "x".repeat(ZELLIJ_PACKET_CHAR_CAP + 1);
        let err = ZellijPacket::new(valid_target(), big, false).expect_err("must fail");
        assert_eq!(err.error_code(), 2611);
    }

    #[test]
    fn packet_accepts_at_cap() {
        let at_cap = "x".repeat(ZELLIJ_PACKET_CHAR_CAP);
        assert!(ZellijPacket::new(valid_target(), at_cap, false).is_ok());
    }

    #[test]
    fn packet_validate_catches_over_cap() {
        let mut p = ZellijPacket::new(valid_target(), "hi", false).expect("valid");
        p.chars = "x".repeat(ZELLIJ_PACKET_CHAR_CAP + 1);
        assert!(p.validate().is_err());
    }

    // ── ReadOnly bridge ──────────────────────────────────────────────────────

    #[test]
    fn read_only_capability_class_is_read_only() {
        let b = ZellijDispatch::new_read_only();
        assert_eq!(b.capability_class(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn read_only_supports_write_is_false() {
        let b = ZellijDispatch::new_read_only();
        assert!(!b.supports_write());
    }

    #[test]
    fn read_only_schema_id() {
        let b = ZellijDispatch::new_read_only();
        assert_eq!(b.schema_id(), "hle.zellij.v1");
    }

    #[test]
    fn read_only_port_is_none() {
        let b = ZellijDispatch::new_read_only();
        assert!(b.port().is_none());
    }

    #[test]
    fn read_only_validate_packet_ok() {
        let b = ZellijDispatch::new_read_only();
        let p = ZellijPacket::new(valid_target(), "hello", false).expect("valid");
        assert!(b.validate_packet(&p).is_ok());
    }

    // ── WriteGateSealed guard ────────────────────────────────────────────────

    #[test]
    fn dispatch_packet_returns_receipt_on_valid_token() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let pkt = ZellijPacket::new(valid_target(), "cmd", false).expect("valid");
        let receipt = b.dispatch_packet(&pkt, &tok).expect("must succeed");
        assert_eq!(receipt.schema_id, "hle.zellij.v1");
    }

    #[test]
    fn dispatch_packet_receipt_sha_matches_chars() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let pkt = ZellijPacket::new(valid_target(), "test", false).expect("valid");
        let receipt = b.dispatch_packet(&pkt, &tok).expect("must succeed");
        let expected = xor_fold_sha256_stub(b"test");
        assert_eq!(receipt.payload_sha256, expected);
    }

    #[test]
    fn dispatch_packet_receipt_has_auth_receipt_id() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let pkt = ZellijPacket::new(valid_target(), "x", false).expect("valid");
        let receipt = b.dispatch_packet(&pkt, &tok).expect("must succeed");
        assert_eq!(receipt.auth_receipt_id, Some(1));
    }

    // ── DispatchOutcome ──────────────────────────────────────────────────────

    #[test]
    fn dispatch_outcome_sent_display() {
        let o = DispatchOutcome::Sent { bytes: 42 };
        assert!(o.to_string().contains("42"));
    }

    #[test]
    fn dispatch_outcome_skipped_display() {
        let o = DispatchOutcome::Skipped {
            reason: SkipReason::BinaryAbsent,
        };
        assert!(o.to_string().contains("binary-absent"));
    }

    #[test]
    fn skip_reason_dry_run_display() {
        assert_eq!(SkipReason::DryRun.to_string(), "dry-run");
    }

    // ── Error codes ──────────────────────────────────────────────────────────

    #[test]
    fn dispatch_failed_error_code_is_2610() {
        let e = ZellijDispatchError::DispatchFailed {
            tab: 0,
            pane_label: String::from("L"),
            reason: String::from("r"),
            retryable: false,
        };
        assert_eq!(e.error_code(), 2610);
    }

    #[test]
    fn packet_too_large_error_code_is_2611() {
        let e = ZellijDispatchError::PacketTooLarge {
            size_bytes: 5000,
            cap_bytes: 4096,
        };
        assert_eq!(e.error_code(), 2611);
    }

    #[test]
    fn write_gate_sealed_error_code_is_2612() {
        let e = ZellijDispatchError::WriteGateSealed {
            bridge: "zellij_dispatch",
        };
        assert_eq!(e.error_code(), 2612);
    }

    #[test]
    fn dispatch_failed_retryable_is_retryable() {
        let e = ZellijDispatchError::DispatchFailed {
            tab: 0,
            pane_label: String::from("L"),
            reason: String::from("eagain"),
            retryable: true,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn write_gate_sealed_is_not_retryable() {
        let e = ZellijDispatchError::WriteGateSealed {
            bridge: "zellij_dispatch",
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn error_display_contains_code_prefix() {
        let e = ZellijDispatchError::WriteGateSealed {
            bridge: "zellij_dispatch",
        };
        assert!(e.to_string().contains("[HLE-2612]"));
    }

    // ── PaneTarget additional ────────────────────────────────────────────────

    #[test]
    fn pane_target_tab_zero_is_valid() {
        assert!(PaneTarget::new(0, "P1").is_ok());
    }

    #[test]
    fn pane_target_tab_at_max_boundary() {
        let t = PaneTarget::new(ZELLIJ_MAX_TAB, "X").expect("valid");
        assert_eq!(t.tab(), ZELLIJ_MAX_TAB);
    }

    #[test]
    fn pane_target_over_max_tab_produces_2610() {
        let err = PaneTarget::new(ZELLIJ_MAX_TAB + 1, "X").expect_err("fail");
        assert_eq!(err.error_code(), 2610);
    }

    #[test]
    fn pane_target_clone_equality() {
        let t = PaneTarget::new(1, "BETA").expect("valid");
        let c = t.clone();
        assert_eq!(t, c);
    }

    #[test]
    fn pane_target_display_includes_tab_and_label() {
        let t = PaneTarget::new(3, "GAMMA").expect("valid");
        let s = t.to_string();
        assert!(s.contains("3"));
        assert!(s.contains("GAMMA"));
    }

    #[test]
    fn pane_target_ascii_label_with_hyphen_is_valid() {
        assert!(PaneTarget::new(0, "ALPHA-LEFT").is_ok());
    }

    #[test]
    fn pane_target_numeric_label_is_valid() {
        assert!(PaneTarget::new(0, "12").is_ok());
    }

    // ── ZellijPacket additional ──────────────────────────────────────────────

    #[test]
    fn packet_default_timeout_is_five_seconds() {
        let p = ZellijPacket::new(valid_target(), "cmd", false).expect("valid");
        assert_eq!(p.timeout.as_duration(), std::time::Duration::from_secs(5));
    }

    #[test]
    fn packet_empty_chars_is_valid() {
        assert!(ZellijPacket::new(valid_target(), "", false).is_ok());
    }

    #[test]
    fn packet_byte_len_at_cap_without_cr() {
        let at_cap = "x".repeat(ZELLIJ_PACKET_CHAR_CAP);
        let p = ZellijPacket::new(valid_target(), at_cap, false).expect("valid");
        assert_eq!(p.byte_len(), ZELLIJ_PACKET_CHAR_CAP);
    }

    #[test]
    fn packet_byte_len_at_cap_with_cr() {
        let at_cap = "x".repeat(ZELLIJ_PACKET_CHAR_CAP);
        let p = ZellijPacket::new(valid_target(), at_cap, true).expect("valid");
        assert_eq!(p.byte_len(), ZELLIJ_PACKET_CHAR_CAP + 1);
    }

    #[test]
    fn packet_display_contains_tab_and_pane() {
        let t = PaneTarget::new(2, "DELTA").expect("valid");
        let p = ZellijPacket::new(t, "data", false).expect("valid");
        let s = p.to_string();
        assert!(s.contains("2"));
        assert!(s.contains("DELTA"));
    }

    // ── LiveWrite bridge additional ──────────────────────────────────────────

    #[test]
    fn live_write_capability_class_is_live_write() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        assert_eq!(b.capability_class(), CapabilityClass::LiveWrite);
    }

    #[test]
    fn live_write_supports_write_is_true() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        assert!(b.supports_write());
    }

    #[test]
    fn live_write_schema_id_matches_read_only() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        assert_eq!(b.schema_id(), "hle.zellij.v1");
    }

    #[test]
    fn live_write_paths_not_empty() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        assert!(!b.paths().is_empty());
    }

    #[test]
    fn live_write_name_matches_read_only() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        assert_eq!(b.name(), "zellij_dispatch");
    }

    #[test]
    fn live_write_validate_packet_rejects_over_cap() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let mut p = ZellijPacket::new(valid_target(), "hi", false).expect("valid");
        p.chars = "x".repeat(ZELLIJ_PACKET_CHAR_CAP + 1);
        assert!(b.validate_packet(&p).is_err());
    }

    #[test]
    fn dispatch_packet_over_cap_returns_2611() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let mut p = ZellijPacket::new(valid_target(), "ok", false).expect("valid");
        p.chars = "x".repeat(ZELLIJ_PACKET_CHAR_CAP + 1);
        let err = b.dispatch_packet(&p, &tok).expect_err("must fail");
        assert_eq!(err.error_code(), 2611);
    }

    #[test]
    fn dispatch_packet_receipt_operation_is_dispatch_packet() {
        let tok = valid_token();
        let b = ZellijDispatch::new_live_write(&tok).expect("valid");
        let pkt = ZellijPacket::new(valid_target(), "data", false).expect("valid");
        let receipt = b.dispatch_packet(&pkt, &tok).expect("must succeed");
        assert_eq!(receipt.operation, "dispatch_packet");
    }

    // ── DispatchOutcome / SkipReason additional ──────────────────────────────

    #[test]
    fn dispatch_outcome_sent_zero_bytes() {
        let o = DispatchOutcome::Sent { bytes: 0 };
        assert!(matches!(o, DispatchOutcome::Sent { bytes: 0 }));
    }

    #[test]
    fn dispatch_outcome_skipped_dry_run() {
        let o = DispatchOutcome::Skipped {
            reason: SkipReason::DryRun,
        };
        assert!(matches!(
            o,
            DispatchOutcome::Skipped {
                reason: SkipReason::DryRun
            }
        ));
    }

    #[test]
    fn skip_reason_binary_absent_display() {
        assert_eq!(SkipReason::BinaryAbsent.to_string(), "binary-absent");
    }

    #[test]
    fn dispatch_outcome_skipped_binary_absent_display() {
        let o = DispatchOutcome::Skipped {
            reason: SkipReason::BinaryAbsent,
        };
        assert!(o.to_string().contains("binary-absent"));
    }

    // ── ZellijDispatchError additional ───────────────────────────────────────

    #[test]
    fn dispatch_failed_non_retryable_is_not_retryable() {
        let e = ZellijDispatchError::DispatchFailed {
            tab: 1,
            pane_label: String::from("X"),
            reason: String::from("perm"),
            retryable: false,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn packet_too_large_is_not_retryable() {
        let e = ZellijDispatchError::PacketTooLarge {
            size_bytes: 5000,
            cap_bytes: 4096,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn dispatch_failed_display_contains_tab() {
        let e = ZellijDispatchError::DispatchFailed {
            tab: 7,
            pane_label: String::from("ZETA"),
            reason: String::from("r"),
            retryable: false,
        };
        assert!(e.to_string().contains("7"));
    }

    #[test]
    fn packet_too_large_display_contains_size() {
        let e = ZellijDispatchError::PacketTooLarge {
            size_bytes: 9999,
            cap_bytes: 4096,
        };
        assert!(e.to_string().contains("9999"));
    }

    // ── BridgeContract impl via dyn ──────────────────────────────────────────

    #[test]
    fn read_only_port_is_none_via_dyn() {
        let b: std::sync::Arc<dyn BridgeContract> =
            std::sync::Arc::new(ZellijDispatch::new_read_only());
        assert!(b.port().is_none());
    }

    #[test]
    fn read_only_paths_contains_write_chars() {
        let b = ZellijDispatch::new_read_only();
        assert!(b.paths().iter().any(|p| p.contains("write")));
    }
}
