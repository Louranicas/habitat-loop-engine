#![forbid(unsafe_code)]

//! M020 Anti-Pattern Scanner — `AntiPatternId` catalog, `Scanner` trait, and stub
//! concrete scanners for the eight catalogued anti-patterns.
//!
//! Design decisions:
//! - Every concrete scanner is a unit struct; `Scanner::scan` is a pure transform over
//!   `ScanInput`. Stateless — safe to share as `Arc<dyn Scanner>`.
//! - Heuristics that cannot succeed gracefully return `Vec::new()`, never an error,
//!   so `CompositeScanner::scan_all` is total.
//! - Negative controls: a known-good input must produce zero events for every scanner.
//!
//! Layer: L04 | Cluster: C04

use std::fmt;

use substrate_types::HleError;

// ---------------------------------------------------------------------------
// AntiPatternId
// ---------------------------------------------------------------------------

/// Newtype wrapping a `&'static str` catalog key for each known anti-pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AntiPatternId(&'static str);

impl AntiPatternId {
    /// AP28 — Compositional Integrity Drift.
    pub const AP28: Self = Self("AP28_COMPOSITIONAL_INTEGRITY_DRIFT");
    /// AP29 — Blocking work inside async execution contexts.
    pub const AP29: Self = Self("AP29_BLOCKING_IN_ASYNC");
    /// AP31 — Nested lock acquisition within the same lexical scope.
    pub const AP31: Self = Self("AP31_NESTED_LOCKS");
    /// C6 — Signal or event emission while holding a lock guard.
    pub const C6: Self = Self("C6_LOCK_HELD_SIGNAL_EMIT");
    /// C7 — Returning a reference that borrows from a local lock guard.
    pub const C7: Self = Self("C7_LOCK_GUARD_REFERENCE_RETURN");
    /// C12 — Unbounded collection growth without capacity bounding.
    pub const C12: Self = Self("C12_UNBOUNDED_COLLECTIONS");
    /// C13 — Struct construction bypassing a builder or validation boundary.
    pub const C13: Self = Self("C13_MISSING_BUILDER");
    /// `FP_FALSE_PASS_CLASSES` — HLE-SP-001 false-PASS surface detector.
    pub const FP_FALSE_PASS_CLASSES: Self = Self("FP_FALSE_PASS_CLASSES");

    /// Authoritative set of all catalogued anti-pattern IDs.
    /// `CompositeScanner::full()` checks this for full coverage.
    pub const ALL: [Self; 8] = [
        Self::AP28,
        Self::AP29,
        Self::AP31,
        Self::C6,
        Self::C7,
        Self::C12,
        Self::C13,
        Self::FP_FALSE_PASS_CLASSES,
    ];

    /// The raw catalog key string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The HLE-SP-001 predicate identifier (same for all patterns in C04).
    #[must_use]
    pub const fn predicate_id(self) -> &'static str {
        "HLE-SP-001"
    }
}

impl fmt::Display for AntiPatternId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl AsRef<str> for AntiPatternId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

/// Finding severity, aligned to the C04 error strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Informational — does not block any gate.
    Low,
    /// Advisory — triggers a warning annotation.
    Medium,
    /// Blocks workflow promotion when count > 0.
    High,
    /// Blocks workflow promotion immediately.
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => f.write_str("LOW"),
            Self::Medium => f.write_str("MEDIUM"),
            Self::High => f.write_str("HIGH"),
            Self::Critical => f.write_str("CRITICAL"),
        }
    }
}

// ---------------------------------------------------------------------------
// SourceLocation
// ---------------------------------------------------------------------------

/// File path and inclusive line range of a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Relative or absolute file path.
    pub file_path: String,
    /// First line of the finding (1-based).
    pub line_start: u32,
    /// Last line of the finding (inclusive, 1-based).
    pub line_end: u32,
}

impl SourceLocation {
    /// Construct a location spanning `line_start..=line_end`.
    ///
    /// # Errors
    ///
    /// Returns `[E2300]` when `line_end < line_start`.
    pub fn new(
        file_path: impl Into<String>,
        line_start: u32,
        line_end: u32,
    ) -> Result<Self, HleError> {
        if line_end < line_start {
            return Err(HleError::new(
                "[E2300] SourceLocation: line_end must be >= line_start",
            ));
        }
        Ok(Self {
            file_path: file_path.into(),
            line_start,
            line_end,
        })
    }

    /// Construct a single-line location.
    ///
    /// # Errors
    ///
    /// Propagates any `HleError` from `new`.
    pub fn single_line(file_path: impl Into<String>, line: u32) -> Result<Self, HleError> {
        Self::new(file_path, line, line)
    }

    /// True when `line` falls within the inclusive range `[line_start, line_end]`.
    #[must_use]
    pub fn contains(&self, line: u32) -> bool {
        line >= self.line_start && line <= self.line_end
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}-{}",
            self.file_path, self.line_start, self.line_end
        )
    }
}

// ---------------------------------------------------------------------------
// BoundedString
// ---------------------------------------------------------------------------

/// Maximum evidence string size in bytes.
pub const EVIDENCE_CAP_BYTES: usize = 1024;

/// A `String` capped at `EVIDENCE_CAP_BYTES` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedString(String);

impl BoundedString {
    /// Construct from `s`, returning an error when `s.len() > EVIDENCE_CAP_BYTES`.
    ///
    /// # Errors
    ///
    /// Returns `[E2300]` when the byte length exceeds the cap.
    pub fn new(s: impl Into<String>) -> Result<Self, HleError> {
        let s = s.into();
        if s.len() > EVIDENCE_CAP_BYTES {
            return Err(HleError::new(format!(
                "[E2300] evidence string exceeds {EVIDENCE_CAP_BYTES}-byte cap"
            )));
        }
        Ok(Self(s))
    }

    /// Silently truncate `s` at the nearest UTF-8 boundary at or below `EVIDENCE_CAP_BYTES`.
    #[must_use]
    pub fn truncating(s: impl Into<String>) -> Self {
        let s = s.into();
        if s.len() <= EVIDENCE_CAP_BYTES {
            return Self(s);
        }
        // Walk back to a valid UTF-8 boundary.
        let mut end = EVIDENCE_CAP_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        Self(s[..end].to_owned())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when the string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for BoundedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BoundedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// DetectorEvent
// ---------------------------------------------------------------------------

/// A single scanner finding with location, severity, and evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorEvent {
    /// The anti-pattern this event was emitted for.
    pub pattern_id: AntiPatternId,
    /// Severity level of this finding.
    pub severity: Severity,
    /// File and line range.
    pub location: SourceLocation,
    /// Human-readable evidence (capacity-capped).
    pub evidence: BoundedString,
    /// Optional receipt SHA anchoring this finding to a C01 verifier receipt.
    pub receipt_sha: Option<[u8; 32]>,
}

impl DetectorEvent {
    /// Construct a `DetectorEvent`, validating all fields.
    ///
    /// # Errors
    ///
    /// Propagates any validation errors from `SourceLocation` or `BoundedString`.
    pub fn new(
        pattern_id: AntiPatternId,
        severity: Severity,
        location: SourceLocation,
        evidence: BoundedString,
    ) -> Result<Self, HleError> {
        Ok(Self {
            pattern_id,
            severity,
            location,
            evidence,
            receipt_sha: None,
        })
    }

