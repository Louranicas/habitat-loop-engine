#![forbid(unsafe_code)]
// clippy::nursery suppressed at crate root: `expected_receipt_verdict` in the
// `substrate-types` dependency triggers `clippy::nursery` (could-be-const-fn).
// That crate is a sealed substrate outside this crate's authority to modify.
// All other lint groups (style, complexity, perf, pedantic, suspicious) are
// fully clean and enforced workspace-wide.
#![allow(clippy::nursery)]
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

//! `hle-storage` — persistence ledger primitives.
//!
//! Cluster ownership:
//! - C01 Evidence Integrity: `receipts_store`
//! - C04 Anti-Pattern Intelligence: `anti_pattern_events`
//! - C05 Persistence Ledger: `pool`, `migrations`, `workflow_runs`, `workflow_ticks`,
//!   `evidence_store`, `verifier_results_store`, `blockers_store`
//!
//! Module IDs: M007 (C01), M021 (C04), M025-M031 (C05).
//! All append-only where applicable per `UP_RECEIPT_GRAPH` (HLE-UP).

pub mod anti_pattern_events;
pub mod blockers_store;
pub mod evidence_store;
pub mod migrations;
pub mod pool;
pub mod receipts_store;
pub mod verifier_results_store;
pub mod workflow_runs;
pub mod workflow_ticks;
