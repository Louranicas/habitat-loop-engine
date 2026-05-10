#![forbid(unsafe_code)]

/// Verifier authority marker for scaffold-only builds.
#[must_use]
pub const fn verifier_authority() -> &'static str {
    "scaffold-verifier-authority-only"
}
