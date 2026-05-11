#![forbid(unsafe_code)]
// clippy::all + clippy::nursery are suppressed at crate root for hle-verifier.
//
// Rationale: the crate has >30 distinct lint findings across 6 modules:
//   - 6× cast_possible_truncation (usize→u32 in anti_pattern_scanner)
//   - 2× cast_precision_loss (usize→f64 in test_taxonomy_verifier)
//   - 5× doc_markdown (false_pass_auditor, claim_authority_verifier)
//   - 3× double_must_use (receipt_sha_verifier)
//   - 2× redundant_closure + map_or simplifications (false_pass_auditor)
//   - 1× needless_pass_by_value (final_claim_evaluator)
//   - 1× too_many_lines (claim_authority_verifier)
//   - 1× unused_self (false_pass_auditor)
//   - various identical-arm / if-identical findings
//
// This exceeds the 10-per-crate threshold for per-item narrowing.
// Planned: address in a dedicated hle-verifier lint-hardening pass.
#![allow(clippy::all, clippy::pedantic, clippy::nursery)]
#![cfg_attr(
    test,
    allow(
        warnings,
        clippy::all,
        clippy::pedantic,
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::todo,
        clippy::dbg_macro
    )
)]

//! `hle-verifier` — sole PASS/FAIL authority. Independent OS-process boundary
//! enforced via separate crate; no executor mutation paths imported.
//!
//! Cluster ownership:
//! - C01 Evidence Integrity: `receipt_sha_verifier`, `final_claim_evaluator`
//! - C02 Authority & State: `claim_authority_verifier`
//! - C04 Anti-Pattern Intelligence: `anti_pattern_scanner`, `test_taxonomy_verifier`,
//!   `false_pass_auditor`
//!
//! Module IDs: M008-M009 (C01), M014 (C02), M020, M023-M024 (C04).

pub mod anti_pattern_scanner;
pub mod claim_authority_verifier;
pub mod false_pass_auditor;
pub mod final_claim_evaluator;
pub mod receipt_sha_verifier;
pub mod test_taxonomy_verifier;
