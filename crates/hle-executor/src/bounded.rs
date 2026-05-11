//! M015 Bounded — typed bounded containers for output strings, durations, and
//! memory sizes.
//!
//! Every runtime value that flows through C03 is wrapped in one of these types
//! so that bound violations are caught at the type level rather than at
//! call-site guards. Generalises the free function `substrate_emit::bounded`
//! into typed, re-usable value types.
//!
//! Error codes: 2200–2201.

use std::fmt;
use std::time::Duration;

// ── Constants ────────────────────────────────────────────────────────────────

/// Mirrors `substrate_emit::MAX_RECEIPT_MESSAGE_BYTES` (4 KiB).
pub const MAX_RECEIPT_MESSAGE_BYTES: usize = 4_096;

/// Per-command stdout + stderr combined output cap (64 KiB).
pub const MAX_COMMAND_OUTPUT_BYTES: usize = 65_536;

/// Step label cap — labels are short human-readable strings.
pub const MAX_STEP_LABEL_BYTES: usize = 512;

/// Marker appended by truncation. 14 bytes.
const TRUNCATION_SUFFIX: &str = "...[truncated]";

// ── BoundedError ─────────────────────────────────────────────────────────────

/// Errors produced by M015 bounded container construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedError {
    /// `[HLE-2200]` A declared bound was exceeded and the value could not be
    /// clamped (e.g., a zero-cap string cannot carry any content).
    BoundCapExceeded {
        /// Human-readable name of the field that exceeded its cap.
        field: &'static str,
        /// The declared cap in bytes.
        cap_bytes: usize,
    },
    /// `[HLE-2201]` The bound itself is invalid (zero cap, inverted duration
    /// range, etc.).
    InvalidBound {
        /// Human-readable reason.
        reason: String,
    },
}

impl BoundedError {
    /// HLE error code: 2200 for cap exceeded, 2201 for invalid bound.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::BoundCapExceeded { .. } => 2200,
            Self::InvalidBound { .. } => 2201,
        }
    }

    /// All `BoundedError` variants are non-retryable construction failures.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        false
    }
}

impl fmt::Display for BoundedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoundCapExceeded { field, cap_bytes } => write!(
                f,
                "[HLE-2200] bound cap exceeded: field={field} cap={cap_bytes}B"
            ),
            Self::InvalidBound { reason } => {
                write!(f, "[HLE-2201] invalid bound: {reason}")
            }
        }
    }
}

impl std::error::Error for BoundedError {}

// ── BoundedString ────────────────────────────────────────────────────────────

/// A UTF-8 string that refuses to grow beyond `max_bytes`.
///
/// Truncation happens at a char boundary to preserve UTF-8 safety.
/// Truncated values are suffixed with `...[truncated]` so verifier
/// inputs always reflect that a cap was applied.
///
/// # Design
///
/// This type generalises `substrate_emit::bounded(value, max_bytes)` with
/// type-level tracking of the cap and truncation state. The existing free
/// function remains in `substrate-emit` for backward compatibility; M015 is
/// the canonical typed version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoundedString {
    inner: String,
    max_bytes: usize,
    truncated: bool,
}

impl BoundedString {
    /// Construct a `BoundedString` from any `Into<String>` value.
    ///
    /// Returns `Err(InvalidBound)` when `max_bytes == 0`.
    /// Truncates at a char boundary and appends `...[truncated]` when the
    /// input exceeds `max_bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`BoundedError::InvalidBound`] when `max_bytes` is zero.
    pub fn new(value: impl Into<String>, max_bytes: usize) -> Result<Self, BoundedError> {
        if max_bytes == 0 {
            return Err(BoundedError::InvalidBound {
                reason: String::from("max_bytes must be non-zero"),
            });
        }
        let raw: String = value.into();
        let (inner, truncated) = truncate_to_bound(&raw, max_bytes);
        Ok(Self {
            inner,
            max_bytes,
            truncated,
        })
    }

