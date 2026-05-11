# M024 False Pass Auditor — false_pass_auditor.rs

> **File:** `crates/hle-verifier/src/false_pass_auditor.rs` | **LOC:** ~360 | **Tests:** ~60
> **Layer:** L04 | **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE
> **Role:** Flagship HLE-SP-001 detector — walks gate JSON and receipt ledger, flags every PASS claim that lacks the four required anchored evidence fields

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|---|---|---|---|
| `FalsePassAuditor` | struct | No | Entry point; walks gate JSON + receipt chain |
| `AuditConfig` | struct | No | Builder-constructed auditor configuration |
| `AuditInput` | struct | No | Gate JSON bytes + optional receipt store handle |
| `AuditReport` | struct | No | Complete per-claim findings for one audit run |
| `ClaimFinding` | struct | No | Finding for a single PASS claim |
| `AnchorKind` | enum | Yes | The four required anchor types |
| `MissingAnchor` | struct | No | Which anchor is absent and why |
| `AuditVerdict` | enum | Yes | Clean / Findings / Blocked |
| `CounterEvidenceLocator` | struct | No | Parsed `^Counter_evidence_locator` value |

---

## AnchorKind — The Four Required Fields

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKind {
    /// ^Verdict — explicit PASS/FAIL/AWAITING_HUMAN verdict string.
    Verdict,

    /// ^Manifest_sha256 — SHA-256 of the scaffold manifest at claim time.
    /// Format: 64-char lowercase hex matching `^[0-9a-f]{64}$`.
    ManifestSha256,

    /// ^Framework_sha256 — SHA-256 of the framework/source tree at claim time.
    /// Format: 64-char lowercase hex matching `^[0-9a-f]{64}$`.
    FrameworkSha256,

    /// ^Counter_evidence_locator — pointer to negative-control artifacts that
    /// the PASS claim survived. Must be a non-empty string path or URI.
    CounterEvidenceLocator,
}
```

**Constants:** `ALL: [Self; 4]`

| Method | Signature | Notes |
|---|---|---|
| `anchor_key` | `const fn(self) -> &'static str` | Returns the literal field key: `"^Verdict"`, `"^Manifest_sha256"`, `"^Framework_sha256"`, `"^Counter_evidence_locator"` |
| `description` | `const fn(self) -> &'static str` | Human-readable requirement |
| `error_code` | `const fn(self) -> u32` | 2340 / 2341 / 2342 / 2343 |

**Traits:** `Display` (emits `anchor_key` value), `AsRef<str>`

---

## MissingAnchor

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAnchor {
    pub kind:     AnchorKind,
    pub field_key: &'static str,   // Same as AnchorKind::anchor_key()
    pub rationale: String,          // Why it is considered missing (field absent vs. malformed)
}
```

A `MissingAnchor` is generated for each of the four `AnchorKind` values that fails
validation in a PASS claim. A claim with zero missing anchors is considered anchored.

---

## ClaimFinding

```rust
#[derive(Debug, Clone)]
pub struct ClaimFinding {
    /// JSON path within the gate document that identifies this claim.
    pub claim_path:       String,

    /// Verdict string found in the claim, if any.
    pub raw_verdict:      Option<String>,

    /// Whether this finding is for a claim asserting PASS (or equivalent).
    pub is_pass_claim:    bool,

    /// Anchors that are absent or malformed.
    pub missing_anchors:  Vec<MissingAnchor>,

    /// Result of walking the ^Counter_evidence_locator if present and parseable.
    pub counter_evidence: Option<CounterEvidenceResult>,

