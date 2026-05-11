# M015 Bounded — bounded.rs

> **File:** `crates/hle-executor/src/bounded.rs` | **Target LOC:** ~280 | **Target Tests:** 55
> **Layer:** L03 | **Cluster:** C03_BOUNDED_EXECUTION | **Error Codes:** 2200-2201
> **Role:** Primitive bounded containers for output strings, durations, and memory sizes. Generalizes `substrate_emit::bounded(value, max_bytes)` into typed, re-usable value types.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `BoundedString` | struct | No | UTF-8 string capped at a declared max byte count |
| `BoundedDuration` | struct | Yes | Duration clamped to `[min, max]` with declared units |
| `BoundedMemory` | struct | Yes | Memory size capped in bytes with explicit cap |
| `BoundedError` | enum | No | Errors 2200-2201 for this module |

---

## BoundedString

```rust
/// A UTF-8 string that refuses to grow beyond `max_bytes`.
///
/// Truncation happens at a char boundary to preserve UTF-8 safety.
/// Truncated values are suffixed with `...[truncated]` so verifier
/// inputs always reflect that a cap was applied.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoundedString {
    inner: String,
    max_bytes: usize,
    truncated: bool,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(value: impl Into<String>, max_bytes: usize) -> Result<Self, BoundedError>` | Returns `Err(InvalidBound)` when `max_bytes == 0`. Truncates if `value.len() > max_bytes`. |
| `from_utf8_lossy` | `fn(bytes: &[u8], max_bytes: usize) -> Result<Self, BoundedError>` | Converts bytes via `String::from_utf8_lossy` then applies bound. |
| `as_str` | `fn(&self) -> &str` | #[must_use]. Returns inner string (possibly truncated). |
| `len` | `fn(&self) -> usize` | #[must_use]. Byte length of inner string. |
| `is_empty` | `fn(&self) -> bool` | #[must_use] |
| `was_truncated` | `fn(&self) -> bool` | #[must_use]. True when the original value exceeded `max_bytes`. |
| `max_bytes` | `fn(&self) -> usize` | #[must_use]. Declared cap. |
| `into_string` | `fn(self) -> String` | Consumes self. |

**Traits:** `Display` (the bounded string), `AsRef<str>`, `From<BoundedString> for String`

**Truncation algorithm** (matches existing `substrate_emit::bounded`):

```rust
// Walk chars; push until adding next char would exceed (max_bytes - 32)
// then push "...[truncated]".
// This reserves 14 bytes for the suffix marker while staying within max_bytes.
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
    out.push_str("...[truncated]");
    (out, true)
}
```

The loop exits at a `char_boundary` because `char::len_utf8()` accounts for the full codepoint width. No mid-codepoint split is possible.

**Constants:**

```rust
pub const MAX_RECEIPT_MESSAGE_BYTES: usize = 4_096;   // mirrors substrate_emit
pub const MAX_COMMAND_OUTPUT_BYTES:  usize = 65_536;  // 64 KiB per-command stdout+stderr
pub const MAX_STEP_LABEL_BYTES:      usize = 512;
```

---

## BoundedDuration

```rust
/// A `std::time::Duration` clamped to a declared `[min, max]` range.
///
/// Useful for timeout and backoff values that must never be zero or
/// runaway-large. Constructed via builder; rejected at build time if
/// `min > max` or `max` is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoundedDuration {
    inner: Duration,
    min: Duration,
    max: Duration,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(value: Duration, min: Duration, max: Duration) -> Result<Self, BoundedError>` | `Err(InvalidBound)` when `min > max` or `max.is_zero()`. Clamps `value` to `[min, max]`. |
| `as_duration` | `const fn(&self) -> Duration` | #[must_use] |
| `min` | `const fn(&self) -> Duration` | #[must_use] |
| `max` | `const fn(&self) -> Duration` | #[must_use] |
| `was_clamped` | `fn(&self, original: Duration) -> bool` | #[must_use]. True when original != clamped. |

