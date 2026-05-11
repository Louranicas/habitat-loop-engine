# M020 Anti-Pattern Scanner — anti_pattern_scanner.rs

> **File:** `crates/hle-verifier/src/anti_pattern_scanner.rs` | **LOC:** ~380 | **Tests:** ~60
> **Layer:** L04 | **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE
> **Role:** Turns the anti-pattern catalog into executable per-pattern scanner instances that emit typed evidence events

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|---|---|---|---|
| `AntiPatternId` | newtype(`&'static str`) | Yes | One constant per catalogued pattern |
| `Severity` | enum | Yes | Low / Medium / High / Critical |
| `SourceLocation` | struct | No | File path + line range |
| `BoundedString` | newtype(`String`) | No | Capacity-capped evidence string (max 1024 bytes) |
| `DetectorEvent` | struct | No | Single scanner finding |
| `ScanInput` | struct | No | Target text or AST blob fed to a scanner |
| `Scanner` | trait | — | Core scanning contract |
| `AP28Scanner` | struct | No | Compositional integrity drift detector |
| `AP29Scanner` | struct | No | Blocking-in-async detector |
| `AP31Scanner` | struct | No | Nested lock detector |
| `C6Scanner` | struct | No | Lock-held signal emit detector |
| `C7Scanner` | struct | No | Lock guard reference return detector |
| `C12Scanner` | struct | No | Unbounded collections detector |
| `C13Scanner` | struct | No | Missing builder detector |
| `FalsePassClassScanner` | struct | No | False PASS class surface detector |
| `CompositeScanner` | struct | No | Runs all registered scanners, deduplicates by location |
| `ScanReport` | struct | No | Aggregated output of a `CompositeScanner` run |

---