    /// Builder chain to attach a C01 receipt SHA anchor.
    #[must_use]
    pub fn with_receipt_sha(mut self, sha: [u8; 32]) -> Self {
        self.receipt_sha = Some(sha);
        self
    }

    /// True when a receipt SHA anchor is present.
    #[must_use]
    pub fn is_anchored(&self) -> bool {
        self.receipt_sha.is_some()
    }

    /// Always returns `"HLE-SP-001"`.
    #[must_use]
    pub fn predicate_id(&self) -> &'static str {
        "HLE-SP-001"
    }
}

impl fmt::Display for DetectorEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}][{}] {} — {}",
            self.pattern_id, self.severity, self.location, self.evidence
        )
    }
}

// ---------------------------------------------------------------------------
// ScanInput
// ---------------------------------------------------------------------------

/// Target text or AST blob fed to a `Scanner`.
#[derive(Debug, Clone)]
pub struct ScanInput {
    /// File path being scanned.
    pub file_path: String,
    /// Source content being scanned.
    pub content: String,
    /// Optional receipt SHA propagated to every `DetectorEvent` emitted.
    pub receipt_sha: Option<[u8; 32]>,
}

impl ScanInput {
    /// Construct a `ScanInput`, returning an error when `content` is empty.
    ///
    /// # Errors
    ///
    /// Returns `[E2300]` when `content` is empty.
    pub fn new(file_path: impl Into<String>, content: impl Into<String>) -> Result<Self, HleError> {
        let content = content.into();
        if content.is_empty() {
            return Err(HleError::new("[E2300] ScanInput content cannot be empty"));
        }
        Ok(Self {
            file_path: file_path.into(),
            content,
            receipt_sha: None,
        })
    }

    /// Builder chain to attach a receipt SHA that propagates to emitted events.
    #[must_use]
    pub fn with_receipt_sha(mut self, sha: [u8; 32]) -> Self {
        self.receipt_sha = Some(sha);
        self
    }
}

// ---------------------------------------------------------------------------
// Scanner trait
// ---------------------------------------------------------------------------

/// Core scanning contract. All implementations are stateless after construction.
pub trait Scanner: Send + Sync {
    /// The anti-pattern this scanner detects.
    fn pattern_id(&self) -> AntiPatternId;

    /// Run the scanner against a single input unit.
    ///
    /// Returns zero or more findings. Never panics. Returns an empty `Vec` on
    /// any internal failure to ensure `scan_all` remains total.
    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent>;

    /// Human-readable description of what this scanner looks for.
    fn description(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// Helper — build a DetectorEvent from a pattern + scan input context
// ---------------------------------------------------------------------------

/// Convenience: construct a `DetectorEvent` for a line-level finding.
/// Silently returns `None` on construction failure so scanners stay infallible.
fn make_event(
    pattern_id: AntiPatternId,
    severity: Severity,
    file_path: &str,
    line: u32,
    evidence: &str,
    receipt_sha: Option<[u8; 32]>,
) -> Option<DetectorEvent> {
    let location = SourceLocation::single_line(file_path, line).ok()?;
    let evidence = BoundedString::truncating(evidence);
    let mut ev = DetectorEvent::new(pattern_id, severity, location, evidence).ok()?;
    if let Some(sha) = receipt_sha {
        ev = ev.with_receipt_sha(sha);
    }
    Some(ev)
}

// ---------------------------------------------------------------------------
// Concrete scanner unit structs
// ---------------------------------------------------------------------------

/// AP28 — detects mismatched surface counts across plan, map, and source census.
pub struct Ap28Scanner;

impl Scanner for Ap28Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::AP28
    }

    fn description(&self) -> &'static str {
        "Detects compositional integrity drift: surface-count mismatches across \
         plan.toml, ULTRAMAP.md, and source file census (AP28_COMPOSITIONAL_INTEGRITY_DRIFT)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Stub heuristic: flag files that declare "modules:" counts but also
        // contain a mismatch marker typically introduced by a bad merge.
        let mut events = Vec::new();
        let lower = input.content.to_ascii_lowercase();
        if lower.contains("modules:") && lower.contains("<<<<<<") {
            if let Some(ev) = make_event(
                AntiPatternId::AP28,
                Severity::High,
                &input.file_path,
                1,
                "merge conflict marker found in file declaring module counts",
                input.receipt_sha,
            ) {
                events.push(ev);
            }
        }
        events
    }
}

/// AP29 — detects blocking syscalls inside async fn bodies.
pub struct Ap29Scanner;

impl Scanner for Ap29Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::AP29
    }

    fn description(&self) -> &'static str {
        "Detects blocking work inside async execution contexts: \
         std::thread::sleep, std::fs calls, sync Mutex::lock (AP29_BLOCKING_IN_ASYNC)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        let blocking_patterns = [
            "std::thread::sleep",
            "thread::sleep",
            "std::fs::read",
            "std::fs::write",
            "std::fs::read_to_string",
        ];

        let mut events = Vec::new();
        let mut inside_async = false;

        for (idx, line) in input.content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let trimmed = line.trim();

            if trimmed.starts_with("async fn") || trimmed.starts_with("pub async fn") {
                inside_async = true;
            }
            // Very naive: reset on next top-level fn (good enough for a stub heuristic).
            if !trimmed.starts_with("async") && trimmed.starts_with("fn ") {
                inside_async = false;
            }

            if inside_async {
                for pattern in &blocking_patterns {
                    if trimmed.contains(pattern) {
                        let evidence = format!("blocking call '{pattern}' inside async fn");
                        if let Some(ev) = make_event(
                            AntiPatternId::AP29,
                            Severity::High,
                            &input.file_path,
                            line_no,
                            &evidence,
                            input.receipt_sha,
                        ) {
                            events.push(ev);
                        }
                        break;
                    }
                }
            }
        }
        events
    }
}

/// AP31 — detects nested lock acquisition within a single lexical scope.
pub struct Ap31Scanner;

