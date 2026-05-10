#![forbid(unsafe_code)]

/// Emitter marker. This crate must not emit runtime receipts before M0.
#[must_use]
pub const fn emitter_boundary() -> &'static str {
    "scaffold-emitter-boundary-only"
}