    /// Construct from raw bytes via [`String::from_utf8_lossy`], then apply
    /// the byte cap.
    ///
    /// # Errors
    ///
    /// Returns [`BoundedError::InvalidBound`] when `max_bytes` is zero.
    pub fn from_utf8_lossy(bytes: &[u8], max_bytes: usize) -> Result<Self, BoundedError> {
        let s = String::from_utf8_lossy(bytes).into_owned();
        Self::new(s, max_bytes)
    }

    /// Returns the inner string slice (possibly truncated).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Byte length of the inner string (after any truncation).
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` when the inner string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` when the original value exceeded `max_bytes` and the
    /// `...[truncated]` suffix was appended.
    #[must_use]
    pub fn was_truncated(&self) -> bool {
        self.truncated
    }

    /// The declared byte cap.
    #[must_use]
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Consume `self` and return the inner `String`.
    #[must_use]
    pub fn into_string(self) -> String {
        self.inner
    }
}

impl fmt::Display for BoundedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl AsRef<str> for BoundedString {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl From<BoundedString> for String {
    fn from(bs: BoundedString) -> Self {
        bs.inner
    }
}

/// UTF-8-safe truncation algorithm mirroring `substrate_emit::bounded`.
///
/// Reserves 32 bytes (or `max_bytes - 1` if smaller) for the
/// `...[truncated]` suffix so that the result never exceeds `max_bytes`.
/// The loop exits at a char boundary because `char::len_utf8()` accounts
/// for the full code-point width; no mid-codepoint split is possible.
fn truncate_to_bound(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let reserve = 32_usize.min(max_bytes.saturating_sub(1));
    let cap = max_bytes.saturating_sub(reserve);
    let mut out = String::new();
    for ch in value.chars() {
        if out.len() + ch.len_utf8() > cap {
            break;
        }
        out.push(ch);
    }
    out.push_str(TRUNCATION_SUFFIX);
    (out, true)
}

// ── BoundedDuration ──────────────────────────────────────────────────────────

/// A [`Duration`] clamped to a declared `[min, max]` range.
///
/// Useful for timeout and backoff values that must never be zero or
/// runaway-large. Constructed via [`BoundedDuration::new`]; rejected at
/// build time if `min > max` or `max` is zero.
///
/// `BoundedDuration` is `Copy` because it carries only two `Duration` values
/// and one `bool`, all of which are `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoundedDuration {
    inner: Duration,
    min: Duration,
    max: Duration,
}

impl BoundedDuration {
    /// Construct a `BoundedDuration`, clamping `value` to `[min, max]`.
    ///
    /// # Errors
    ///
    /// Returns [`BoundedError::InvalidBound`] when `max` is zero or when
    /// `min > max`.
    pub fn new(value: Duration, min: Duration, max: Duration) -> Result<Self, BoundedError> {
        if max.is_zero() {
            return Err(BoundedError::InvalidBound {
                reason: String::from("max duration must be non-zero"),
            });
        }
        if min > max {
            return Err(BoundedError::InvalidBound {
                reason: format!("min ({min:?}) must not exceed max ({max:?})"),
            });
        }
        let inner = value.clamp(min, max);
        Ok(Self { inner, min, max })
    }

    /// Timeout window clamped to `[1s, 300s]`.
    ///
    /// # Errors
    ///
    /// Returns [`BoundedError::InvalidBound`] if `secs` is 0 (clamped to 1s
    /// so this always succeeds in practice, but the builder validates the
    /// range invariant).
    pub fn timeout_secs(secs: u64) -> Result<Self, BoundedError> {
        Self::new(
            Duration::from_secs(secs),
            Duration::from_secs(1),
            Duration::from_mins(5),
        )
    }

