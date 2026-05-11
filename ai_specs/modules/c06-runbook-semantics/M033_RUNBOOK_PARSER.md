# M033 Runbook Parser — `crates/hle-runbook/src/parser.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2500-2530
> **Role:** Parse and validate TOML runbook files into typed `Runbook` structs.
> **LOC target:** ~350 | **Test target:** ≥50

---

## Purpose

M033 is the single entry point for converting TOML on disk into the typed `Runbook` from M032. It applies all structural validation rules — required fields, constraint ranges, and phase dependency checking — before returning. A `Runbook` returned by M033 is guaranteed to satisfy all M032 invariants. Callers do not validate; they parse.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `RunbookParser` | struct | Stateless; all methods are effectively pure |
| `ParseError` | enum | 4 variants covering 2500-2530 |
| `ParseOptions` | struct | Parse-time configuration (strict mode, path context) |
| `PhaseDepGraph` | struct (private) | Internal adjacency list for cycle detection |

---

## Error Enum: `ParseError`

```rust
/// Errors produced by RunbookParser. Maps to error codes 2500-2530.
#[derive(Debug)]
pub enum ParseError {
    /// Code 2500 — TOML syntax error or schema mismatch.
    Toml {
        line: Option<u32>,
        column: Option<u32>,
        message: String,
    },
    /// Code 2510 — Required field absent or constraint violated.
    Validation {
        field: &'static str,
        reason: String,
    },
    /// Code 2520 — Circular dependency in phase trigger/dependency graph.
    CircularPhase {
        cycle: Vec<String>,
    },
    /// Code 2530 — `max_traversals` in the TOML exceeds the system maximum.
    MaxTraversalsExceeded {
        declared: u32,
        system_max: u32,
    },
}
```

`ParseError` implements `std::error::Error`, `Display`, and the crate's `ErrorClassifier` trait:
- `Toml` → code 2500, severity Medium
- `Validation` → code 2510, severity Medium
- `CircularPhase` → code 2520, severity High
- `MaxTraversalsExceeded` → code 2530, severity High

---

## Struct: `ParseOptions`

```rust
/// Configuration for a parse call.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Directory used to resolve relative `canonical_schematic` and `canonical_runbook` paths.
    pub base_dir: Option<std::path::PathBuf>,
    /// When true, unknown TOML fields are rejected rather than silently ignored.
    pub strict: bool,
    /// Maximum `max_traversals` value the system accepts (default: 100).
    pub system_max_traversals: u32,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            base_dir: None,
            strict: false,
            system_max_traversals: 100,
        }
    }
}
```

---

## Struct: `RunbookParser`

```rust
/// Stateless TOML → Runbook parser and validator.
///
/// All methods are `&self` and functionally pure — the parser holds no mutable state.
/// Construct once and reuse, or use the free-function convenience wrappers.
#[derive(Debug, Clone, Default)]
pub struct RunbookParser {
    options: ParseOptions,
}
```

---

## Method Table

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn() -> Self` | `ParseOptions::default()` |
| `with_options` | `fn(options: ParseOptions) -> Self` | |
| `parse_str` | `fn(&self, toml: &str) -> Result<Runbook, ParseError>` | Primary entry point |
| `parse_file` | `fn(&self, path: &std::path::Path) -> Result<Runbook, ParseError>` | Reads file, delegates to `parse_str` |
| `parse_bytes` | `fn(&self, bytes: &[u8]) -> Result<Runbook, ParseError>` | UTF-8 decode then `parse_str` |
| `validate_only` | `fn(&self, runbook: &Runbook) -> Result<(), ParseError>` | Re-runs validation on already-parsed struct |

**Free functions (convenience wrappers):**

```rust
/// Parse a TOML string with default options.
pub fn parse_str(toml: &str) -> Result<Runbook, ParseError>;

