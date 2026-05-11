# M037 Runbook Scaffold — `crates/hle-runbook/src/scaffold.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2599
> **Role:** Pure-function incident runbook skeleton generator — deterministic TOML from typed input.
> **LOC target:** ~240 | **Test target:** ≥50

---

## Purpose

M037 generates starter runbook TOML files for incident-response scenarios. Its single exported function, `scaffold`, takes a `ScaffoldInput` and returns a `RunbookToml` (a newtype over `String`). The function is pure: same input, same output every call. No I/O, no randomness, no side effects. Callers are responsible for writing the output to disk, registering it with the ledger, or passing it directly to M033 for parsing.

The primary consumers of M037 are:
1. The CLI surface (C08 `cli_run`) when an operator runs `hle scaffold --incident <sig>` to bootstrap a new runbook from a known failure signature.
2. M038 (incident replay) when generating fixture runbooks programmatically from the 8 canonical incident signatures.

M037 does not validate that the generated TOML is parseable by M033 at generation time — the caller should do so if needed. This avoids a circular compile dependency. Integration tests verify that every scaffold output round-trips through M033 without error.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `ScaffoldInput` | struct | All parameters needed to generate a runbook |
| `IncidentSignature` | enum | 8 canonical signatures from Framework §17.8 fixture map |
| `ScaffoldPhaseSpec` | struct | Per-phase customization in the scaffold |
| `RunbookToml` | newtype(`String`) | Output type — valid UTF-8 TOML text |
| `ScaffoldOptions` | struct | Optional scaffold-wide configuration |

---

## Enum: `IncidentSignature`

```rust
/// The 8 canonical incident signatures from Framework §17.8 fixture map.
///
/// Each variant corresponds to one replay fixture. The scaffold generator
/// produces a pre-populated TOML for the relevant failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncidentSignature {
    /// S112 bridge circuit breaker port drift (fixture: s112-bridge-breaker.toml)
    S112BridgeBreakerPortDrift,
    /// Maintenance Engine EventBus dark traffic (fixture: me-eventbus-dark.toml)
    MeEventbusDarkTraffic,
    /// POVM write-only / readback-zero (fixture: povm-write-only.toml)
    PovmWriteOnlyReadbackZero,
    /// S117 TTL sweep deletes legitimate entries (fixture: s117-ttl-sweep.toml)
    S117TtlSweepDeletesLegitimate,
    /// Port retirement tombstone collision (fixture: port-retirement.toml)
    PortRetirementTombstoneCollision,
    /// DevEnv batch dependency failure (fixture: devenv-batch-failure.toml)
    DevenvBatchDependencyFailure,
    /// SYNTHEX thermal saturation runaway (fixture: thermal-saturation.toml)
    SynthexThermalSaturationRunaway,
    /// Concurrent markdown write conflict (fixture: concurrent-markdown-write.toml)
    ConcurrentMarkdownWriteConflict,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `all` | `fn() -> [Self; 8]` | All variants in Framework §17.8 fixture order |
| `failure_signature_str` | `const fn(&self) -> &'static str` | UPPERCASE_UNDERSCORE string for runbook field |
| `fixture_path` | `const fn(&self) -> &'static str` | Relative path to the fixture TOML |
| `default_safety_class` | `const fn(&self) -> SafetyClass` | Pre-calibrated per incident type |
| `default_idempotent` | `const fn(&self) -> bool` | Pre-calibrated per incident type |
| `habitat_history_hint` | `const fn(&self) -> &'static str` | Cross-reference to session / incident record |

---

## Struct: `ScaffoldInput`

```rust
/// All parameters needed to generate a runbook skeleton.
///
/// Optional fields are pre-populated from `IncidentSignature` defaults
/// when `from_signature` is used. Override any field with the builder.
#[derive(Debug, Clone)]
pub struct ScaffoldInput {
    /// Runbook identifier (must satisfy RunbookId validation rules).
    pub id: String,
    /// One-line title.
    pub title: String,
    /// Pre-set failure signature, if known.
    pub failure_signature: Option<IncidentSignature>,
    /// Free-form habitat history cross-reference.
    pub habitat_history: Option<String>,
    /// Safety class to embed in the scaffold (default: Soft).
    pub safety_class: SafetyClass,
    /// Whether the generated runbook should declare idempotent = true (default: true).
    pub idempotent: bool,
    /// Maximum traversals to embed (default: 3).
    pub max_traversals: u32,
    /// Which phases to include (default: Detect + Block + Fix + Verify).
    pub phases: Vec<ScaffoldPhaseSpec>,
    /// Scaffold-wide options.
    pub options: ScaffoldOptions,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `from_signature` | `fn(sig: IncidentSignature) -> Self` | Pre-populates all defaults from the signature |
| `builder` | `fn(id: impl Into<String>, title: impl Into<String>) -> ScaffoldInputBuilder` | Manual construction path |

---

## Struct: `ScaffoldPhaseSpec`

```rust
/// Per-phase scaffold configuration.
#[derive(Debug, Clone)]
pub struct ScaffoldPhaseSpec {
    pub kind: PhaseKind,
    /// Optional placeholder trigger condition (written as a TOML comment placeholder).
    pub trigger_hint: Option<String>,
    /// Probe stubs to include (written as commented-out examples).
    pub probe_stubs: Vec<String>,
    /// Evidence paths to include as placeholders.
    pub evidence_hints: Vec<String>,
}
```

---

## Struct: `ScaffoldOptions`

```rust
/// Scaffold-wide generation options.
#[derive(Debug, Clone)]
pub struct ScaffoldOptions {
    /// Include commented-out example probes in each phase (default: true).
    pub include_probe_stubs: bool,
    /// Include framework §17.8 reference comment at file top (default: true).
    pub include_framework_comment: bool,
    /// Indent width in spaces (default: 2).
    pub indent_width: u8,
}

