#![forbid(unsafe_code)]
// clippy::nursery suppressed at crate root: `expected_receipt_verdict` in the
// `substrate-types` dependency triggers `clippy::nursery` (could-be-const-fn).
// That crate is a sealed substrate outside this crate's authority to modify.
// All other lint groups (style, complexity, perf, pedantic, suspicious) are
// fully clean and enforced workspace-wide.
#![allow(clippy::nursery)]
// Test-only relaxations: tests may use expect()/unwrap()/panic for ergonomic assertions.
// Production code remains under workspace-wide deny.
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

//! `hle-runbook` — L7 typed runbook workflow definitions with `AwaitingHuman` semantics.
//!
//! Cluster ownership: C06 Runbook Semantics (M032-M039).
//!
//! Runbook is a KIND of workflow definition, NOT a parallel workflow engine.
//! See `phase_map` for the seam mapping runbook phases onto executor phases.

pub mod human_confirm;
pub mod incident_replay;
pub mod manual_evidence;
pub mod parser;
pub mod phase_map;
pub mod safety_policy;
pub mod scaffold;
pub mod schema;