/// Parse a TOML file with default options.
pub fn parse_file(path: &std::path::Path) -> Result<Runbook, ParseError>;
```

---

## Validation Rules (enforced by `parse_str`)

The parser enforces rules in this order, returning the first encountered error:

### Step 1: TOML Syntax (→ `ParseError::Toml`)
- Raw TOML deserialization using `toml` crate.
- In strict mode, unknown top-level keys produce `ParseError::Toml`.

### Step 2: Required Field Checks (→ `ParseError::Validation`)

| Field | Rule | Error field |
|-------|------|-------------|
| `id` | Non-empty, matches `[a-z0-9_-]+` | `"id"` |
| `title` | Non-empty string | `"title"` |
| `max_traversals` | `>= 1` | `"max_traversals"` |
| `phases` | At least one phase entry | `"phases"` |
| `safety_class` | One of "soft" / "hard" / "safety" | `"safety_class"` |

### Step 3: Constraint Checks (→ `ParseError::MaxTraversalsExceeded` or `Validation`)

```
if declared_max_traversals > options.system_max_traversals {
    return Err(ParseError::MaxTraversalsExceeded { declared, system_max })
}
```

Non-idempotent runbooks with `max_traversals > 1` emit a `ParseError::Validation` warning in strict mode (not an error in permissive mode, but logged).

### Step 4: Phase Dependency Cycle Check (→ `ParseError::CircularPhase`)

Runbooks may declare `trigger` fields that reference other phase IDs. The parser builds a `PhaseDepGraph` and runs depth-first cycle detection:

```rust
/// Returns Err if a cycle exists in the trigger dependency graph.
fn check_phase_cycles(phases: &HashMap<PhaseKind, Phase>) -> Result<(), ParseError>;
```

In current Framework §17.8, phases do not have cross-references, so this check always passes for spec-compliant TOML. The implementation exists to catch future schema extensions and custom runbooks.

---

## Internal TOML Shadow Type

The parser uses private shadow types for deserialization, then converts to M032 types:

```rust
// Private: only visible inside parser.rs
#[derive(serde::Deserialize)]
struct RawRunbook {
    id: String,
    title: String,
    habitat_history: Option<String>,
    failure_signature: Option<String>,
    mode_applicability: Option<RawModeApplicability>,
    canonical_schematic: Option<String>,
    canonical_runbook: Option<String>,
    max_traversals: Option<u32>,
    idempotent: Option<bool>,
    safety_class: Option<String>,
    // phases deserialized as Vec<RawPhase> then folded into HashMap
    #[serde(default)]
    phases: std::collections::HashMap<String, RawPhase>,
}
```

The `RawRunbook → Runbook` conversion applies all validation rules and calls `RunbookBuilder::build()` which enforces M032 invariants.

---

## Design Notes

- `parse_str` is the single source of truth; `parse_file` and `parse_bytes` are thin wrappers that resolve to `parse_str`. This keeps the validation surface small and testable.
- The shadow type pattern (private `RawRunbook` → public `Runbook`) prevents `serde::Deserialize` from leaking into the public M032 types, keeping the schema module dependency-free from serde.
- `ParseOptions::strict` defaults to `false` for operational runbooks found on disk, but test fixtures should use `strict: true` to detect schema drift early.
- The cycle check uses DFS on an adjacency list rather than a full graph library to keep the dependency surface minimal. The graph has at most 5 nodes (one per `PhaseKind`), so complexity is bounded regardless.
- `validate_only` enables M038 (incident replay) to re-validate imported fixtures after deserialization from a different format.

---

## Example Usage

```rust
// Parse a runbook from a TOML string
let toml = r#"
[runbook]
id = "s112-bridge-breaker"
title = "S112 Bridge Breaker Recovery"
max_traversals = 3
idempotent = true
safety_class = "hard"

[runbook.mode_applicability]
local_m0 = true

[phases.detect]
probes = [{ id = "check-breaker", description = "Check circuit breaker state" }]
"#;

let runbook = parse_str(toml)?;
assert_eq!(runbook.id.as_str(), "s112-bridge-breaker");
assert_eq!(runbook.safety_class, SafetyClass::Hard);
```

---

## Cluster Invariants (this module)

- A `Runbook` returned by any parse method has passed all four validation steps.
- `parse_file` propagates `std::io::Error` as `ParseError::Toml` with a descriptive message — it does not use a separate IO error variant.
- `parse_str("")` (empty string) returns `ParseError::Toml`, not `ParseError::Validation` — the empty input is a syntax error, not a field error.
- `validate_only` produces identical results to re-parsing the runbook through `parse_str` with the same options.

---

*M033 Runbook Parser | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
