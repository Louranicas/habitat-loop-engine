# M023 Test Taxonomy Verifier — test_taxonomy_verifier.rs

> **File:** `crates/hle-verifier/src/test_taxonomy_verifier.rs` | **LOC:** ~340 | **Tests:** ~55
> **Layer:** L04 | **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE
> **Role:** Inspects test suites against the taxonomy vocabulary (M022) and rejects inflated or vacuous test collections before they can contribute to a PASS claim

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|---|---|---|---|
| `TaxonomyVerifier` | struct | No | Entry point; holds policy config |
| `TaxonomyPolicy` | struct | No | Builder-constructed acceptance thresholds |
| `TaxonomyReport` | struct | No | Per-module verdict and per-test labels |
| `RejectionReason` | enum | Yes | Classified reason for a module-level rejection |
| `TestVerdictEntry` | struct | No | Per-test pass/reject decision |
| `InflationClass` | enum | Yes | Subtype of inflation finding |
| `ModuleTestProfile` | struct | No | Summary counts used to evaluate a module |

---

## TaxonomyPolicy

```rust
#[derive(Debug, Clone)]
pub struct TaxonomyPolicy {
    /// Minimum number of Behavioral or Property tests in a module.
    pub min_behavior_bearing:    usize,    // Default: 1 (at least one non-vacuous test)
    /// Maximum fraction of Smoke tests permitted. 0.5 = at most 50% Smoke.
    pub max_smoke_fraction:      f64,      // Default: 0.5; clamped [0.0, 1.0]
    /// Whether Doctest counts toward the behavior-bearing minimum.
    pub doctests_count_as_behavioral: bool, // Default: false
    /// Hard limit on total Excluded tests per module.
    pub max_excluded:            usize,    // Default: 5; >5 requires explicit override
}
```

| Builder Method | Notes |
|---|---|
| `TaxonomyPolicy::builder()` | Returns `TaxonomyPolicyBuilder` |
| `min_behavior_bearing(usize)` | Minimum 1; zero is rejected at build time |
| `max_smoke_fraction(f64)` | Clamped to [0.0, 1.0] |
| `doctests_count_as_behavioral(bool)` | |
| `max_excluded(usize)` | |
| `build()` | Returns `Result<TaxonomyPolicy>` |

**Default:** `TaxonomyPolicy::default()` uses the values shown above.

---

## TaxonomyVerifier

```rust
pub struct TaxonomyVerifier {
    policy: TaxonomyPolicy,
}
```

### Construction

```rust
impl TaxonomyVerifier {
    pub fn new(policy: TaxonomyPolicy) -> Self;
    pub fn with_default_policy() -> Self;
}
```

### Core Methods

| Method | Signature | Notes |
|---|---|---|
| `verify_module` | `fn(&self, module_path: &str, descriptors: &[TestDescriptor]) -> Result<TaxonomyReport>` | Evaluates one module's test suite |
| `verify_workspace` | `fn(&self, modules: &[(&str, Vec<TestDescriptor>)]) -> Result<Vec<TaxonomyReport>>` | Evaluates all modules; returns one report per module |
| `is_module_passing` | `fn(&self, report: &TaxonomyReport) -> bool` | True only if `report.rejection` is `None` |

---

## TaxonomyReport

```rust
#[derive(Debug, Clone)]
pub struct TaxonomyReport {
    pub module_path:  String,
    pub profile:      ModuleTestProfile,
    pub test_entries: Vec<TestVerdictEntry>,
    pub rejection:    Option<RejectionReason>,
    pub rationale:    String,   // Human-readable summary; required even if passing
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_passing` | `fn(&self) -> bool` | `rejection.is_none()` |
| `rejected_tests` | `fn(&self) -> Vec<&TestVerdictEntry>` | Only entries marked rejected |
| `passing_tests` | `fn(&self) -> Vec<&TestVerdictEntry>` | Only entries marked passing |
| `vacuous_count` | `fn(&self) -> usize` | Total tests flagged with any `VacuousPattern` |

---

