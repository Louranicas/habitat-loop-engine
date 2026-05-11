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

//! `hle-core` — foundation types and authority primitives for the Habitat Loop Engine.
//!
//! Cluster ownership:
//! - C01 Evidence Integrity: `evidence::receipt_hash`, `evidence::claims_store`
//! - C02 Authority & State:  `authority::claim_authority`, `state::workflow_state`
//! - C04 Anti-Pattern Intelligence: `testing::test_taxonomy`
//!
//! End-to-end stack cross-reference: M005-M006, M010-M011, M022 / L01 / C01,C02,C04.
//! Keep alignment with `CLAUDE.local.md` -> `README.md` -> `MASTER_INDEX.md` -> `ULTRAMAP.md`
//! -> `ai_specs/modules/INDEX.md` -> `ai_docs/modules/` -> source.

pub mod authority;
pub mod evidence;
pub mod state;
pub mod testing;