impl Default for ScaffoldOptions {
    fn default() -> Self {
        Self {
            include_probe_stubs: true,
            include_framework_comment: true,
            indent_width: 2,
        }
    }
}
```

---

## Newtype: `RunbookToml`

```rust
/// Output of the scaffold generator — a UTF-8 TOML string.
///
/// The content is a valid scaffold (parseable by M033 after removing comment-only stubs).
/// Call `.as_str()` to write to disk or pass to M033.
#[derive(Debug, Clone, PartialEq)]
pub struct RunbookToml(String);

impl RunbookToml {
    #[must_use]
    pub fn as_str(&self) -> &str;
    #[must_use]
    pub fn into_string(self) -> String;
    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize;
    #[must_use]
    pub fn is_empty(&self) -> bool;
}
```

**Traits:** `Display`, `AsRef<str>`, `From<RunbookToml> for String`

---

## Primary Function: `scaffold`

```rust
/// Generate a runbook TOML skeleton from the provided input.
///
/// # Purity
/// This function is deterministic: the same `ScaffoldInput` always produces
/// the same `RunbookToml`. No I/O, no randomness, no side effects.
///
/// # Arguments
/// - `input` — All parameters for the scaffold. Use `ScaffoldInput::from_signature`
///   for canonical incident types.
///
/// # Returns
/// A `RunbookToml` containing valid TOML text. The text contains placeholder
/// comments in probe and evidence fields; parse with M033 in strict mode
/// only after removing or filling placeholders.
#[must_use]
pub fn scaffold(input: &ScaffoldInput) -> RunbookToml;
```

---

## TOML Output Shape

For a `ScaffoldInput::from_signature(IncidentSignature::S112BridgeBreakerPortDrift)`, the output approximates:

```toml
# Framework §17.8 runbook scaffold — generated by hle-runbook::scaffold
# Incident: S112 Bridge Breaker Port Drift
# Edit placeholders before use.

[runbook]
id = "s112_bridge_breaker_port_drift"
title = "S112 Bridge Breaker Port Drift Recovery"
failure_signature = "S112_BRIDGE_BREAKER_PORT_DRIFT"
habitat_history = "S112 — Bridge Breaker + ORAC bridge circuit"
max_traversals = 3
idempotent = true
safety_class = "hard"

[runbook.mode_applicability]
local_m0 = true

[phases.detect]
trigger = "circuit_breaker_open"
evidence_required = ["# TODO: path/to/breaker-state-evidence"]

[[phases.detect.probes]]
id = "check-breaker-state"
description = "Verify circuit breaker is open on affected bridge port"
command = "# TODO: curl -s http://localhost:<PORT>/health | jq .circuit_breaker"

[phases.verify]
pass_predicate = "circuit_breaker_closed"
fail_predicate = "circuit_breaker_open_after_fix"
```

---

## Design Notes

- The scaffold generator uses simple string building (no template engine dependency) to keep the dependency surface minimal. The output format is validated only by round-trip tests in M038, not inline.
- Probe stubs are emitted as TOML string values with `# TODO:` prefixes rather than as commented-out TOML blocks, because TOML does not have block comments. The M033 parser will parse the `# TODO:` strings as literal values — this is intentional; the operator replaces them before real use.
- `ScaffoldOptions::indent_width` affects phase sub-table indentation. The TOML specification does not require indentation, but consistent formatting makes diffs readable.
- `IncidentSignature::default_safety_class()` calibrations: `S112BridgeBreakerPortDrift` → `Hard`; `MeEventbusDarkTraffic` → `Soft`; `PovmWriteOnlyReadbackZero` → `Hard`; `SynthexThermalSaturationRunaway` → `Safety`; all others → `Hard`. These calibrations are encoded as `const fn` and tested explicitly.

---

## Cluster Invariants (this module)

- `scaffold(&input)` returns identical output for identical `input`. There is no timestamp, UUID, or random element in the output.
- For every `IncidentSignature` variant, `scaffold(&ScaffoldInput::from_signature(sig))` produces output that M033 can parse (after stripping `# TODO:` placeholder fields). This is verified by round-trip integration tests.
- `scaffold` never panics. If `max_traversals == 0`, it substitutes `1` and includes a TOML comment noting the substitution. The caller should use `ScaffoldInput::builder` with validated inputs for production use.
- The 8 `IncidentSignature` variants in this module correspond 1:1 to the 8 fixture IDs in Framework §17.8 fixture map and M038. Adding a new signature without adding a corresponding fixture in M038 violates `INV-C06-06`.

---

*M037 Runbook Scaffold | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