## RejectionReason

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectionReason {
    /// The module has no behavior-bearing tests (Behavioral or Property).
    NoBehavioralTests,
    /// The fraction of Smoke tests exceeds TaxonomyPolicy::max_smoke_fraction.
    TooManySmoke,
    /// The module has more Excluded tests than TaxonomyPolicy::max_excluded.
    TooManyExcluded,
    /// One or more tests contain a VacuousPattern and inflated test count.
    VacuousTestInflation,
    /// All tests are Smoke with no Behavioral companion.
    SmokeOnlyModule,
}
```

**Traits:** `Display` with explanation sentence, e.g. `"VacuousTestInflation: at least one test contains assert!(true) or equivalent tautological assertion"`

---

## InflationClass

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InflationClass {
    /// assert!(true) or assert!(false) — unconditionally passes or fails
    Tautological,
    /// assert_eq!(x, x) — same expression on both sides
    SameVariableBothSides,
    /// Test function body contains no assertion at all
    NoAssertions,
    /// Result<T> returned by a call is silently discarded with no check
    SilentResultDiscard,
    /// Assertion condition is a literal compile-time constant
    LiteralConstantCondition,
}
```

`InflationClass` is the M023 classification of a `VacuousPattern` after source analysis. One
`VacuousPattern` may map to one or more `InflationClass` values when the verifier runs heuristic
decomposition.

---

## TestVerdictEntry

```rust
#[derive(Debug, Clone)]
pub struct TestVerdictEntry {
    pub descriptor:     TestDescriptor,
    pub verdict:        TestVerdict,
    pub inflation_class: Option<InflationClass>,
    pub rationale:      String,
}
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVerdict {
    Passing,
    Rejected,
}
```

---

## ModuleTestProfile

```rust
#[derive(Debug, Clone)]
pub struct ModuleTestProfile {
    pub total:              usize,
    pub behavioral_count:   usize,
    pub property_count:     usize,
    pub smoke_count:        usize,
    pub doctest_count:      usize,
    pub excluded_count:     usize,
    pub vacuous_count:      usize,
    pub behavior_bearing:   usize,   // behavioral + property (+ doctest if policy allows)
    pub smoke_fraction:     f64,     // smoke_count / total.max(1)
}
```

| Method | Signature | Notes |
|---|---|---|
| `meets_policy` | `fn(&self, policy: &TaxonomyPolicy) -> Result<(), RejectionReason>` | Returns first failing reason |

---

## Known Vacuous Patterns Detected

The following source patterns trigger `VacuousPattern` classification:

| Source pattern | VacuousPattern | InflationClass |
|---|---|---|
| `assert!(true)` | `AssertTrue` | `Tautological` |
| `assert_eq!(x, x)` (identical ident) | `TautologicalEq` | `SameVariableBothSides` |
| Test body with no `assert*` macro call | `NoAssertions` | `NoAssertions` |
| `assert!(CONSTANT)` where constant resolves to true | `ConstantTrue` | `LiteralConstantCondition` |
| `let _ = call_returning_result();` with no assertion | `UnassertedResult` | `SilentResultDiscard` |

Negative controls: tests that check `Result::is_ok()`, use `assert_ne!`, or chain `?` properly
are NOT flagged. See `tests/negative_controls/taxonomy_negatives.rs`.

---

## Design Notes

- `TaxonomyVerifier` is stateless after construction; all mutable state flows through the
  `TaxonomyReport` output. This makes the verifier safe to share as `Arc<TaxonomyVerifier>`.
- `RejectionReason::VacuousTestInflation` is distinct from `NoBehavioralTests`: a module may
  have 10 tests that all contain `assert!(true)`. It passes the count check but fails the
  vacuity check. Both are required to prevent Goodhart's Law gaming of the minimum-count rule.
- `TaxonomyReport::rationale` is mandatory even on passing reports so that a downstream auditor
  (M024) can verify that the verifier actually evaluated the module, not just returned a default
  `None` rejection. An empty rationale string causes `verify_module` to return error 2320.
- The verifier consumes M022 vocabulary types (`TestKind`, `ClusterRole`, `LoopPhaseSet`,
  `VacuousPattern`) but does not redefine them. This respects the strict layer DAG: L04 imports
  L01; L01 does not import L04.

---

*M023 Test Taxonomy Verifier Spec v1.0 | 2026-05-10*
