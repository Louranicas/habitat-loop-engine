# C04 Anti-Pattern Intelligence — Cluster Overview

> **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE | **Modules:** 5 (M020–M024) | **Layers:** L01 / L02 / L04
> **LOC Estimate:** ~1,680 | **Tests Estimate:** ~280
> **Error Code Range:** 2300–2399
> **Synergy:** catalogued anti-patterns become scanner events, test taxonomy checks, and false-pass audits

---

## Purpose

C04 converts the anti-pattern catalog from static markdown into executable evidence. Each known
failure mode — compositional drift, blocking-in-async, nested locks, lock-held signal emission,
guard-lifetime reference leaks, unbounded collections, missing builders, and false-pass classes —
has a corresponding scanner that emits typed `DetectorEvent` values. Those events are persisted by
the append-only store (M021), cross-checked against the test taxonomy (M022 / M023), and audited
for false PASS claims (M024, the flagship `HLE-SP-001` detector).

The cluster's core promise: **a warning in a markdown predicate doc must eventually fire as a
machine-verifiable check**. C04 is the bridge between human-readable anti-pattern definitions and
the verifier gate that either passes or blocks a workflow run.

---

## File Map

```
crates/hle-core/src/testing/
└── test_taxonomy.rs                   # M022 — L01 vocabulary: test kinds, loop phases, cluster roles

crates/hle-storage/src/
└── anti_pattern_events.rs             # M021 — L02 append-only DetectorEvent store

crates/hle-verifier/src/
├── anti_pattern_scanner.rs            # M020 — L04 Scanner trait + 8 concrete scanners
├── test_taxonomy_verifier.rs          # M023 — L04 TaxonomyVerifier rejects vacuous tests
└── false_pass_auditor.rs              # M024 — L04 FalsePassAuditor (HLE-SP-001 flagship)
```

---

## Module Dependency Graph (Internal)

```
M022 test_taxonomy         (L01 — no upward deps)
    ↑
M023 test_taxonomy_verifier (L04 — reads M022 TestKind / LoopPhase)
M020 anti_pattern_scanner  (L04 — reads M022 AntiPatternId; emits DetectorEvent)
    ↓
M021 anti_pattern_events   (L02 — receives DetectorEvent from M020; persists to store)
    ↑
M024 false_pass_auditor    (L04 — reads receipts from C01; references M021 event store)
```

Direction follows the strict layer DAG: L04 may import L01 and L02; L02 may import L01; no
upward imports.

---

## Cross-Cluster Dependencies

| Dependency | Direction | What C04 needs |
|---|---|---|
| `substrate-types::HleError` | C04 → substrate | Unified error type for all `Result` returns |
| `substrate-types::BoundedString` | C04 → substrate | Evidence strings capped at 1024 bytes |
| `substrate-types::SourceLocation` | C04 → substrate | File path + line range in `DetectorEvent` |
| `substrate-types::Severity` | C04 → substrate | Low / Medium / High / Critical on each event |
| C01 `receipt_hash` (M001) | M024 reads C01 | Auditor verifies `^Manifest_sha256` / `^Framework_sha256` against `ReceiptHash` |
| C01 `receipts_store` (M003) | M024 reads C01 | Auditor walks the C01 receipt ledger to resolve counter-evidence locators |
| C02 `claim_authority` (M006) | M024 reads C02 | Auditor checks that PASS claims are stamped by verifier authority, not executor |
| C05 `pool` (M021) | M021 → C05 | `EventStore` uses the shared database pool for append-only writes |

C04 does NOT import from C03 (execution), C06 (runbooks), or C07 (bridges). Events flow
outward from C04 scanners; nothing flows back in.

---

## Concurrency Architecture

| Module | Sync Strategy | Rationale |
|---|---|---|
| M022 `test_taxonomy` | None (pure value types — all `Copy` or `Clone`) | Stateless vocabulary; thread-safe by construction |
| M020 `anti_pattern_scanner` | None (stateless `Scanner` trait impls, `&self`) | Each scan is a pure transform over `ScanInput` |
| M021 `anti_pattern_events` | `parking_lot::Mutex<Connection>` via C05 pool | Append-only WAL SQLite; single writer per connection |
| M023 `test_taxonomy_verifier` | None (stateless after construction) | `Arc<TaxonomyVerifier>` safe; returns owned `TaxonomyReport` |
| M024 `false_pass_auditor` | None (stateless after construction) | `Arc<FalsePassAuditor>` safe; returns owned `AuditReport` |

All `Scanner` implementations use `&self`. `EventStore` wraps the pool's mutex internally;
callers see only `append()` and `query()`. Lock guards are never held across external calls
(C6 compliance: no signal emission while holding a lock).