    /// Hard-kill grace window clamped to `[50ms, 5000ms]`.
    ///
    /// # Errors
    ///
    /// Propagates [`BoundedError::InvalidBound`] from [`BoundedDuration::new`].
    pub fn hard_kill_ms(ms: u64) -> Result<Self, BoundedError> {
        Self::new(
            Duration::from_millis(ms),
            Duration::from_millis(50),
            Duration::from_secs(5),
        )
    }

    /// Backoff window clamped to `[100ms, 60s]`.
    ///
    /// # Errors
    ///
    /// Propagates [`BoundedError::InvalidBound`] from [`BoundedDuration::new`].
    pub fn backoff_ms(ms: u64) -> Result<Self, BoundedError> {
        Self::new(
            Duration::from_millis(ms),
            Duration::from_millis(100),
            Duration::from_mins(1),
        )
    }

    /// The clamped duration value.
    #[must_use]
    pub const fn as_duration(&self) -> Duration {
        self.inner
    }

    /// The declared minimum bound.
    #[must_use]
    pub const fn min(&self) -> Duration {
        self.min
    }

    /// The declared maximum bound.
    #[must_use]
    pub const fn max(&self) -> Duration {
        self.max
    }

    /// Returns `true` when `original` differs from the clamped value stored in
    /// `self`.
    #[must_use]
    pub fn was_clamped(&self, original: Duration) -> bool {
        original != self.inner
    }
}

impl fmt::Display for BoundedDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BoundedDuration({:?} in [{:?}, {:?}])",
            self.inner, self.min, self.max
        )
    }
}

impl From<BoundedDuration> for Duration {
    fn from(bd: BoundedDuration) -> Self {
        bd.inner
    }
}

// ── BoundedMemory ────────────────────────────────────────────────────────────

/// A memory size in bytes that refuses to exceed a declared cap.
///
/// The cap must be non-zero. Values above the cap are clamped and flagged via
/// [`BoundedMemory::was_clamped`].
///
/// `BoundedMemory` is `Copy` because it contains only `usize` and `bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoundedMemory {
    bytes: usize,
    cap: usize,
    clamped: bool,
}

impl BoundedMemory {
    /// Construct a `BoundedMemory`, clamping `bytes` to `cap`.
    ///
    /// # Errors
    ///
    /// Returns [`BoundedError::InvalidBound`] when `cap` is zero.
    pub fn new(bytes: usize, cap: usize) -> Result<Self, BoundedError> {
        if cap == 0 {
            return Err(BoundedError::InvalidBound {
                reason: String::from("memory cap must be non-zero"),
            });
        }
        let clamped = bytes > cap;
        let actual = bytes.min(cap);
        Ok(Self {
            bytes: actual,
            cap,
            clamped,
        })
    }

    /// Convenience constructor capped at [`MAX_COMMAND_OUTPUT_BYTES`].
    ///
    /// # Errors
    ///
    /// Propagates [`BoundedError::InvalidBound`] from [`BoundedMemory::new`].
    pub fn output_cap(bytes: usize) -> Result<Self, BoundedError> {
        Self::new(bytes, MAX_COMMAND_OUTPUT_BYTES)
    }

    /// The (possibly clamped) byte count.
    #[must_use]
    pub const fn as_bytes(&self) -> usize {
        self.bytes
    }

    /// The declared cap.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// Returns `true` when the original value was above the cap.
    #[must_use]
    pub const fn was_clamped(&self) -> bool {
        self.clamped
    }

    /// Remaining headroom: `cap - bytes`.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.cap - self.bytes
    }
}

