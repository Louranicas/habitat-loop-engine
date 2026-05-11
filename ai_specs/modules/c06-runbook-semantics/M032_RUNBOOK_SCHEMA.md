# M032 Runbook Schema — `crates/hle-runbook/src/schema.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2500-2510
> **Role:** Typed Rust mirror of the Framework §17.8 TOML runbook definition schema.
> **LOC target:** ~380 | **Test target:** ≥50

---

## Purpose

M032 is the vocabulary layer for all of C06. It defines every type that other runbook modules operate on. The TOML file on disk is a serialization format; the structs here are canonical. No business logic lives in this module — only type definitions, constructors, and pure derived methods.

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|------|------|------|-------|
| `Runbook` | struct | No | Root document type; owns all phases |
| `Phase` | struct | No | One detect/block/fix/verify/meta_test phase |
| `Probe` | struct | No | A single observable check within a phase |
| `EvidenceLocator` | enum | No | Points to file path or inline content |
| `PhaseKind` | enum | Yes | Canonical phase identifiers |
| `ModeApplicability` | struct | No | Which operational modes a runbook applies to |
| `SafetyClass` | enum | Yes | Soft / Hard / Safety tiering |
| `RunbookId` | newtype(`String`) | No | Validated runbook identifier |
| `FixtureId` | newtype(`String`) | No | Validated fixture identifier (used by M038) |

---

## Core Struct: `Runbook`

```rust
/// Root runbook document — typed mirror of Framework §17.8 TOML [runbook] header.
///
/// # Invariants
/// - `id` is non-empty, matches `[a-z0-9_-]+`
/// - `max_traversals >= 1`
/// - `phases` contains at least one entry
/// - `safety_class` governs which operations M039 permits without elevation
#[derive(Debug, Clone, PartialEq)]
pub struct Runbook {
    pub id: RunbookId,
    pub title: String,
    /// Free-form cross-reference to habitat session or incident record.
    pub habitat_history: Option<String>,
    /// Incident fingerprint used for replay fixture matching (M038).
    pub failure_signature: Option<String>,
    pub mode_applicability: ModeApplicability,
    /// Path to canonical schematic (relative to runbook dir).
    pub canonical_schematic: Option<String>,
    /// Self-referential canonical runbook path.
    pub canonical_runbook: Option<String>,
    /// Maximum allowed execution passes before the executor halts.
    pub max_traversals: u32,
    /// When true, re-running is safe; when false, M039 enforces traversal guard.
    pub idempotent: bool,
    pub safety_class: SafetyClass,
    /// Keyed by phase kind; order of execution is determined by PhaseKind::execution_order.
    pub phases: std::collections::HashMap<PhaseKind, Phase>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `builder` | `fn(id: impl Into<String>, title: impl Into<String>) -> RunbookBuilder` | Entry point; returns builder |
| `phase` | `fn(&self, kind: PhaseKind) -> Option<&Phase>` | `#[must_use]` |
| `has_phase` | `fn(&self, kind: PhaseKind) -> bool` | `#[must_use]` |
| `ordered_phases` | `fn(&self) -> Vec<(PhaseKind, &Phase)>` | Sorted by `PhaseKind::execution_order` |
| `is_safe_for_auto_execution` | `fn(&self) -> bool` | `safety_class == Soft && idempotent` |

---

## Struct: `Phase`

```rust
/// One phase of a runbook (detect / block / fix / verify / meta_test).
///
/// A phase is empty-safe: `probes` may be empty for stub phases.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Phase {
    /// Optional trigger condition (evaluated before probes run).
    pub trigger: Option<String>,
    /// Ordered list of observable checks.
    pub probes: Vec<Probe>,
    /// Predicate string that, when true, advances the phase to PASS.
    pub pass_predicate: Option<String>,
    /// Predicate string that, when true, fails the phase immediately.
    pub fail_predicate: Option<String>,
    /// Evidence items required before the phase may be marked complete.
    pub evidence_required: Vec<EvidenceLocator>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `is_empty` | `fn(&self) -> bool` | No probes and no evidence required |
| `requires_evidence` | `fn(&self) -> bool` | `!evidence_required.is_empty()` |
| `probe_count` | `fn(&self) -> usize` | |

---

## Struct: `Probe`

```rust
/// A single observable check within a phase.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub id: String,
    /// Human-readable description of what this probe observes.
    pub description: String,
    /// Command or script to run. None means manual observation.
    pub command: Option<String>,
    /// Expected exit code; None means any non-error exit is acceptable.
    pub expected_exit_code: Option<i32>,
}
```

---

## Enum: `PhaseKind`

```rust
/// Canonical runbook phase identifiers (Framework §17.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PhaseKind {
    /// Detect: identify that an incident is occurring.
    Detect,
    /// Block: prevent the incident from spreading or worsening.
    Block,
    /// Fix: apply the corrective action.
    Fix,
    /// Verify: confirm the fix resolved the incident.
    Verify,
    /// MetaTest: validate the runbook itself (replay fixtures).
    MetaTest,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `as_str` | `const fn(&self) -> &'static str` | "detect" / "block" / "fix" / "verify" / "meta_test" |