---

## Design Principles

1. **Catalog-to-code fidelity.** Every predicate ID listed in `ai_docs/anti_patterns/*.md`
   (`AP28`, `AP29`, `AP31`, `C6`, `C7`, `C12`, `C13`, `FP_FALSE_PASS_CLASSES`) maps to exactly one
   scanner constant in `AntiPatternId` and one concrete `Scanner` impl.

2. **Evidence, not counts.** Consistent with `HLE-SP-001`, every `DetectorEvent` carries a
   `BoundedString` evidence field and a `SourceLocation` (file path + line range). File-count-only
   signals are insufficient.

3. **Append-only store.** `EventStore` in M021 accepts `append` and `query` only. No updates, no
   deletes. Events form a monotone audit log, not a mutable registry.

4. **Negative-control discipline.** M023 and M024 each carry a set of known-good fixtures. A
   scanner that fires on a negative control has a bug, not a finding.

5. **SHA anchoring.** `DetectorEvent` carries a `receipt_sha` field referencing the verifier
   receipt chain from C01. A finding without a receipt anchor is advisory only; it cannot promote
   or block a final claim.

6. **No `unwrap`, `expect`, `panic`, `todo`, `dbg`, or `unsafe`.** Enforced by workspace lints.
   All constructors return `Result<T>`.

---

## Error Strategy (2300–2399)

All C04 modules use `substrate_types::HleError` and return `Result<_, HleError>`. The numeric
prefix `[E23xx]` appears in `HleError` message strings for log filtering.

| Code | Variant | Severity | Source module | Trigger |
|---|---|---|---|---|
| 2300 | `ScanInputInvalid` | Low | M020 | Malformed or empty `ScanInput`; evidence string exceeds cap |
| 2301 | `ScannerNotFound` | Low | M020 | Requested `AntiPatternId` not in `AntiPatternId::ALL` |
| 2310 | `EventStoreAppendFailed` | Medium | M021 | Database write error on event append; JSONL flush error |
| 2311 | `EventStoreQueryFailed` | Medium | M021 | Database read error; `row_sha` mismatch on integrity check |
| 2320 | `TaxonomyRejectInvalid` | Low | M022 / M023 | `TaxonomyReport` or `TestDescriptor` constructed with empty rationale |
| 2321 | `VacuousTestRejected` | Medium | M023 | Vacuous test detected: `assert!(true)`, tautological assert, or no assertions |
| 2330 | `AuditInputMissing` | Medium | M024 | Gate JSON document is empty or unparseable |
| 2340 | `MissingVerdictAnchor` | High | M024 | `^Verdict` field absent from PASS claim |
| 2341 | `MissingManifestAnchor` | High | M024 | `^Manifest_sha256` absent or malformed in PASS claim |
| 2342 | `MissingFrameworkAnchor` | High | M024 | `^Framework_sha256` absent or malformed in PASS claim |
| 2343 | `MissingCounterEvidence` | High | M024 | `^Counter_evidence_locator` absent or empty in PASS claim |
| 2344 | `AuditStorageIo` | Medium | M024 | Error reading gate JSON or receipt files during audit walk |
| 2399 | `Other` | Low | C04 (any) | Unclassified C04 error |

---

## Quality Gate Results Template

```
cargo check --workspace --all-targets               PASS  0 errors
cargo clippy --workspace -- -D warnings             PASS  0 warnings
cargo clippy --workspace -- -W pedantic             PASS  0 warnings
cargo test --workspace --all-targets                PASS  ~280 tests, 0 failures
Zero-tolerance grep (unsafe/unwrap/expect/todo/dbg) PASS  0 hits in C04 source
HLE-SP-001 self-audit (FalsePassAuditor::audit)     PASS  all PASS claims carry all 4 anchored fields
Negative-control suite                              PASS  0 scanners fire on known-good fixtures
```

Minimum gate commands:

```bash
# Scaffold gate (no runtime required)
scripts/quality-gate.sh --scaffold --json

# M0 gate (local one-shot bounded)
scripts/quality-gate.sh --m0 --json

# Zero-tolerance grep across all C04 source files
rg -n 'unwrap\(\)|\.expect\(|panic!|todo!|dbg!|unsafe' \
  crates/hle-core/src/testing/test_taxonomy.rs \
  crates/hle-storage/src/anti_pattern_events.rs \
  crates/hle-verifier/src/anti_pattern_scanner.rs \
  crates/hle-verifier/src/test_taxonomy_verifier.rs \
  crates/hle-verifier/src/false_pass_auditor.rs
# Expected: 0 matches
```

---

*C04 Anti-Pattern Intelligence Cluster Overview v1.0 | 2026-05-10*
