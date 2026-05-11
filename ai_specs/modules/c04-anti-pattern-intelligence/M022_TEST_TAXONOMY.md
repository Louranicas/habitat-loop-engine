# M022 Test Taxonomy — test_taxonomy.rs

> **File:** `crates/hle-core/src/testing/test_taxonomy.rs` | **LOC:** ~290 | **Tests:** ~50
> **Layer:** L01 | **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE
> **Role:** Vocabulary model for test classification; consumed by M023 (verifier) and M024 (auditor) to make test-quality claims machine-checkable

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|---|---|---|---|
| `TestKind` | enum | Yes | Behavioral / Property / Smoke / Doctest / Excluded |
| `ClusterRole` | enum | Yes | Producer / Binder / Transformer / Verifier / TerminalConsumer |
| `LoopPhase` | enum | Yes | 8 phases aligned to HLE workflow loop |
| `LoopPhaseSet` | newtype(`u8`) | Yes | Bitmask of `LoopPhase` affinity (8 bits) |
| `TestDescriptor` | struct | No | Fully-classified test metadata |
| `TestDescriptorBuilder` | struct | No | Validated construction |
| `VacuousPattern` | enum | Yes | Known vacuous-assertion patterns detected by M023 |
| `TaxonomyLabel` | struct | No | Structured label applied to a test item |

---

## TestKind

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestKind {
    /// Tests observable behaviour visible at the module boundary.
    /// Must make at least one non-tautological assertion.
    Behavioral,

    /// Property-based tests (proptest / quickcheck style).
    /// Must call a generator; must include at least one shrinking strategy hint.
    Property,

    /// Fast smoke check — single happy-path assertion.
    /// Acceptable only if the module has >= 1 Behavioral test as companion.
    Smoke,

    /// Documentation example compiled and run as a test.
    Doctest,

    /// Intentionally excluded with a documented reason.
    /// The reason string is captured in TestDescriptor::exclusion_reason.
    Excluded,
}
```

**Constants:** `ALL: [Self; 5]`
**Traits:** `Display` (`"Behavioral"` / `"Property"` / `"Smoke"` / `"Doctest"` / `"Excluded"`)

---

## ClusterRole

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClusterRole {
    /// Emits primary data or events consumed downstream.
    Producer,

    /// Connects two modules or clusters; passes through without modifying meaning.
    Binder,

    /// Converts types or representations; semantics change across the boundary.
    Transformer,

    /// Independently checks or recomputes a prior claim.
    Verifier,

    /// Final consumer; no module downstream reads its output.
    TerminalConsumer,
}
```

**Constants:** `ALL: [Self; 5]`
**Traits:** `Display` (`"producer"` / `"binder"` / `"transformer"` / `"verifier"` / `"terminal_consumer"`)

---

## LoopPhase

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LoopPhase {
    Detect   = 0,   // Scan or observe system state
    Block    = 1,   // Halt a workflow on a finding
    Fix      = 2,   // Apply a corrective action
    Verify   = 3,   // Confirm a fix or claim
    MetaTest = 4,   // Test the test infrastructure itself
    Receipt  = 5,   // Produce or consume a verifier receipt
    Persist  = 6,   // Write to the append-only ledger
    Notify   = 7,   // Signal a human or downstream consumer
}
```

**Constants:** `ALL: [Self; 8]`

| Method | Signature | Notes |
|---|---|---|
| `index` | `const fn(self) -> u8` | 0..=7 |
| `name` | `const fn(self) -> &'static str` | e.g. `"detect"` |
| `from_index` | `const fn(u8) -> Option<Self>` | None if >= 8 |
| `from_name` | `fn(&str) -> Option<Self>` | Case-insensitive |

**Traits:** `Display` (`"detect"`)

---

