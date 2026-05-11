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

//! `hle-executor` — bounded one-shot execution primitives.
//!
//! Cluster ownership:
//! - C02 Authority & State: `state_machine`, `status_transitions`
//! - C03 Bounded Execution: `bounded`, `local_runner`, `phase_executor`,
//!   `timeout_policy`, `retry_policy`
//!
//! Module IDs: M012-M013 (C02), M015-M019 (C03).
//! Executor MUST NOT certify its own success (`UP_EXECUTOR_VERIFIER_SPLIT` / HLE-UP-001).

pub mod bounded;
pub mod local_runner;
pub mod phase_executor;
pub mod retry_policy;
pub mod state_machine;
pub mod status_transitions;
pub mod timeout_policy;