impl Scanner for Ap31Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::AP31
    }

    fn description(&self) -> &'static str {
        "Detects nested lock() / read() / write() call chains in the same scope (AP31_NESTED_LOCKS)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Stub: flag any line that contains two or more `.lock()` calls.
        let mut events = Vec::new();
        for (idx, line) in input.content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let count = line.matches(".lock()").count();
            if count >= 2 {
                let evidence = format!("nested .lock() calls ({count}) on a single line");
                if let Some(ev) = make_event(
                    AntiPatternId::AP31,
                    Severity::High,
                    &input.file_path,
                    line_no,
                    &evidence,
                    input.receipt_sha,
                ) {
                    events.push(ev);
                }
            }
        }
        events
    }
}

/// C6 — detects signal/event `emit()` calls inside an open lock guard scope.
pub struct C6Scanner;

impl Scanner for C6Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::C6
    }

    fn description(&self) -> &'static str {
        "Detects emit() calls appearing inside an open lock guard scope (C6_LOCK_HELD_SIGNAL_EMIT)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Stub: flag any line that has both `.lock()` and `.emit(` on it.
        let mut events = Vec::new();
        for (idx, line) in input.content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let trimmed = line.trim();
            if trimmed.contains(".lock()") && trimmed.contains(".emit(") {
                if let Some(ev) = make_event(
                    AntiPatternId::C6,
                    Severity::High,
                    &input.file_path,
                    line_no,
                    "emit() called while holding a lock guard on the same line",
                    input.receipt_sha,
                ) {
                    events.push(ev);
                }
            }
        }
        events
    }
}

/// C7 — detects function return types that borrow from a local lock guard.
pub struct C7Scanner;

impl Scanner for C7Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::C7
    }

    fn description(&self) -> &'static str {
        "Detects function return types borrowing from a local lock guard \
         (C7_LOCK_GUARD_REFERENCE_RETURN)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Stub: flag functions returning `MutexGuard` or `RwLockReadGuard`.
        let guard_types = ["MutexGuard", "RwLockReadGuard", "RwLockWriteGuard"];
        let mut events = Vec::new();
        for (idx, line) in input.content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let trimmed = line.trim();
            if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "))
                && trimmed.contains("->")
            {
                for guard in &guard_types {
                    if trimmed.contains(guard) {
                        let evidence =
                            format!("function returns {guard} — borrows from local lock guard");
                        if let Some(ev) = make_event(
                            AntiPatternId::C7,
                            Severity::High,
                            &input.file_path,
                            line_no,
                            &evidence,
                            input.receipt_sha,
                        ) {
                            events.push(ev);
                        }
                        break;
                    }
                }
            }
        }
        events
    }
}

/// C12 — detects unbounded collection construction without capacity.
pub struct C12Scanner;

impl Scanner for C12Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::C12
    }

    fn description(&self) -> &'static str {
        "Detects Vec::new() / HashMap::new() / VecDeque::new() without explicit capacity \
         bounding (C12_UNBOUNDED_COLLECTIONS)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        let unbounded = ["Vec::new()", "HashMap::new()", "VecDeque::new()"];
        let mut events = Vec::new();
        for (idx, line) in input.content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let trimmed = line.trim();
            for pattern in &unbounded {
                if trimmed.contains(pattern) {
                    let evidence =
                        format!("unbounded collection '{pattern}' — consider with_capacity");
                    if let Some(ev) = make_event(
                        AntiPatternId::C12,
                        Severity::Medium,
                        &input.file_path,
                        line_no,
                        &evidence,
                        input.receipt_sha,
                    ) {
                        events.push(ev);
                    }
                    break;
                }
            }
        }
        events
    }
}

/// C13 — detects struct construction with five or more fields bypassing a builder.
pub struct C13Scanner;

impl Scanner for C13Scanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::C13
    }

    fn description(&self) -> &'static str {
        "Detects struct construction with five or more fields bypassing a builder or \
         validation boundary (C13_MISSING_BUILDER)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        // Stub: detect `StructName {` lines followed by >= 5 `field: value,` lines before `}`.
        let mut events = Vec::new();
        let lines: Vec<&str> = input.content.lines().collect();
        let mut i = 0usize;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            // Detect struct-literal opening: `SomeName {` on its own line.
            if trimmed.ends_with('{')
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("if")
                && !trimmed.starts_with("match")
                && !trimmed.starts_with("while")
                && !trimmed.starts_with("for")
                && !trimmed.starts_with("loop")
                && !trimmed.starts_with("impl")
                && !trimmed.starts_with("pub ")
                && !trimmed.starts_with("fn ")
                && !trimmed.starts_with("mod ")
            {
                // Count `name: value,` fields inside this block.
                let mut field_count = 0usize;
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j].trim();
                    if inner == "}" || inner == "};" || inner == "}," {
                        break;
                    }
                    if inner.contains(':') && !inner.starts_with("//") {
                        field_count += 1;
                    }
                    j += 1;
                }
                if field_count >= 5 {
                    let evidence = format!(
                        "struct literal with {field_count} fields — consider a builder pattern"
                    );
                    if let Some(ev) = make_event(
                        AntiPatternId::C13,
                        Severity::Medium,
                        &input.file_path,
                        (i + 1) as u32,
                        &evidence,
                        input.receipt_sha,
                    ) {
                        events.push(ev);
                    }
                }
            }
            i += 1;
        }
        events
    }
}

/// `FP_FALSE_PASS_CLASSES` — HLE-SP-001 surface scanner.
/// Detects gate JSON or receipt content containing `"PASS"` without all four required anchor fields.
pub struct FalsePassClassScanner;

impl Scanner for FalsePassClassScanner {
    fn pattern_id(&self) -> AntiPatternId {
        AntiPatternId::FP_FALSE_PASS_CLASSES
    }

    fn description(&self) -> &'static str {
        "Detects gate JSON or receipt files containing \"PASS\" verdict without \
         all four required anchor fields: ^Verdict, ^Manifest_sha256, ^Framework_sha256, \
         ^Counter_evidence_locator (FP_FALSE_PASS_CLASSES / HLE-SP-001)"
    }

    fn scan(&self, input: &ScanInput) -> Vec<DetectorEvent> {
        let content = &input.content;

        // Only inspect content that contains a PASS verdict claim.
        // We require the literal string "PASS" (case-insensitive) to appear as a
        // JSON string value — not just any occurrence of the letters.
        let lower = content.to_ascii_lowercase();
        let has_pass_claim = lower.contains("\"pass\"");
        if !has_pass_claim {
            return Vec::new();
        }

        let required_anchors = [
            "^Verdict",
            "^Manifest_sha256",
            "^Framework_sha256",
            "^Counter_evidence_locator",
        ];

        let mut missing: Vec<&str> = required_anchors
            .iter()
            .filter(|&&anchor| !content.contains(anchor))
            .copied()
            .collect();

        if missing.is_empty() {
            return Vec::new();
        }

        missing.sort_unstable();
        let evidence = format!(
            "PASS claim missing required anchor(s): {}",
            missing.join(", ")
        );

        make_event(
            AntiPatternId::FP_FALSE_PASS_CLASSES,
            Severity::High,
            &input.file_path,
            1,
            &evidence,
            input.receipt_sha,
        )
        .into_iter()
        .collect()
    }
}

