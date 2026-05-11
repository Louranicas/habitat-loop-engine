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
// Crate-level lint suppressions for C07 compile-safe stubs.
// These lints are pedantic style choices that conflict with the spec's
// explicit design decisions (e.g. `#[must_use]` on every Result-returning
// method is required by the spec regardless of whether the type is already
// must_use; doc_markdown for module codes like M040 is intentionally not
// backticked).
#![allow(clippy::double_must_use)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::return_self_not_must_use)]

//! `hle-bridge` — L5 dispatch adapters with read-only-by-default capability gating.
//!
//! Cluster ownership: C07 Dispatch Bridges (M040-M045).
//!
//! Every bridge defaults to `ReadOnly`. `LiveWrite` requires a sealed token type
//! constructible only via an explicit authorization receipt path. Bridges are
//! passive adapters and MUST NOT import `hle-executor`.

pub mod atuin_qi_bridge;
pub mod bridge_contract;
pub mod devops_v3_probe;
pub mod stcortex_anchor_bridge;
pub mod watcher_notice_writer;
pub mod zellij_dispatch;