## LoopPhaseSet

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LoopPhaseSet(u8);  // bottom 8 bits
```

| Method | Signature | Notes |
|---|---|---|
| `from_raw` | `const fn(bits: u8) -> Self` | |
| `with_phase` | `const fn(self, p: LoopPhase) -> Self` | Sets bit; chainable |
| `has_phase` | `const fn(self, p: LoopPhase) -> bool` | Bit test |
| `count` | `const fn(self) -> u32` | Popcount |
| `union` | `const fn(self, other: Self) -> Self` | Bitwise OR |
| `active_phases` | `fn(self) -> Vec<LoopPhase>` | In order of `LoopPhase::index` |

**Constants:** `EMPTY = LoopPhaseSet(0)`, `ALL_PHASES = LoopPhaseSet(0xFF)`

---

## VacuousPattern

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VacuousPattern {
    /// assert!(true)
    AssertTrue,
    /// assert_eq!(x, x) — same variable on both sides
    TautologicalEq,
    /// Test body is empty (no assertions at all)
    NoAssertions,
    /// assert!(condition) where condition is a compile-time constant true
    ConstantTrue,
    /// Result unwrap immediately discarded with no assertion on the Ok value
    UnassertedResult,
}
```

**Constants:** `ALL: [Self; 5]`

| Method | Signature | Notes |
|---|---|---|
| `description` | `fn(self) -> &'static str` | Human-readable rationale for M023 rejection message |
| `severity` | `fn(self) -> Severity` | `AssertTrue` / `TautologicalEq` → High; others → Medium |

---

## TestDescriptor

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestDescriptor {
    pub test_name:        String,
    pub module_path:      String,
    pub kind:             TestKind,
    pub cluster_role:     ClusterRole,
    pub loop_phase_set:   LoopPhaseSet,
    pub vacuous_patterns: Vec<VacuousPattern>,  // Populated by M023 analysis
    pub exclusion_reason: Option<String>,        // Required when kind == Excluded
}
```

| Method | Signature | Notes |
|---|---|---|
| `builder` | `fn(test_name: impl Into<String>, module_path: impl Into<String>) -> TestDescriptorBuilder` | |
| `is_vacuous` | `fn(&self) -> bool` | `!vacuous_patterns.is_empty()` |
| `is_behavior_bearing` | `fn(&self) -> bool` | `kind == Behavioral \|\| kind == Property` and `!is_vacuous()` |
| `has_phase` | `fn(&self, p: LoopPhase) -> bool` | Delegates to `loop_phase_set` |

---

## TestDescriptorBuilder

| Builder Method | Notes |
|---|---|
| `kind(TestKind)` | Required |
| `cluster_role(ClusterRole)` | Required |
| `phase(LoopPhase)` | May be called multiple times |
| `phase_set(LoopPhaseSet)` | Alternative to repeated `phase` calls |
| `exclusion_reason(impl Into<String>)` | Required when `kind == Excluded`; errors if kind is other |
| `build()` | Returns `Result<TestDescriptor>` — errors if `kind == Excluded` without reason |

---

## TaxonomyLabel

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyLabel {
    pub test_name:    String,
    pub kind:         TestKind,
    pub cluster_role: ClusterRole,
    pub phases:       LoopPhaseSet,
}
```

`TaxonomyLabel` is the compact form emitted by M023 in a `TaxonomyReport` — one label per
inspected test. It does not carry `vacuous_patterns`; those remain on the full `TestDescriptor`
used internally.

---

## Design Notes

- `LoopPhaseSet` is a bitmask rather than `Vec<LoopPhase>` so it is `Copy` and amenable to
  `const fn` composition. A test affiliated with multiple phases (e.g., `Detect | Persist`) sets
  both bits without heap allocation.
- `VacuousPattern` is defined in L01 so M023 (L04) and M024 (L04) share the same classification
  vocabulary without a dependency inversion.
- `TestKind::Smoke` is conditionally valid: M023 will reject a module where every test is `Smoke`
  with no `Behavioral` companions. The vocabulary itself does not encode this rule; M023 does.
- `TestKind::Excluded` requires a reason string at construction time to prevent silent coverage
  gaps — a compile-time guard against quiet test exclusion without documentation.
- All 6 types are `#[must_use]` on every pure method.

---

*M022 Test Taxonomy Spec v1.0 | 2026-05-10*