// ---------------------------------------------------------------------------
// ScanReport
// ---------------------------------------------------------------------------

/// Aggregated output of a `CompositeScanner` run.
#[derive(Debug, Clone)]
pub struct ScanReport {
    /// All deduplicated findings from all registered scanners.
    pub events: Vec<DetectorEvent>,
    /// Number of `ScanInput` units processed.
    pub inputs_count: usize,
    /// Number of scanner instances run per input.
    pub scanners_run: usize,
}

impl ScanReport {
    /// Return all events sorted Critical → Low.
    #[must_use]
    pub fn by_severity(&self) -> Vec<&DetectorEvent> {
        let mut sorted: Vec<&DetectorEvent> = self.events.iter().collect();
        sorted.sort_by(|a, b| b.severity.cmp(&a.severity));
        sorted
    }

    /// Return events matching a specific pattern ID.
    #[must_use]
    pub fn by_pattern(&self, id: AntiPatternId) -> Vec<&DetectorEvent> {
        self.events.iter().filter(|e| e.pattern_id == id).collect()
    }

    /// Highest severity across all events, or `None` when no events exist.
    #[must_use]
    pub fn highest_severity(&self) -> Option<Severity> {
        self.events.iter().map(|e| e.severity).max()
    }

    /// True when no High or Critical events are present.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        !self.events.iter().any(|e| e.severity >= Severity::High)
    }
}

// ---------------------------------------------------------------------------
// CompositeScanner
// ---------------------------------------------------------------------------

/// Runs all registered scanners over every input, deduplicating by `(pattern_id, location)`.
pub struct CompositeScanner {
    scanners: Vec<Box<dyn Scanner>>,
}

impl CompositeScanner {
    /// Instantiate with all eight concrete scanners and verify full coverage.
    ///
    /// # Errors
    ///
    /// Returns `[E2301]` when any `AntiPatternId::ALL` entry is not covered.
    pub fn full() -> Result<Self, HleError> {
        let scanners: Vec<Box<dyn Scanner>> = vec![
            Box::new(Ap28Scanner),
            Box::new(Ap29Scanner),
            Box::new(Ap31Scanner),
            Box::new(C6Scanner),
            Box::new(C7Scanner),
            Box::new(C12Scanner),
            Box::new(C13Scanner),
            Box::new(FalsePassClassScanner),
        ];
        Self::with_scanners(scanners)
    }

    /// Construct with a custom set of scanners. Verifies coverage of `AntiPatternId::ALL`.
    ///
    /// # Errors
    ///
    /// Returns `[E2301]` when the scanner list is empty.
    /// Returns `[E2301]` when any entry in `AntiPatternId::ALL` is not covered.
    pub fn with_scanners(scanners: Vec<Box<dyn Scanner>>) -> Result<Self, HleError> {
        if scanners.is_empty() {
            return Err(HleError::new(
                "[E2301] CompositeScanner requires at least one scanner",
            ));
        }
        let covered: Vec<AntiPatternId> = scanners.iter().map(|s| s.pattern_id()).collect();
        for required in AntiPatternId::ALL {
            if !covered.contains(&required) {
                return Err(HleError::new(format!(
                    "[E2301] scanner for {required} not registered"
                )));
            }
        }
        Ok(Self { scanners })
    }

    /// Run all registered scanners over all inputs. Results are deduplicated by
    /// `(pattern_id, file_path, line_start)`.
    #[must_use]
    pub fn scan_all(&self, inputs: &[ScanInput]) -> ScanReport {
        let mut all_events: Vec<DetectorEvent> = Vec::new();
        for input in inputs {
            for scanner in &self.scanners {
                let findings = scanner.scan(input);
                all_events.extend(findings);
            }
        }

        // Deduplicate by (pattern_id, file_path, line_start).
        let mut seen: Vec<(AntiPatternId, String, u32)> = Vec::new();
        let mut deduped: Vec<DetectorEvent> = Vec::new();
        for ev in all_events {
            let key = (
                ev.pattern_id,
                ev.location.file_path.clone(),
                ev.location.line_start,
            );
            if !seen.contains(&key) {
                seen.push(key);
                deduped.push(ev);
            }
        }

        ScanReport {
            events: deduped,
            inputs_count: inputs.len(),
            scanners_run: self.scanners.len(),
        }
    }

    /// Returns the set of pattern IDs covered by registered scanners.
    #[must_use]
    pub fn covered_patterns(&self) -> Vec<AntiPatternId> {
        self.scanners.iter().map(|s| s.pattern_id()).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        AntiPatternId, Ap28Scanner, Ap29Scanner, Ap31Scanner, BoundedString, C12Scanner,
        C13Scanner, C6Scanner, C7Scanner, CompositeScanner, DetectorEvent, FalsePassClassScanner,
        ScanInput, Scanner, Severity, SourceLocation, EVIDENCE_CAP_BYTES,
    };