## AntiPatternId

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AntiPatternId(&'static str);

impl AntiPatternId {
    pub const AP28: Self = Self("AP28_COMPOSITIONAL_INTEGRITY_DRIFT");
    pub const AP29: Self = Self("AP29_BLOCKING_IN_ASYNC");
    pub const AP31: Self = Self("AP31_NESTED_LOCKS");
    pub const C6:   Self = Self("C6_LOCK_HELD_SIGNAL_EMIT");
    pub const C7:   Self = Self("C7_LOCK_GUARD_REFERENCE_RETURN");
    pub const C12:  Self = Self("C12_UNBOUNDED_COLLECTIONS");
    pub const C13:  Self = Self("C13_MISSING_BUILDER");
    pub const FP_FALSE_PASS_CLASSES: Self = Self("FP_FALSE_PASS_CLASSES");

    /// All known pattern IDs. Used by CompositeScanner to verify full coverage.
    pub const ALL: [Self; 8] = [
        Self::AP28, Self::AP29, Self::AP31,
        Self::C6,   Self::C7,   Self::C12,
        Self::C13,  Self::FP_FALSE_PASS_CLASSES,
    ];

    #[must_use]
    pub const fn as_str(&self) -> &'static str { self.0 }

    #[must_use]
    pub const fn predicate_id(&self) -> &'static str { "HLE-SP-001" }
}
```

**Traits:** `Display` (`"AP28_COMPOSITIONAL_INTEGRITY_DRIFT"`), `AsRef<str>`

---

## SourceLocation

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(file_path: impl Into<String>, line_start: u32, line_end: u32) -> Result<Self>` | Errors if `line_end < line_start` |
| `single_line` | `fn(file_path: impl Into<String>, line: u32) -> Result<Self>` | Sets both start and end |
| `contains` | `fn(&self, line: u32) -> bool` | Inclusive range check |

**Traits:** `Display` (`"src/foo.rs:12-18"`)

---

## BoundedString

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedString(String);

pub const EVIDENCE_CAP_BYTES: usize = 1024;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(s: impl Into<String>) -> Result<Self>` | Errors (2300) if bytes > `EVIDENCE_CAP_BYTES` |
| `truncating` | `fn(s: impl Into<String>) -> Self` | Silently truncates at cap — use for best-effort summaries |
| `as_str` | `fn(&self) -> &str` | `#[must_use]` |
| `len` | `fn(&self) -> usize` | byte count |
| `is_empty` | `fn(&self) -> bool` | |

**Traits:** `Display`, `AsRef<str>`

---

## DetectorEvent

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorEvent {
    pub pattern_id:  AntiPatternId,
    pub severity:    Severity,
    pub location:    SourceLocation,
    pub evidence:    BoundedString,
    pub receipt_sha: Option<[u8; 32]>,   // SHA-256 of the verifier receipt that triggered the scan
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(pattern_id, severity, location, evidence) -> Result<Self>` | Validates all fields |
| `with_receipt_sha` | `fn(self, sha: [u8; 32]) -> Self` | Builder chain — anchors finding to receipt |
| `is_anchored` | `fn(&self) -> bool` | `receipt_sha.is_some()` |
| `predicate_id` | `fn(&self) -> &'static str` | Always `"HLE-SP-001"` |

**Traits:** `Display` (`"[AP28][HIGH] src/foo.rs:12-18 — <evidence>"`), `From<DetectorEvent> for serde_json::Value` (for JSONL persistence in M021)

---

## ScanInput

```rust
#[derive(Debug, Clone)]
pub struct ScanInput {
    pub file_path: String,
    pub content:   String,
    pub receipt_sha: Option<[u8; 32]>,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(file_path: impl Into<String>, content: impl Into<String>) -> Result<Self>` | Errors (2300) if content is empty |
| `with_receipt_sha` | `fn(self, sha: [u8; 32]) -> Self` | Propagates receipt anchor to emitted events |

---

## Scanner Trait

```rust
pub trait Scanner: Send + Sync {
    /// The anti-pattern this scanner detects.
    fn pattern_id(&self) -> AntiPatternId;

    /// Run the scanner against a single input unit.
    /// Returns zero or more findings; never panics.
    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent>;

    /// Human-readable description of what this scanner looks for.
    fn description(&self) -> &'static str;
}
```

All implementations must satisfy: if `input` is a known-good negative control for `pattern_id`,
`scan` must return an empty `Vec`.

---

## Concrete Scanner Method Table

Each concrete scanner is a unit struct implementing `Scanner`. All follow the same structure:

| Scanner | pattern_id | Core detection heuristic |
|---|---|---|
| `AP28Scanner` | `AP28` | Detects mismatched surface counts across `plan.toml`, `ULTRAMAP.md`, and source file census |
| `AP29Scanner` | `AP29` | Detects `std::thread::sleep`, `std::fs::*` blocking calls, and sync mutex locks inside `async fn` bodies |
| `AP31Scanner` | `AP31` | Detects nested `lock()` / `read()` / `write()` call chains within the same lexical scope |
| `C6Scanner` | `C6` | Detects signal/event `emit()` calls appearing inside an open lock guard scope |
| `C7Scanner` | `C7` | Detects function return types that borrow from a local lock guard |
| `C12Scanner` | `C12` | Detects `Vec::new()`, `HashMap::new()`, `VecDeque::new()` without an explicit `with_capacity` or bounded wrapper |
| `C13Scanner` | `C13` | Detects struct construction with five or more fields bypassing a builder or validation boundary |
| `FalsePassClassScanner` | `FP_FALSE_PASS_CLASSES` | Detects gate JSON or receipt files containing `"verdict":"PASS"` without all four required anchor fields |

Each concrete scanner example:

```rust
pub struct AP29Scanner;

impl Scanner for AP29Scanner {
    fn pattern_id(&self) -> AntiPatternId { AntiPatternId::AP29 }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Heuristic: search for blocking syscall patterns inside async fn blocks.
        // Returns Vec::new() for negative controls (sync-only code, spawn_blocking wrappers).
        todo_replaced_by_m0_implementation()
    }

    fn description(&self) -> &'static str {
        "Detects blocking work inside async execution contexts (AP29_BLOCKING_IN_ASYNC)"
    }
}
```

---

## CompositeScanner

```rust
pub struct CompositeScanner {
    scanners: Vec<Box<dyn Scanner>>,
}
```

| Method | Signature | Notes |
|---|---|---|
| `full` | `fn() -> Result<Self>` | Instantiates all 8 concrete scanners; errors if `ALL` is not fully covered |
| `with_scanners` | `fn(scanners: Vec<Box<dyn Scanner>>) -> Result<Self>` | Custom subset; errors if empty |
| `scan_all` | `fn(&self, inputs: &[ScanInput]) -> ScanReport` | Runs each scanner over each input; deduplicates by `(pattern_id, location)` |
| `covered_patterns` | `fn(&self) -> Vec<AntiPatternId>` | Set of patterns covered by registered scanners |

---

## ScanReport

```rust
#[derive(Debug, Clone)]
pub struct ScanReport {
    pub events:       Vec<DetectorEvent>,
    pub inputs_count: usize,
    pub scanners_run: usize,
}
```

| Method | Signature | Notes |
|---|---|---|
| `by_severity` | `fn(&self) -> Vec<&DetectorEvent>` | Sorted Critical → Low |
| `by_pattern` | `fn(&self, id: AntiPatternId) -> Vec<&DetectorEvent>` | Filter by pattern |
| `highest_severity` | `fn(&self) -> Option<Severity>` | None if no events |
| `is_clean` | `fn(&self) -> bool` | No events at High or Critical |

---

## Design Notes

- `AntiPatternId::ALL` is the authoritative coverage list. `CompositeScanner::full()` asserts that
  every element of `ALL` is represented; a missing entry is a compile-time-detectable gap at M0.
- `BoundedString` caps at `EVIDENCE_CAP_BYTES` to prevent unbounded evidence accumulation — a
  C12 finding in its own module would be ironic.
- `ScanInput::receipt_sha` propagates into every `DetectorEvent` emitted from that input, enabling
  M021 to correlate findings back to the verifier receipt that triggered the scan (C01 dependency).
- Negative control fixtures live in `tests/negative_controls/` and are run as part of the
  standard test suite; any scanner that fires against them fails the gate.
- No scanner implementation may call `unwrap`, `expect`, `panic`, `todo`, `dbg`, or use `unsafe`.
  Heuristics that fail gracefully return an empty `Vec`, not an error, to keep `scan_all` total.

---

*M020 Anti-Pattern Scanner Spec v1.0 | 2026-05-10*