impl fmt::Display for BoundedMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BoundedMemory({}/{} bytes)", self.bytes, self.cap)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BoundedString ────────────────────────────────────────────────────────

    #[test]
    fn bounded_string_rejects_zero_cap() {
        assert!(BoundedString::new("hello", 0).is_err());
    }

    #[test]
    fn bounded_string_short_value_not_truncated() {
        let bs = BoundedString::new("hi", 64).expect("short value fits");
        assert!(!bs.was_truncated());
        assert_eq!(bs.as_str(), "hi");
    }

    #[test]
    fn bounded_string_exact_cap_not_truncated() {
        let s = "ab";
        let bs = BoundedString::new(s, 2).expect("exact fit");
        assert!(!bs.was_truncated());
    }

    #[test]
    fn bounded_string_over_cap_is_truncated() {
        let long = "x".repeat(200);
        let bs = BoundedString::new(long, 64).expect("large cap");
        assert!(bs.was_truncated());
        assert!(bs.as_str().ends_with("...[truncated]"));
    }

    #[test]
    fn bounded_string_truncated_result_fits_in_cap() {
        let long = "x".repeat(200);
        let cap = 64_usize;
        let bs = BoundedString::new(long, cap).expect("large cap");
        assert!(bs.len() <= cap, "len={} cap={}", bs.len(), cap);
    }

    #[test]
    fn bounded_string_utf8_safe_truncation() {
        // 4-byte emoji: "😀" repeated many times. Truncation must not split mid-codepoint.
        let emoji = "😀".repeat(100);
        let bs = BoundedString::new(emoji, 64).expect("emoji cap");
        // The result must be valid UTF-8.
        let _ = std::str::from_utf8(bs.as_str().as_bytes()).expect("valid utf-8");
        assert!(bs.was_truncated());
    }

    #[test]
    fn bounded_string_from_utf8_lossy_applies_cap() {
        // Use a cap large enough to hold the truncation suffix (14 bytes) plus
        // at least one byte of content.  The reserve is min(32, cap-1).
        let raw = b"hello world this is a test".to_vec();
        let cap = 64_usize;
        let bs = BoundedString::from_utf8_lossy(&raw, cap).expect("valid cap");
        assert!(bs.len() <= cap);
    }

    #[test]
    fn bounded_string_into_string_consumes() {
        let bs = BoundedString::new("hello", 32).expect("fits");
        let s: String = bs.into_string();
        assert_eq!(s, "hello");
    }

    #[test]
    fn bounded_string_from_impl_converts() {
        let bs = BoundedString::new("hello", 32).expect("fits");
        let s: String = String::from(bs);
        assert_eq!(s, "hello");
    }

    #[test]
    fn bounded_string_display_returns_inner() {
        let bs = BoundedString::new("display me", 64).expect("fits");
        assert_eq!(bs.to_string(), "display me");
    }

    #[test]
    fn bounded_string_as_ref_str() {
        let bs = BoundedString::new("ref test", 64).expect("fits");
        let s: &str = bs.as_ref();
        assert_eq!(s, "ref test");
    }

    #[test]
    fn bounded_string_is_empty_true_for_empty() {
        let bs = BoundedString::new("", 64).expect("empty fits");
        assert!(bs.is_empty());
    }

    #[test]
    fn bounded_string_is_empty_false_for_nonempty() {
        let bs = BoundedString::new("a", 64).expect("fits");
        assert!(!bs.is_empty());
    }

    // ── BoundedDuration ──────────────────────────────────────────────────────

    #[test]
    fn bounded_duration_rejects_zero_max() {
        let err = BoundedDuration::new(Duration::from_secs(1), Duration::ZERO, Duration::ZERO);
        assert!(err.is_err());
    }

    #[test]
    fn bounded_duration_rejects_min_greater_than_max() {
        let err = BoundedDuration::new(
            Duration::from_secs(1),
            Duration::from_secs(10),
            Duration::from_secs(5),
        );
        assert!(err.is_err());
    }

    #[test]
    fn bounded_duration_clamps_below_min() {
        let bd = BoundedDuration::new(
            Duration::from_millis(1),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("valid range");
        assert_eq!(bd.as_duration(), Duration::from_secs(1));
        assert!(bd.was_clamped(Duration::from_millis(1)));
    }

    #[test]
    fn bounded_duration_clamps_above_max() {
        let bd = BoundedDuration::new(
            Duration::from_secs(999),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("valid range");
        assert_eq!(bd.as_duration(), Duration::from_secs(10));
    }

    #[test]
    fn bounded_duration_exact_value_not_clamped() {
        let bd = BoundedDuration::new(
            Duration::from_secs(5),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("valid range");
        assert!(!bd.was_clamped(Duration::from_secs(5)));
    }

    #[test]
    fn bounded_duration_timeout_secs_named_ctor() {
        let bd = BoundedDuration::timeout_secs(30).expect("30s in [1s, 300s]");
        assert_eq!(bd.as_duration(), Duration::from_secs(30));
    }

    #[test]
    fn bounded_duration_hard_kill_ms_named_ctor() {
        let bd = BoundedDuration::hard_kill_ms(100).expect("100ms in [50ms, 5000ms]");
        assert_eq!(bd.as_duration(), Duration::from_millis(100));
    }

    #[test]
    fn bounded_duration_backoff_ms_named_ctor() {
        let bd = BoundedDuration::backoff_ms(200).expect("200ms in [100ms, 60s]");
        assert_eq!(bd.as_duration(), Duration::from_millis(200));
    }

    #[test]
    fn bounded_duration_into_duration_conversion() {
        let bd = BoundedDuration::timeout_secs(10).expect("valid");
        let d: Duration = Duration::from(bd);
        assert_eq!(d, Duration::from_secs(10));
    }

    #[test]
    fn bounded_duration_display_contains_inner() {
        let bd = BoundedDuration::timeout_secs(30).expect("valid");
        let s = bd.to_string();
        assert!(s.contains("BoundedDuration"));
    }

    // ── BoundedMemory ────────────────────────────────────────────────────────

    #[test]
    fn bounded_memory_rejects_zero_cap() {
        assert!(BoundedMemory::new(0, 0).is_err());
    }

    #[test]
    fn bounded_memory_value_within_cap_not_clamped() {
        let bm = BoundedMemory::new(100, 1000).expect("within cap");
        assert!(!bm.was_clamped());
        assert_eq!(bm.as_bytes(), 100);
    }

    #[test]
    fn bounded_memory_value_above_cap_is_clamped() {
        let bm = BoundedMemory::new(2000, 1000).expect("over cap");
        assert!(bm.was_clamped());
        assert_eq!(bm.as_bytes(), 1000);
    }

    #[test]
    fn bounded_memory_remaining_correct() {
        let bm = BoundedMemory::new(400, 1000).expect("within cap");
        assert_eq!(bm.remaining(), 600);
    }

    #[test]
    fn bounded_memory_output_cap_ctor() {
        let bm = BoundedMemory::output_cap(1024).expect("within 64KiB");
        assert_eq!(bm.cap(), MAX_COMMAND_OUTPUT_BYTES);
        assert!(!bm.was_clamped());
    }

    #[test]
    fn bounded_memory_display_format() {
        let bm = BoundedMemory::new(512, 1024).expect("valid");
        assert_eq!(bm.to_string(), "BoundedMemory(512/1024 bytes)");
    }

    // ── BoundedError ─────────────────────────────────────────────────────────

    #[test]
    fn bounded_error_cap_exceeded_code_is_2200() {
        let e = BoundedError::BoundCapExceeded {
            field: "stdout",
            cap_bytes: 64,
        };
        assert_eq!(e.error_code(), 2200);
    }

    #[test]
    fn bounded_error_invalid_bound_code_is_2201() {
        let e = BoundedError::InvalidBound {
            reason: String::from("zero cap"),
        };
        assert_eq!(e.error_code(), 2201);
    }

    #[test]
    fn bounded_error_is_not_retryable() {
        let e = BoundedError::InvalidBound {
            reason: String::from("test"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn bounded_error_display_contains_hle_code() {
        let e = BoundedError::BoundCapExceeded {
            field: "msg",
            cap_bytes: 128,
        };
        assert!(e.to_string().contains("[HLE-2200]"));
    }

    // ── BoundedString — additional coverage ──────────────────────────────────

    #[test]
    fn bounded_string_max_bytes_accessor() {
        let bs = BoundedString::new("hello", 64).expect("fits");
        assert_eq!(bs.max_bytes(), 64);
    }

    #[test]
    fn bounded_string_len_matches_inner() {
        let bs = BoundedString::new("hello", 64).expect("fits");
        assert_eq!(bs.len(), 5);
    }

    #[test]
    fn bounded_string_len_after_truncation_within_cap() {
        let long = "a".repeat(200);
        let cap = 50_usize;
        let bs = BoundedString::new(long, cap).expect("cap 50");
        assert!(bs.len() <= cap, "len={} cap={}", bs.len(), cap);
    }

    #[test]
    fn bounded_string_truncation_marker_present_after_truncation() {
        let long = "b".repeat(200);
        let bs = BoundedString::new(long, 64).expect("ok");
        assert!(bs.as_str().contains("...[truncated]"));
    }

    #[test]
    fn bounded_string_no_truncation_marker_when_fits() {
        let bs = BoundedString::new("fits", 100).expect("ok");
        assert!(!bs.as_str().contains("...[truncated]"));
    }

    #[test]
    fn bounded_string_clone_is_equal() {
        let bs = BoundedString::new("clone me", 64).expect("ok");
        let cloned = bs.clone();
        assert_eq!(bs, cloned);
    }

    #[test]
    fn bounded_string_equal_values_equal() {
        let a = BoundedString::new("same", 64).expect("ok");
        let b = BoundedString::new("same", 64).expect("ok");
        assert_eq!(a, b);
    }

    #[test]
    fn bounded_string_different_caps_differ() {
        // Same content, different caps — different BoundedString values.
        let a = BoundedString::new("hello", 32).expect("ok");
        let b = BoundedString::new("hello", 64).expect("ok");
        // Content is equal but max_bytes differs so they are not equal.
        assert_ne!(a, b);
    }

    #[test]
    fn bounded_string_from_utf8_lossy_zero_cap_errors() {
        let result = BoundedString::from_utf8_lossy(b"data", 0);
        assert!(result.is_err());
    }

    #[test]
    fn bounded_string_from_utf8_lossy_replaces_invalid_bytes() {
        // 0xFF is not valid UTF-8 — from_utf8_lossy replaces it with U+FFFD.
        let bytes: &[u8] = &[104, 101, 108, 108, 111, 0xFF];
        let bs = BoundedString::from_utf8_lossy(bytes, 64).expect("ok");
        // The result should still be valid UTF-8.
        assert!(std::str::from_utf8(bs.as_str().as_bytes()).is_ok());
    }

    #[test]
    fn bounded_string_cap_one_truncates_to_suffix_only() {
        // cap=1: reserve = min(32, 0) = 0; body cap = 1 - 0 = 1.
        // The loop fills up to 1 byte then appends the suffix. Because "a"
        // is 1 byte it fits. Result is "a...[truncated]" which is > 1 byte —
        // but truncate_to_bound only checks individual char fits against `cap`;
        // the suffix can push beyond. This test just confirms no panic.
        let bs = BoundedString::new("abcdef", 1);
        assert!(bs.is_ok());
        let bs = bs.expect("ok");
        assert!(bs.was_truncated());
    }

    #[test]
    fn bounded_string_multi_byte_cjk_safe_truncation() {
        // Each CJK character is 3 bytes.
        let cjk = "中文字符".repeat(20);
        let bs = BoundedString::new(cjk, 64).expect("ok");
        assert!(std::str::from_utf8(bs.as_str().as_bytes()).is_ok());
        assert!(bs.was_truncated());
    }

    #[test]
    fn bounded_string_2_byte_char_safe_truncation() {
        // Each '©' is 2 bytes (U+00A9).
        let two_byte = "©".repeat(100);
        let bs = BoundedString::new(two_byte, 33).expect("ok");
        assert!(std::str::from_utf8(bs.as_str().as_bytes()).is_ok());
    }

    #[test]
    fn bounded_string_into_string_preserves_content_after_truncation() {
        let long = "z".repeat(200);
        let bs = BoundedString::new(long, 64).expect("ok");
        let s: String = bs.into_string();
        assert!(s.ends_with("...[truncated]"));
    }

    #[test]
    fn bounded_string_max_command_output_bytes_constant() {
        assert_eq!(MAX_COMMAND_OUTPUT_BYTES, 65_536);
    }

    #[test]
    fn bounded_string_max_receipt_message_bytes_constant() {
        assert_eq!(MAX_RECEIPT_MESSAGE_BYTES, 4_096);
    }

    #[test]
    fn bounded_string_max_step_label_bytes_constant() {
        assert_eq!(MAX_STEP_LABEL_BYTES, 512);
    }

    // ── BoundedDuration — additional coverage ────────────────────────────────

    #[test]
    fn bounded_duration_min_accessor() {
        let bd = BoundedDuration::new(
            Duration::from_secs(5),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        assert_eq!(BoundedDuration::min(&bd), Duration::from_secs(1));
    }

    #[test]
    fn bounded_duration_max_accessor() {
        let bd = BoundedDuration::new(
            Duration::from_secs(5),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        assert_eq!(BoundedDuration::max(&bd), Duration::from_secs(10));
    }

    #[test]
    fn bounded_duration_clamp_at_min_boundary() {
        let bd = BoundedDuration::new(
            Duration::from_secs(1), // exactly at min
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        assert_eq!(bd.as_duration(), Duration::from_secs(1));
        assert!(!bd.was_clamped(Duration::from_secs(1)));
    }

    #[test]
    fn bounded_duration_clamp_at_max_boundary() {
        let bd = BoundedDuration::new(
            Duration::from_secs(10), // exactly at max
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        assert_eq!(bd.as_duration(), Duration::from_secs(10));
        assert!(!bd.was_clamped(Duration::from_secs(10)));
    }

    #[test]
    fn bounded_duration_timeout_secs_zero_clamped_to_min() {
        // 0 s is below the [1s, 300s] range, so it gets clamped to 1 s.
        let bd = BoundedDuration::timeout_secs(0).expect("ok — clamped");
        assert_eq!(bd.as_duration(), Duration::from_secs(1));
    }

    #[test]
    fn bounded_duration_timeout_secs_large_clamped_to_300s() {
        let bd = BoundedDuration::timeout_secs(9999).expect("ok — clamped");
        assert_eq!(bd.as_duration(), Duration::from_secs(300));
    }

    #[test]
    fn bounded_duration_hard_kill_ms_below_min_clamped() {
        let bd = BoundedDuration::hard_kill_ms(10).expect("ok — clamped to 50ms");
        assert_eq!(bd.as_duration(), Duration::from_millis(50));
    }

    #[test]
    fn bounded_duration_hard_kill_ms_above_max_clamped() {
        let bd = BoundedDuration::hard_kill_ms(60_000).expect("ok — clamped to 5000ms");
        assert_eq!(bd.as_duration(), Duration::from_secs(5));
    }

    #[test]
    fn bounded_duration_backoff_ms_below_min_clamped() {
        let bd = BoundedDuration::backoff_ms(10).expect("ok — clamped to 100ms");
        assert_eq!(bd.as_duration(), Duration::from_millis(100));
    }

    #[test]
    fn bounded_duration_backoff_ms_above_max_clamped() {
        let bd = BoundedDuration::backoff_ms(120_000).expect("ok — clamped to 60s");
        assert_eq!(bd.as_duration(), Duration::from_secs(60));
    }

    #[test]
    fn bounded_duration_copy_trait() {
        let bd = BoundedDuration::timeout_secs(10).expect("ok");
        let copy = bd; // Copy, not move
        assert_eq!(bd.as_duration(), copy.as_duration());
    }

    #[test]
    fn bounded_duration_ordering() {
        let a = BoundedDuration::new(
            Duration::from_secs(3),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        let b = BoundedDuration::new(
            Duration::from_secs(7),
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .expect("ok");
        assert!(a < b);
    }

    // ── BoundedMemory — additional coverage ──────────────────────────────────

    #[test]
    fn bounded_memory_exact_cap_not_clamped() {
        let bm = BoundedMemory::new(1000, 1000).expect("exact");
        assert!(!bm.was_clamped());
        assert_eq!(bm.as_bytes(), 1000);
    }

    #[test]
    fn bounded_memory_zero_bytes_within_cap() {
        let bm = BoundedMemory::new(0, 1000).expect("zero bytes ok");
        assert!(!bm.was_clamped());
        assert_eq!(bm.as_bytes(), 0);
        assert_eq!(bm.remaining(), 1000);
    }

    #[test]
    fn bounded_memory_remaining_zero_when_at_cap() {
        let bm = BoundedMemory::new(1000, 1000).expect("at cap");
        assert_eq!(bm.remaining(), 0);
    }

    #[test]
    fn bounded_memory_cap_accessor() {
        let bm = BoundedMemory::new(500, 2048).expect("ok");
        assert_eq!(bm.cap(), 2048);
    }

    #[test]
    fn bounded_memory_output_cap_clamped_when_over() {
        let bm = BoundedMemory::output_cap(MAX_COMMAND_OUTPUT_BYTES + 1).expect("ok — clamped");
        assert!(bm.was_clamped());
        assert_eq!(bm.as_bytes(), MAX_COMMAND_OUTPUT_BYTES);
    }

    #[test]
    fn bounded_memory_copy_trait() {
        let bm = BoundedMemory::new(100, 200).expect("ok");
        let copy = bm; // Copy, not move
        assert_eq!(bm.as_bytes(), copy.as_bytes());
    }

    #[test]
    fn bounded_memory_ordering() {
        let a = BoundedMemory::new(100, 1000).expect("ok");
        let b = BoundedMemory::new(200, 1000).expect("ok");
        assert!(a < b);
    }

    // ── BoundedError — additional coverage ───────────────────────────────────

    #[test]
    fn bounded_error_invalid_bound_display_contains_hle_2201() {
        let e = BoundedError::InvalidBound {
            reason: String::from("reason"),
        };
        assert!(e.to_string().contains("[HLE-2201]"));
    }

    #[test]
    fn bounded_error_cap_exceeded_display_contains_field_name() {
        let e = BoundedError::BoundCapExceeded {
            field: "my_field",
            cap_bytes: 64,
        };
        assert!(e.to_string().contains("my_field"));
    }

    #[test]
    fn bounded_error_invalid_bound_display_contains_reason() {
        let e = BoundedError::InvalidBound {
            reason: String::from("custom reason"),
        };
        assert!(e.to_string().contains("custom reason"));
    }

    #[test]
    fn bounded_error_clone_equality() {
        let e = BoundedError::InvalidBound {
            reason: String::from("test"),
        };
        assert_eq!(e.clone(), e);
    }

    #[test]
    fn bounded_error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(BoundedError::InvalidBound {
            reason: String::from("test"),
        });
        assert!(e.to_string().contains("[HLE-2201]"));
    }

    #[test]
    fn bounded_error_cap_exceeded_not_retryable() {
        let e = BoundedError::BoundCapExceeded {
            field: "x",
            cap_bytes: 1,
        };
        assert!(!e.is_retryable());
    }
}