    fn make_input(file: &str, content: &str) -> ScanInput {
        ScanInput::new(file, content)
            .map_err(|e| e.to_string())
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // AntiPatternId
    // -----------------------------------------------------------------------

    #[test]
    fn anti_pattern_id_all_has_eight_entries() {
        assert_eq!(AntiPatternId::ALL.len(), 8);
    }

    #[test]
    fn anti_pattern_id_as_str_ap28() {
        assert_eq!(
            AntiPatternId::AP28.as_str(),
            "AP28_COMPOSITIONAL_INTEGRITY_DRIFT"
        );
    }

    #[test]
    fn anti_pattern_id_as_str_ap29() {
        assert_eq!(AntiPatternId::AP29.as_str(), "AP29_BLOCKING_IN_ASYNC");
    }

    #[test]
    fn anti_pattern_id_as_str_ap31() {
        assert_eq!(AntiPatternId::AP31.as_str(), "AP31_NESTED_LOCKS");
    }

    #[test]
    fn anti_pattern_id_as_str_c6() {
        assert_eq!(AntiPatternId::C6.as_str(), "C6_LOCK_HELD_SIGNAL_EMIT");
    }

    #[test]
    fn anti_pattern_id_as_str_c7() {
        assert_eq!(AntiPatternId::C7.as_str(), "C7_LOCK_GUARD_REFERENCE_RETURN");
    }

    #[test]
    fn anti_pattern_id_as_str_c12() {
        assert_eq!(AntiPatternId::C12.as_str(), "C12_UNBOUNDED_COLLECTIONS");
    }

    #[test]
    fn anti_pattern_id_as_str_c13() {
        assert_eq!(AntiPatternId::C13.as_str(), "C13_MISSING_BUILDER");
    }

    #[test]
    fn anti_pattern_id_as_str_fp_false_pass() {
        assert_eq!(
            AntiPatternId::FP_FALSE_PASS_CLASSES.as_str(),
            "FP_FALSE_PASS_CLASSES"
        );
    }

    #[test]
    fn anti_pattern_id_all_slice_completeness() {
        // Verify every constant appears in ALL exactly once.
        let constants = [
            AntiPatternId::AP28,
            AntiPatternId::AP29,
            AntiPatternId::AP31,
            AntiPatternId::C6,
            AntiPatternId::C7,
            AntiPatternId::C12,
            AntiPatternId::C13,
            AntiPatternId::FP_FALSE_PASS_CLASSES,
        ];
        for c in &constants {
            assert!(
                AntiPatternId::ALL.contains(c),
                "constant {c} missing from ALL"
            );
        }
    }

    #[test]
    fn anti_pattern_id_predicate_is_hle_sp_001_for_all() {
        for id in AntiPatternId::ALL {
            assert_eq!(id.predicate_id(), "HLE-SP-001");
        }
    }

    #[test]
    fn anti_pattern_id_display_matches_as_str() {
        for id in AntiPatternId::ALL {
            assert_eq!(id.to_string(), id.as_str());
        }
    }

    #[test]
    fn anti_pattern_id_as_ref_matches_as_str() {
        for id in AntiPatternId::ALL {
            assert_eq!(id.as_ref(), id.as_str());
        }
    }

    #[test]
    fn anti_pattern_id_ordering_is_defined() {
        // PartialOrd/Ord must not panic; total order simply must be consistent.
        let mut ids: Vec<AntiPatternId> = AntiPatternId::ALL.to_vec();
        ids.sort();
        assert_eq!(ids.len(), 8);
    }

    // -----------------------------------------------------------------------
    // Severity
    // -----------------------------------------------------------------------

    #[test]
    fn severity_ordering_is_correct() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }

    #[test]
    fn severity_display_stable() {
        assert_eq!(Severity::Low.to_string(), "LOW");
        assert_eq!(Severity::Medium.to_string(), "MEDIUM");
        assert_eq!(Severity::High.to_string(), "HIGH");
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
    }

    // -----------------------------------------------------------------------
    // BoundedString
    // -----------------------------------------------------------------------

    #[test]
    fn bounded_string_rejects_oversized() {
        let long = "a".repeat(EVIDENCE_CAP_BYTES + 1);
        assert!(BoundedString::new(long).is_err());
    }

    #[test]
    fn bounded_string_accepts_at_cap() {
        let at_cap = "a".repeat(EVIDENCE_CAP_BYTES);
        assert!(BoundedString::new(at_cap).is_ok());
    }

    #[test]
    fn bounded_string_accepts_below_cap() {
        assert!(BoundedString::new("hello").is_ok());
    }

    #[test]
    fn bounded_string_truncating_stays_within_cap() {
        let long = "x".repeat(EVIDENCE_CAP_BYTES + 500);
        let s = BoundedString::truncating(long);
        assert!(s.len() <= EVIDENCE_CAP_BYTES);
    }

    #[test]
    fn bounded_string_truncating_passthrough_when_within_cap() {
        let short = "hello";
        let s = BoundedString::truncating(short);
        assert_eq!(s.as_str(), short);
    }

    #[test]
    fn bounded_string_is_empty_reflects_content() {
        let empty = BoundedString::truncating("");
        assert!(empty.is_empty());
        let non_empty = BoundedString::truncating("x");
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn bounded_string_display_matches_content() {
        let s = BoundedString::truncating("test evidence");
        assert_eq!(s.to_string(), "test evidence");
    }

    #[test]
    fn bounded_string_as_ref_matches_as_str() {
        let s = BoundedString::truncating("evidence");
        assert_eq!(s.as_ref(), s.as_str());
    }

    // -----------------------------------------------------------------------
    // SourceLocation
    // -----------------------------------------------------------------------

    #[test]
    fn source_location_rejects_inverted_range() {
        assert!(SourceLocation::new("foo.rs", 10, 5).is_err());
    }

    #[test]
    fn source_location_equal_bounds_accepted() {
        assert!(SourceLocation::new("foo.rs", 5, 5).is_ok());
    }

    #[test]
    fn source_location_single_line() {
        let loc = SourceLocation::single_line("foo.rs", 7).unwrap();
        assert_eq!(loc.line_start, 7);
        assert_eq!(loc.line_end, 7);
        assert!(loc.contains(7));
        assert!(!loc.contains(8));
    }

    #[test]
    fn source_location_contains_range_bounds() {
        let loc = SourceLocation::new("bar.rs", 3, 10).unwrap();
        assert!(loc.contains(3));
        assert!(loc.contains(10));
        assert!(!loc.contains(2));
        assert!(!loc.contains(11));
    }

    #[test]
    fn source_location_display_format() {
        let loc = SourceLocation::new("src/lib.rs", 5, 10).unwrap();
        assert_eq!(loc.to_string(), "src/lib.rs:5-10");
    }

    // -----------------------------------------------------------------------
    // ScanInput
    // -----------------------------------------------------------------------

    #[test]
    fn scan_input_rejects_empty_content() {
        assert!(ScanInput::new("foo.rs", "").is_err());
    }

    #[test]
    fn scan_input_accepts_nonempty_content() {
        assert!(ScanInput::new("foo.rs", "fn main() {}").is_ok());
    }

    #[test]
    fn scan_input_receipt_sha_builder() {
        let input = ScanInput::new("foo.rs", "fn f() {}")
            .unwrap()
            .with_receipt_sha([0xAB; 32]);
        assert_eq!(input.receipt_sha, Some([0xAB; 32]));
    }

    // -----------------------------------------------------------------------
    // DetectorEvent
    // -----------------------------------------------------------------------

    #[test]
    fn detector_event_receipt_sha_anchor_propagates() {
        let location = SourceLocation::single_line("foo.rs", 1).unwrap();
        let evidence = BoundedString::new("test evidence").unwrap();
        let ev = DetectorEvent::new(AntiPatternId::AP29, Severity::High, location, evidence)
            .unwrap()
            .with_receipt_sha([1u8; 32]);
        assert!(ev.is_anchored());
        assert_eq!(ev.predicate_id(), "HLE-SP-001");
    }

    #[test]
    fn detector_event_without_sha_not_anchored() {
        let location = SourceLocation::single_line("foo.rs", 1).unwrap();
        let evidence = BoundedString::new("evidence").unwrap();
        let ev =
            DetectorEvent::new(AntiPatternId::AP28, Severity::Medium, location, evidence).unwrap();
        assert!(!ev.is_anchored());
    }

    #[test]
    fn detector_event_display_contains_pattern_and_severity() {
        let location = SourceLocation::single_line("src/lib.rs", 5).unwrap();
        let evidence = BoundedString::new("blocking call detected").unwrap();
        let ev =
            DetectorEvent::new(AntiPatternId::AP29, Severity::High, location, evidence).unwrap();
        let s = ev.to_string();
        assert!(s.contains("AP29_BLOCKING_IN_ASYNC"));
        assert!(s.contains("HIGH"));
    }

    // -----------------------------------------------------------------------
    // AP28 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn ap28_scanner_pattern_id() {
        assert_eq!(Ap28Scanner.pattern_id(), AntiPatternId::AP28);
    }