| `from_str` | `fn(&str) -> Option<Self>` | Case-insensitive parse |
| `execution_order` | `const fn(&self) -> u8` | Detect=0, Block=1, Fix=2, Verify=3, MetaTest=4 |
| `all` | `fn() -> [Self; 5]` | All variants in execution order |

**Traits:** `Display` ("detect"), `Hash` (used as HashMap key in `Runbook.phases`)

---

## Enum: `SafetyClass`

```rust
/// Safety tier for a runbook — governs M039 enforcement.
///
/// - Soft: may auto-execute if idempotent and confidence is high.
/// - Hard: requires explicit operator confirmation (M035) before Fix phase.
/// - Safety: requires explicit authority elevation + operator confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SafetyClass {
    Soft,
    Hard,
    Safety,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `as_str` | `const fn(&self) -> &'static str` | "soft" / "hard" / "safety" |
| `from_str` | `fn(&str) -> Option<Self>` | Case-insensitive |
| `requires_elevation` | `const fn(&self) -> bool` | `Hard` and `Safety` return true |
| `requires_explicit_confirm` | `const fn(&self) -> bool` | `Hard` and `Safety` return true |

---

## Struct: `ModeApplicability`

```rust
/// Operational mode gates for a runbook.
///
/// A runbook is applicable only when all enabled gates are satisfied.
/// Empty struct = applies in all modes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModeApplicability {
    /// Applicable in scaffold-only mode.
    pub scaffold: bool,
    /// Applicable in local-M0 (one-shot) mode.
    pub local_m0: bool,
    /// Applicable in production (future-authorized) mode.
    pub production: bool,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `all` | `fn() -> Self` | All three flags true |
| `scaffold_only` | `fn() -> Self` | Only scaffold=true |
| `local_m0_only` | `fn() -> Self` | Only local_m0=true |
| `applies_in` | `fn(&self, mode: &OperationalMode) -> bool` | Pattern-matches mode to flag |

---

## Enum: `EvidenceLocator`

```rust
/// Points to evidence required or attached for a phase.
#[derive(Debug, Clone, PartialEq)]
pub enum EvidenceLocator {
    /// Relative path within the runbook directory.
    FilePath(String),
    /// Inline content stored directly in the runbook TOML.
    Inline(String),
    /// Receipt ID from the ledger (persisted by M036 at attach time).
    ReceiptId(String),
}
```

---

## Newtype: `RunbookId`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunbookId(String);

impl RunbookId {
    /// Validates that id matches `[a-z0-9_-]+` and is non-empty.
    pub fn new(raw: impl Into<String>) -> Result<Self, RunbookError>;
    #[must_use]
    pub fn as_str(&self) -> &str;
}
```

---

## Builder: `RunbookBuilder`

```rust
pub struct RunbookBuilder { /* private */ }

impl RunbookBuilder {
    #[must_use]
    pub fn habitat_history(self, h: impl Into<String>) -> Self;
    #[must_use]
    pub fn failure_signature(self, s: impl Into<String>) -> Self;
    #[must_use]
    pub fn mode_applicability(self, m: ModeApplicability) -> Self;
    #[must_use]
    pub fn max_traversals(self, n: u32) -> Self;
    #[must_use]
    pub fn idempotent(self, v: bool) -> Self;
    #[must_use]
    pub fn safety_class(self, c: SafetyClass) -> Self;
    #[must_use]
    pub fn add_phase(self, kind: PhaseKind, phase: Phase) -> Self;
    /// Validates all invariants before returning.
    pub fn build(self) -> Result<Runbook, RunbookError>;
}
```

---

## Design Notes

- All float-returning methods clamp output to `[0.0, 1.0]` (matches L1 convention).
- `PhaseKind` implements `Hash` because `Runbook.phases` is `HashMap<PhaseKind, Phase>`. This is the only collection-keyed enum in C06.
- `SafetyClass` implements `PartialOrd`/`Ord` so M039 can use `safety_class > SafetyClass::Soft` pattern without a match.
- `ModeApplicability::default()` produces all-false (no modes) — callers must be explicit. Use `ModeApplicability::all()` or a specific builder method; never rely on default silently permitting.
- The `habitat_history` field intentionally allows arbitrary strings — it is a cross-reference anchor (session ID, Obsidian note title, POVM pathway key) rather than a typed foreign key.
- `Runbook.phases` is not ordered at the type level; ordering is produced only by `ordered_phases()`. This preserves HashMap round-trip fidelity in TOML serde.

---

## Cluster Invariants (this module)

- `RunbookId::new` rejects empty strings and strings with characters outside `[a-z0-9_-]`.
- `RunbookBuilder::build` rejects `max_traversals == 0` with error code 2510.
- `RunbookBuilder::build` rejects a runbook with zero phases with error code 2510.
- `SafetyClass::requires_elevation` and `requires_explicit_confirm` are `const fn` — usable in static assertions by M039.

---

*M032 Runbook Schema | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