**Traits:** `Display` ("BoundedDuration(30s in [1s, 300s])"), `From<BoundedDuration> for Duration`

**Named constructors:**

```rust
impl BoundedDuration {
    /// Timeout in 1s..=300s window.
    pub fn timeout_secs(secs: u64) -> Result<Self, BoundedError> {
        Self::new(Duration::from_secs(secs), Duration::from_secs(1), Duration::from_secs(300))
    }
    /// Hard-kill grace in 50ms..=5000ms window.
    pub fn hard_kill_ms(ms: u64) -> Result<Self, BoundedError> {
        Self::new(Duration::from_millis(ms), Duration::from_millis(50), Duration::from_millis(5_000))
    }
    /// Backoff in 100ms..=60s window.
    pub fn backoff_ms(ms: u64) -> Result<Self, BoundedError> {
        Self::new(Duration::from_millis(ms), Duration::from_millis(100), Duration::from_secs(60))
    }
}
```

---

## BoundedMemory

```rust
/// A memory size in bytes that refuses to exceed a declared cap.
///
/// The cap must be non-zero. Values above the cap are clamped and flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoundedMemory {
    bytes: usize,
    cap: usize,
    clamped: bool,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(bytes: usize, cap: usize) -> Result<Self, BoundedError>` | `Err(InvalidBound)` when `cap == 0`. Clamps `bytes` to `cap`. |
| `as_bytes` | `const fn(&self) -> usize` | #[must_use] |
| `cap` | `const fn(&self) -> usize` | #[must_use] |
| `was_clamped` | `const fn(&self) -> bool` | #[must_use] |
| `remaining` | `fn(&self) -> usize` | #[must_use]. `cap - bytes`. |

**Traits:** `Display` ("BoundedMemory(4096/65536 bytes)")

**Named constructors:**

```rust
impl BoundedMemory {
    pub fn output_cap(bytes: usize) -> Result<Self, BoundedError> {
        Self::new(bytes, MAX_COMMAND_OUTPUT_BYTES)
    }
}
```

---

## BoundedError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedError {
    /// Code 2200. A declared bound was exceeded and the value could not be clamped.
    BoundCapExceeded { field: &'static str, cap_bytes: usize },
    /// Code 2201. The bound itself is invalid (zero cap, inverted range).
    InvalidBound { reason: String },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2200 or 2201 |
| `is_retryable` | `const fn(&self) -> bool` — always false |

**Traits:** `Display` ("[HLE-2200] bound cap exceeded: ..."), `std::error::Error`

---

## Design Notes

- `BoundedString` generalizes `substrate_emit::bounded(value, max_bytes)` with type-level tracking of the cap and truncation state. The existing free function remains for backward compat in `substrate-emit`; M015 is the canonical typed version.
- The `...[truncated]` suffix is 14 bytes. The reserve of 32 bytes gives headroom for multi-byte UTF-8 sequences near the cutoff boundary without overshooting `max_bytes`.
- `BoundedDuration` and `BoundedMemory` are `Copy` because they carry no heap allocation. `BoundedString` is not `Copy` for the same reason `String` is not.
- All three types prohibit zero caps at construction time. A zero `max_bytes` would silently produce an empty `BoundedString` with a truncation marker, which is semantically wrong.
- `#[forbid(unsafe_code)]` is workspace-level; M015 requires no unsafe to implement char-boundary truncation.

---

## Cluster Invariants Enforced by M015

- **I-C03-1:** All output flowing through C03 is wrapped in `BoundedString`; raw `String` output from child processes is never forwarded unwrapped.
- **I-C03-2:** Timeout values flowing through C03 are wrapped in `BoundedDuration`; raw `u64`/`Duration` timeout parameters are rejected at M017 construction.
- **I-C03-3:** UTF-8 truncation always occurs at a char boundary; `BoundedString::new` upholds this as a construction invariant.

---

*M015 Bounded Spec v1.0 | C03 Bounded Execution | 2026-05-10*