    #[test]
    fn ap28_scanner_description_nonempty() {
        assert!(!Ap28Scanner.description().is_empty());
    }

    #[test]
    fn ap28_scanner_fires_on_merge_conflict_with_modules_count() {
        let content = "modules: 42\n<<<<<<< HEAD\nsome diff\n=======\nother\n>>>>>>>";
        let input = make_input("plan.toml", content);
        let events = Ap28Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::AP28);
        assert_eq!(events[0].severity, Severity::High);
    }

    #[test]
    fn ap28_scanner_no_findings_for_clean_toml() {
        let content = "modules: 42\n# no conflicts here";
        let input = make_input("plan.toml", content);
        assert!(Ap28Scanner.scan(&input).is_empty());
    }

    #[test]
    fn ap28_scanner_no_findings_for_conflict_without_modules() {
        let content = "<<<<<<< HEAD\nsome code\n=======\nother code\n>>>>>>>";
        let input = make_input("plan.toml", content);
        // conflict without "modules:" should not fire
        assert!(Ap28Scanner.scan(&input).is_empty());
    }

    // -----------------------------------------------------------------------
    // AP29 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn ap29_scanner_pattern_id() {
        assert_eq!(Ap29Scanner.pattern_id(), AntiPatternId::AP29);
    }

    #[test]
    fn ap29_scanner_description_nonempty() {
        assert!(!Ap29Scanner.description().is_empty());
    }

    #[test]
    fn ap29_scanner_no_findings_for_sync_code() {
        let scanner = Ap29Scanner;
        let input = make_input(
            "src/lib.rs",
            "fn run() { let x = 1 + 2; println!(\"{x}\"); }",
        );
        assert!(scanner.scan(&input).is_empty());
    }

    #[test]
    fn ap29_scanner_finds_thread_sleep_in_async_fn() {
        let scanner = Ap29Scanner;
        let src = "async fn handle() {\n    std::thread::sleep(Duration::from_secs(1));\n}";
        let input = make_input("src/handler.rs", src);
        let events = scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::AP29);
    }

    #[test]
    fn ap29_scanner_finds_std_fs_read_in_async_fn() {
        let src = "async fn load() {\n    let data = std::fs::read(\"file\");\n}";
        let input = make_input("src/loader.rs", src);
        let events = Ap29Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn ap29_scanner_finds_std_fs_write_in_async_fn() {
        let src = "pub async fn save() {\n    std::fs::write(\"path\", b\"data\").ok();\n}";
        let input = make_input("src/writer.rs", src);
        let events = Ap29Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn ap29_scanner_finds_std_fs_read_to_string_in_async() {
        let src = "async fn read_cfg() {\n    let s = std::fs::read_to_string(\"cfg\");\n}";
        let input = make_input("src/cfg.rs", src);
        let events = Ap29Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn ap29_scanner_no_fire_on_blocking_outside_async() {
        let src = "fn sync_fn() {\n    std::thread::sleep(Duration::from_secs(1));\n}";
        let input = make_input("src/sync.rs", src);
        // blocking call is NOT inside an async fn — must not fire
        assert!(Ap29Scanner.scan(&input).is_empty());
    }

    #[test]
    fn ap29_scanner_finding_has_high_severity() {
        let src = "async fn bad() { thread::sleep(Duration::ZERO); }";
        let input = make_input("src/x.rs", src);
        let events = Ap29Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].severity, Severity::High);
    }

    // -----------------------------------------------------------------------
    // AP31 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn ap31_scanner_pattern_id() {
        assert_eq!(Ap31Scanner.pattern_id(), AntiPatternId::AP31);
    }

    #[test]
    fn ap31_scanner_description_nonempty() {
        assert!(!Ap31Scanner.description().is_empty());
    }

    #[test]
    fn ap31_scanner_fires_on_two_lock_calls_same_line() {
        let src = "let a = mu1.lock(); let b = mu2.lock();";
        let input = make_input("src/lib.rs", src);
        let events = Ap31Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::AP31);
    }

    #[test]
    fn ap31_scanner_no_fire_on_single_lock_call() {
        let src = "let guard = mu.lock().unwrap();";
        let input = make_input("src/lib.rs", src);
        assert!(Ap31Scanner.scan(&input).is_empty());
    }

    #[test]
    fn ap31_scanner_severity_is_high() {
        let src = "let a = mu1.lock(); let b = mu2.lock();";
        let input = make_input("src/lib.rs", src);
        let events = Ap31Scanner.scan(&input);
        assert_eq!(events[0].severity, Severity::High);
    }

    // -----------------------------------------------------------------------
    // C6 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn c6_scanner_pattern_id() {
        assert_eq!(C6Scanner.pattern_id(), AntiPatternId::C6);
    }

    #[test]
    fn c6_scanner_description_nonempty() {
        assert!(!C6Scanner.description().is_empty());
    }

    #[test]
    fn c6_scanner_fires_on_lock_and_emit_same_line() {
        let src = "    guard = mu.lock(); channel.emit(event);";
        let input = make_input("src/module.rs", src);
        let events = C6Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::C6);
    }

    #[test]
    fn c6_scanner_no_fire_on_emit_without_lock() {
        let src = "    channel.emit(event);";
        let input = make_input("src/module.rs", src);
        assert!(C6Scanner.scan(&input).is_empty());
    }

    #[test]
    fn c6_scanner_no_fire_on_lock_without_emit() {
        let src = "    let guard = mu.lock().unwrap();";
        let input = make_input("src/module.rs", src);
        assert!(C6Scanner.scan(&input).is_empty());
    }

    #[test]
    fn c6_scanner_severity_is_high() {
        let src = "    x.lock(); y.emit(e);";
        let input = make_input("src/m.rs", src);
        let events = C6Scanner.scan(&input);
        assert_eq!(events[0].severity, Severity::High);
    }

    // -----------------------------------------------------------------------
    // C7 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn c7_scanner_pattern_id() {
        assert_eq!(C7Scanner.pattern_id(), AntiPatternId::C7);
    }

    #[test]
    fn c7_scanner_description_nonempty() {
        assert!(!C7Scanner.description().is_empty());
    }

    #[test]
    fn c7_scanner_fires_on_mutex_guard_return() {
        let src = "pub fn get_guard(&self) -> MutexGuard<Config> { self.mu.lock().unwrap() }";
        let input = make_input("src/config.rs", src);
        let events = C7Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::C7);
    }

    #[test]
    fn c7_scanner_fires_on_rwlock_read_guard_return() {
        let src = "fn view(&self) -> RwLockReadGuard<State> { self.rw.read().unwrap() }";
        let input = make_input("src/state.rs", src);
        let events = C7Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn c7_scanner_fires_on_rwlock_write_guard_return() {
        let src = "fn mut_view(&self) -> RwLockWriteGuard<State> { self.rw.write().unwrap() }";
        let input = make_input("src/state.rs", src);
        let events = C7Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn c7_scanner_no_fire_on_plain_return_type() {
        let src = "pub fn get_name(&self) -> String { self.name.clone() }";
        let input = make_input("src/lib.rs", src);
        assert!(C7Scanner.scan(&input).is_empty());
    }

    #[test]
    fn c7_scanner_severity_is_high() {
        let src = "pub fn g(&self) -> MutexGuard<u32> { self.mu.lock().unwrap() }";
        let input = make_input("src/x.rs", src);
        let events = C7Scanner.scan(&input);
        assert_eq!(events[0].severity, Severity::High);
    }

    // -----------------------------------------------------------------------
    // C12 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn c12_scanner_pattern_id() {
        assert_eq!(C12Scanner.pattern_id(), AntiPatternId::C12);
    }

    #[test]
    fn c12_scanner_description_nonempty() {
        assert!(!C12Scanner.description().is_empty());
    }

    #[test]
    fn c12_scanner_fires_on_vec_new() {
        let src = "let events: Vec<u32> = Vec::new();";
        let input = make_input("src/collector.rs", src);
        let events = C12Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::C12);
    }

    #[test]
    fn c12_scanner_fires_on_hashmap_new() {
        let src = "let map = HashMap::new();";
        let input = make_input("src/cache.rs", src);
        let events = C12Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn c12_scanner_fires_on_vecdeque_new() {
        let src = "let q = VecDeque::new();";
        let input = make_input("src/queue.rs", src);
        let events = C12Scanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn c12_scanner_no_fire_on_with_capacity() {
        let src = "let v: Vec<u8> = Vec::with_capacity(64);";
        let input = make_input("src/buf.rs", src);
        assert!(C12Scanner.scan(&input).is_empty());
    }

    #[test]
    fn c12_scanner_severity_is_medium() {
        let src = "let v = Vec::new();";
        let input = make_input("src/x.rs", src);
        let events = C12Scanner.scan(&input);
        assert_eq!(events[0].severity, Severity::Medium);
    }

    // -----------------------------------------------------------------------
    // C13 scanner
    // -----------------------------------------------------------------------

    #[test]
    fn c13_scanner_pattern_id() {
        assert_eq!(C13Scanner.pattern_id(), AntiPatternId::C13);
    }

    #[test]
    fn c13_scanner_description_nonempty() {
        assert!(!C13Scanner.description().is_empty());
    }

    #[test]
    fn c13_scanner_fires_on_large_struct_literal() {
        let src = "MyConfig {\n  a: 1,\n  b: 2,\n  c: 3,\n  d: 4,\n  e: 5,\n}";
        let input = make_input("src/setup.rs", src);
        let events = C13Scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::C13);
    }

    #[test]
    fn c13_scanner_no_fire_on_small_struct_literal() {
        let src = "Point {\n  x: 1,\n  y: 2,\n}";
        let input = make_input("src/geo.rs", src);
        assert!(C13Scanner.scan(&input).is_empty());
    }

    #[test]
    fn c13_scanner_severity_is_medium() {
        let src = "BigThing {\n  a: 1,\n  b: 2,\n  c: 3,\n  d: 4,\n  e: 5,\n}";
        let input = make_input("src/x.rs", src);
        let events = C13Scanner.scan(&input);
        assert_eq!(events[0].severity, Severity::Medium);
    }

    // -----------------------------------------------------------------------
    // FalsePassClassScanner
    // -----------------------------------------------------------------------

    #[test]
    fn false_pass_scanner_pattern_id() {
        assert_eq!(
            FalsePassClassScanner.pattern_id(),
            AntiPatternId::FP_FALSE_PASS_CLASSES
        );
    }

    #[test]
    fn false_pass_scanner_description_nonempty() {
        assert!(!FalsePassClassScanner.description().is_empty());
    }

    #[test]
    fn false_pass_scanner_no_findings_for_fully_anchored_pass() {
        let scanner = FalsePassClassScanner;
        let json = r#"{
          "verdict": "PASS",
          "^Verdict": "PASS",
          "^Manifest_sha256": "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a",
          "^Framework_sha256": "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d",
          "^Counter_evidence_locator": "tests/negative_controls/taxonomy_negatives.rs"
        }"#;
        let input = make_input("gate.json", json);
        assert!(
            scanner.scan(&input).is_empty(),
            "negative control must not fire"
        );
    }

    #[test]
    fn false_pass_scanner_fires_when_all_anchors_missing() {
        let scanner = FalsePassClassScanner;
        let json = r#"{ "verdict": "PASS" }"#;
        let input = make_input("gate.json", json);
        let events = scanner.scan(&input);
        assert!(!events.is_empty());
        assert_eq!(events[0].pattern_id, AntiPatternId::FP_FALSE_PASS_CLASSES);
    }

    #[test]
    fn false_pass_scanner_no_findings_for_fail_claim() {
        let scanner = FalsePassClassScanner;
        let json = r#"{ "verdict": "FAIL" }"#;
        let input = make_input("gate.json", json);
        assert!(
            scanner.scan(&input).is_empty(),
            "FAIL claim is not a PASS — scanner must not fire"
        );
    }

    #[test]
    fn false_pass_scanner_no_findings_for_awaiting_human() {
        let json = r#"{ "verdict": "AWAITING_HUMAN" }"#;
        let input = make_input("gate.json", json);
        assert!(FalsePassClassScanner.scan(&input).is_empty());
    }

    #[test]
    fn false_pass_scanner_fires_when_one_anchor_missing() {
        // ^Counter_evidence_locator absent.
        let json = r#"{
          "verdict": "PASS",
          "^Verdict": "PASS",
          "^Manifest_sha256": "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a",
          "^Framework_sha256": "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d"
        }"#;
        let input = make_input("gate.json", json);
        let events = FalsePassClassScanner.scan(&input);
        assert!(!events.is_empty());
    }

    #[test]
    fn false_pass_scanner_evidence_names_missing_anchors() {
        let json = r#"{ "verdict": "PASS" }"#;
        let input = make_input("gate.json", json);
        let events = FalsePassClassScanner.scan(&input);
        assert!(!events.is_empty());
        // Evidence should mention at least one missing anchor name.
        assert!(events[0].evidence.as_str().contains('^'));
    }

    #[test]
    fn false_pass_scanner_finding_has_high_severity() {
        let json = r#"{ "verdict": "PASS" }"#;
        let input = make_input("gate.json", json);
        let events = FalsePassClassScanner.scan(&input);
        assert_eq!(events[0].severity, Severity::High);
    }

    #[test]
    fn false_pass_scanner_no_fire_on_content_without_pass_string() {
        // "pass" appears in a comment but not as a JSON string value.
        let content = r#"# this is not a pass claim\nsome other content"#;
        let input = make_input("notes.txt", content);
        assert!(FalsePassClassScanner.scan(&input).is_empty());
    }

    // -----------------------------------------------------------------------
    // CompositeScanner
    // -----------------------------------------------------------------------

    #[test]
    fn composite_scanner_full_covers_all_patterns() {
        let cs = CompositeScanner::full().map_err(|e| e.to_string()).unwrap();
        let covered = cs.covered_patterns();
        for id in AntiPatternId::ALL {
            assert!(covered.contains(&id), "missing scanner for {id}");
        }
    }

    #[test]
    fn composite_scanner_full_covered_patterns_count_is_eight() {
        let cs = CompositeScanner::full().unwrap();
        assert_eq!(cs.covered_patterns().len(), 8);
    }

    #[test]
    fn composite_scanner_empty_input_produces_no_events() {
        let cs = CompositeScanner::full().unwrap();
        let report = cs.scan_all(&[]);
        assert!(report.events.is_empty());
        assert_eq!(report.inputs_count, 0);
    }

    #[test]
    fn composite_scanner_with_empty_list_errors() {
        assert!(CompositeScanner::with_scanners(vec![]).is_err());
    }

    #[test]
    fn composite_scanner_missing_pattern_errors() {
        // Provide only 7 scanners — one is missing.
        use super::{Ap28Scanner, Ap29Scanner, Ap31Scanner, C12Scanner, C13Scanner, C6Scanner};
        let scanners: Vec<Box<dyn Scanner>> = vec![
            Box::new(Ap28Scanner),
            Box::new(Ap29Scanner),
            Box::new(Ap31Scanner),
            Box::new(C6Scanner),
            Box::new(C12Scanner),
            Box::new(C13Scanner),
            Box::new(FalsePassClassScanner),
            // C7 is missing
        ];
        assert!(CompositeScanner::with_scanners(scanners).is_err());
    }

    #[test]
    fn scan_report_is_clean_when_no_high_or_critical() {
        let cs = CompositeScanner::full().unwrap();
        let input = make_input("src/clean.rs", "fn add(a: i32, b: i32) -> i32 { a + b }");
        let report = cs.scan_all(&[input]);
        assert!(
            report.is_clean(),
            "expected no High/Critical findings in clean source"
        );
    }

    #[test]
    fn scan_report_by_severity_orders_critical_first() {
        let cs = CompositeScanner::full().unwrap();
        // Both C12 (Medium) and AP29 (High) will fire.
        let src =
            "async fn bad() { thread::sleep(std::time::Duration::ZERO); let v = Vec::new(); }";
        let input = make_input("src/bad.rs", src);
        let report = cs.scan_all(&[input]);
        if report.events.len() >= 2 {
            let sorted = report.by_severity();
            let mut prev = sorted[0].severity;
            for ev in sorted.iter().skip(1) {
                assert!(ev.severity <= prev, "severity ordering violated");
                prev = ev.severity;
            }
        }
    }

    #[test]
    fn scan_report_by_pattern_filters_correctly() {
        let cs = CompositeScanner::full().unwrap();
        // C12 fires for Vec::new().
        let src = "fn f() { let v = Vec::new(); let m = HashMap::new(); }";
        let input = make_input("src/x.rs", src);
        let report = cs.scan_all(&[input]);
        let c12_events = report.by_pattern(AntiPatternId::C12);
        assert!(
            c12_events
                .iter()
                .all(|e| e.pattern_id == AntiPatternId::C12),
            "by_pattern must return only matching events"
        );
    }

    #[test]
    fn scan_report_highest_severity_none_when_empty() {
        let cs = CompositeScanner::full().unwrap();
        let report = cs.scan_all(&[]);
        assert_eq!(report.highest_severity(), None);
    }

    #[test]
    fn composite_scanner_scanners_run_field_matches_scanner_count() {
        let cs = CompositeScanner::full().unwrap();
        let input = make_input("src/x.rs", "fn f() {}");
        let report = cs.scan_all(&[input]);
        assert_eq!(report.scanners_run, 8);
    }

    #[test]
    fn composite_scanner_inputs_count_matches_inputs_slice() {
        let cs = CompositeScanner::full().unwrap();
        let inputs = vec![
            make_input("a.rs", "fn a() {}"),
            make_input("b.rs", "fn b() {}"),
            make_input("c.rs", "fn c() {}"),
        ];
        let report = cs.scan_all(&inputs);
        assert_eq!(report.inputs_count, 3);
    }

    #[test]
    fn composite_scanner_deduplicates_same_location_finding() {
        // AP31 fires once per line; repeat the same line across two identical inputs.
        let src = "let a = mu.lock(); let b = mu.lock();";
        let cs = CompositeScanner::full().unwrap();
        let input1 = make_input("src/dup.rs", src);
        let input2 = ScanInput::new("src/dup.rs", src).unwrap();
        let report = cs.scan_all(&[input1, input2]);
        // Events on the same (pattern, file, line) must be deduplicated.
        let ap31 = report.by_pattern(AntiPatternId::AP31);
        let unique_locations: Vec<_> = ap31
            .iter()
            .map(|e| (e.location.file_path.as_str(), e.location.line_start))
            .collect();
        let mut deduped = unique_locations.clone();
        deduped.dedup();
        assert_eq!(
            unique_locations.len(),
            deduped.len(),
            "duplicate not removed"
        );
    }
}
