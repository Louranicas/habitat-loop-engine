#![forbid(unsafe_code)]

/// Scaffold status marker. This is not M0 runtime state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaffoldStatus {
    /// Scaffold files are present and verifiable.
    ScaffoldOnly,
}

/// Return the compile-time scaffold marker.
#[must_use]
pub const fn scaffold_status() -> ScaffoldStatus {
    ScaffoldStatus::ScaffoldOnly
}