    /// Severity of this finding: High if any missing anchor, Critical if all four absent.
    pub severity:         Severity,
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_false_pass` | `fn(&self) -> bool` | `is_pass_claim && !missing_anchors.is_empty()` |
| `anchor_count` | `fn(&self) -> usize` | Number of present, valid anchors (0..=4) |
| `is_fully_anchored` | `fn(&self) -> bool` | `missing_anchors.is_empty()` |

---

## CounterEvidenceLocator

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterEvidenceLocator {
    /// Raw value of the `^Counter_evidence_locator` field.
    pub raw: String,

    /// Whether the locator resolves to an existing file or accessible URI.
    pub is_resolvable: bool,
}
```

The auditor validates that `raw` is non-empty and (when configured with
`AuditConfig::resolve_counter_evidence = true`) that the path exists on the local
filesystem. A non-resolvable locator is reported as a `MissingAnchor` for
`AnchorKind::CounterEvidenceLocator`.

---

## CounterEvidenceResult

```rust
#[derive(Debug, Clone)]
pub enum CounterEvidenceResult {
    Resolved { path: String },
    NonResolvable { raw: String, reason: String },
    ResolutionSkipped,   // when AuditConfig::resolve_counter_evidence = false
}
```

---

## AuditConfig

```rust
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// When true, the auditor attempts to verify that `^Counter_evidence_locator`
    /// values point to existing filesystem paths. Default: false (scaffold mode).
    pub resolve_counter_evidence: bool,

    /// Maximum number of claims to audit per invocation. Enforces bounded execution.
    /// Default: 1000; capped at 10_000.
    pub max_claims: usize,

    /// Hex pattern for manifest SHA validation. Default: `^[0-9a-f]{64}$`.
    pub sha256_pattern: String,
}
```

| Builder Method | Notes |
|---|---|
| `AuditConfig::builder()` | Returns `AuditConfigBuilder` |
| `resolve_counter_evidence(bool)` | |
| `max_claims(usize)` | Clamped to 1..=10_000 |
| `sha256_pattern(impl Into<String>)` | Must be a valid regex; errors at build time otherwise |
| `build()` | Returns `Result<AuditConfig>` |

**Default:** `AuditConfig::default()` — resolve = false, max_claims = 1000, standard sha256 pattern.

---

## AuditInput

```rust
#[derive(Debug)]
pub struct AuditInput {
    /// Gate JSON document bytes.
    pub gate_json:     Vec<u8>,

    /// Optional reference to the C01 receipt store for receipt-chain walking.
    /// When None, receipt chain verification is skipped (advisory mode only).
    pub receipt_store: Option<Arc<dyn ReceiptStoreRead>>,
}
```

| Factory | Notes |
|---|---|
| `AuditInput::from_json_bytes(Vec<u8>) -> Result<Self>` | Validates UTF-8 and JSON parse; does not walk receipts |
| `AuditInput::from_json_with_receipts(Vec<u8>, Arc<dyn ReceiptStoreRead>) -> Result<Self>` | Full mode |

---

## FalsePassAuditor

```rust
pub struct FalsePassAuditor {
    config: AuditConfig,
}
```

### Construction

```rust
impl FalsePassAuditor {
    pub fn new(config: AuditConfig) -> Self;
    pub fn with_default_config() -> Self;
}
```

### Core Methods

| Method | Signature | Notes |
|---|---|---|
| `audit` | `fn(&self, input: AuditInput) -> Result<AuditReport>` | Primary entry point — walks gate JSON, evaluates every claim |
| `audit_bytes` | `fn(&self, gate_json: &[u8]) -> Result<AuditReport>` | Convenience — no receipt store, scaffold-mode only |
| `audit_claim_object` | `fn(&self, claim: &serde_json::Value, path: &str) -> Result<ClaimFinding>` | Evaluate a single JSON object as a claim |

### Internal Evaluation Sequence (per PASS claim)

```
1. Parse claim as serde_json::Value
2. Extract raw verdict string; skip if not "PASS" (case-insensitive)
3. For each AnchorKind in AnchorKind::ALL:
   a. Check field key presence in claim object
   b. If ManifestSha256 or FrameworkSha256: validate hex pattern match
   c. If CounterEvidenceLocator: parse + conditionally resolve
   d. If absent or malformed: append MissingAnchor
4. Build ClaimFinding with severity:
   - Critical if all 4 anchors missing
   - High    if 1-3 anchors missing
   - Low     if present (sanity-check pass for fully-anchored claims)
5. If is_false_pass: push finding to AuditReport::false_pass_findings
```

---

## AuditReport

```rust
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// All claims inspected (PASS and non-PASS).
    pub all_findings:          Vec<ClaimFinding>,

    /// Subset of all_findings where is_false_pass() == true.
    pub false_pass_findings:   Vec<ClaimFinding>,

    /// Verdict for the whole document.
    pub verdict:               AuditVerdict,

    /// Total claims evaluated (capped at AuditConfig::max_claims).
    pub claims_evaluated:      usize,

    /// Total PASS claims found.
    pub pass_claims_found:     usize,

    /// True when evaluation stopped early due to max_claims limit.
    pub truncated:             bool,

    /// Rationale string (required; empty string causes audit() to return Err(2330)).
    pub rationale:             String,
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_clean` | `fn(&self) -> bool` | `false_pass_findings.is_empty()` |
| `highest_severity` | `fn(&self) -> Option<Severity>` | Across all false-pass findings |
| `should_block` | `fn(&self) -> bool` | `verdict == AuditVerdict::Blocked` |
| `critical_count` | `fn(&self) -> usize` | Findings with severity == Critical |

---

## AuditVerdict

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditVerdict {
    /// No false-pass claims detected.
    Clean,

    /// One or more false-pass claims found; workflow may continue with advisory annotation.
    Findings,

    /// One or more Critical-severity false-pass claims found; workflow must block.
    Blocked,
}
```

**Promotion rules:**
- `Clean` → `Findings` when any `ClaimFinding::is_false_pass()` with severity < Critical
- `Clean` → `Blocked` when any `ClaimFinding::is_false_pass()` with severity == Critical

**Traits:** `Display` (`"CLEAN"` / `"FINDINGS"` / `"BLOCKED"`)

---

## Negative Controls

The following gate JSON shapes must NOT fire `is_false_pass()`:

```json
// Fully anchored PASS — all 4 fields present and valid
{
  "verdict": "PASS",
  "^Verdict": "PASS",
  "^Manifest_sha256": "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a",
  "^Framework_sha256": "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d",
  "^Counter_evidence_locator": "tests/negative_controls/taxonomy_negatives.rs"
}

// Non-PASS claim — auditor skips
{
  "verdict": "AWAITING_HUMAN"
}

// FAIL claim — not a false pass
{
  "verdict": "FAIL",
  "^Verdict": "FAIL"
}
```

The following shapes MUST fire `is_false_pass()`:

```json
// Missing all four anchors
{ "verdict": "PASS" }

// Malformed manifest SHA (wrong length)
{
  "verdict": "PASS",
  "^Verdict": "PASS",
  "^Manifest_sha256": "tooshort",
  "^Framework_sha256": "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a",
  "^Counter_evidence_locator": "tests/controls.rs"
}

// Missing counter-evidence locator
{
  "verdict": "PASS",
  "^Verdict": "PASS",
  "^Manifest_sha256": "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a",
  "^Framework_sha256": "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d"
}
```

Negative-control fixtures live in `tests/negative_controls/false_pass_negatives.rs` and
`tests/false_pass_fixtures/`. The quality gate runs them as part of the standard test suite.

---

## Design Notes

- `FalsePassAuditor` is stateless after construction. All inputs flow through `AuditInput`;
  all outputs flow through `AuditReport`. The struct is safe to share as `Arc<FalsePassAuditor>`.
- The four `AnchorKind` values correspond exactly to the four fields named in `HARNESS_CONTRACT.md`
  (§ "Required receipt fields for PASS promotion"). Implementers must not add a fifth anchor without
  updating `AnchorKind::ALL` and the corresponding `HARNESS_CONTRACT.md` section.
- Receipt-chain walking via `AuditInput::receipt_store` is optional. When provided, the auditor
  verifies that the `^Manifest_sha256` value matches a `ReceiptHash` present in the C01 ledger
  (M007). A sha256 that parses correctly but is absent from the ledger is downgraded from Clean to
  Findings (not Blocked), because the receipt may have been produced by a separate workflow run
  not accessible in this store.
- `AuditVerdict::Blocked` is the strongest signal. A workflow that receives a Blocked report must
  not promote any claim to Final state (M009 gate). The CLI (C08 M044) translates Blocked to a
  non-zero exit code.
- `AuditReport::truncated = true` when `max_claims` is exceeded. Downstream consumers must treat
  a truncated report as advisory only; they cannot conclude the remaining claims are clean.
- No method in this module holds a lock while calling an external function. The C01 receipt store
  is read through a trait object (`Arc<dyn ReceiptStoreRead>`) and its locking is internal to C01.
  C6 compliance is maintained.

---

## Relation to HLE-SP-001

`FalsePassAuditor` is the executable materialization of predicate `HLE-SP-001`:

> A future detector for `FP_FALSE_PASS_CLASSES` must identify evidence of false PASS classes
> where scaffold receipts appear green without implementation-grade evidence in source,
> configuration, verifier receipts, or scaffold review artifacts.

The auditor satisfies HLE-SP-001 by:

1. Requiring all four `AnchorKind` fields — not just file presence or registry size.
2. Providing negative-control fixtures that must not fire the detector.
3. Producing `MissingAnchor` entries that name the affected field, describe the semantic
   correction needed, and point to the `HARNESS_CONTRACT.md` requirement section.
4. Emitting `AuditReport::rationale` on every run so downstream code can verify the
   auditor actually evaluated the document (not returned a default clean verdict).

---

*M024 False Pass Auditor Spec v1.0 | 2026-05-10*
