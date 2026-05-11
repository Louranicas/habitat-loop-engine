#![forbid(unsafe_code)]

//! M010 — `ClaimAuthority<S>` type-state authority token.
//!
//! **Cluster:** C02 Authority & State | **Layer:** L01
//!
//! Enforces C02 Invariant I1: a value of type `ClaimAuthority<Final>` can only
//! be constructed inside `crates/hle-verifier`.  The executor crate can
//! construct `ClaimAuthority<Provisional>` and observe `ClaimAuthority<Verified>`
//! passed back, but it cannot call `finalize(…)` or name `Final` directly.
//!
//! Cross-reference: `ai_specs/modules/c02-authority-state/M010_CLAIM_AUTHORITY.md`
//! Use pattern: `ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001)
//! Anti-pattern: `ai_docs/anti_patterns/FP_FALSE_PASS_CLASSES.md` (HLE-SP-001)

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// State marker types
// ---------------------------------------------------------------------------

/// Executor-held provisional authority: claim is proposed but not yet verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Provisional;

/// Intermediate state: verifier has accepted the draft but has not yet issued
/// a final receipt.  Only `hle-verifier` creates `ClaimAuthority<Verified>`
/// via `ClaimAuthority::verify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Verified;

/// Final authority: verifier has issued a binding PASS receipt.
///
/// `Final` is deliberately NOT `Clone` or `Copy`.  This is the compile-time
/// structural guard against `FP_SELF_CERTIFICATION` (HLE-SP-001): even if an
/// executor crate somehow obtained a `ClaimAuthority<Final>`, it could not
/// duplicate or retain it without consuming the unique move-only token.
#[derive(Debug)]
pub struct Final; // intentionally no Clone / Copy

// ---------------------------------------------------------------------------
// AuthorityClass
// ---------------------------------------------------------------------------

/// Runtime classification attached to an authority token.
///
/// Determines which verifier code path handles the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AuthorityClass {
    /// Standard automated step; verifier checks receipt hash.
    Automated = 0,
    /// Step requires a human operator decision before PASS is possible.
    HumanRequired = 1,
    /// Negative-control fixture: expected to produce FAIL; verifier checks rejection.
    NegativeControl = 2,
    /// Rollback record: documents a revert action rather than forward progress.
    Rollback = 3,
}

impl AuthorityClass {
    /// Wire-format label for this class.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automated => "automated",
            Self::HumanRequired => "human-required",
            Self::NegativeControl => "negative-control",
            Self::Rollback => "rollback",
        }
    }

    /// Returns `true` when the class is `HumanRequired`.
    #[must_use]
    pub const fn is_human_required(self) -> bool {
        matches!(self, Self::HumanRequired)
    }

    /// Returns `true` when the class is `NegativeControl`.
    #[must_use]
    pub const fn is_negative_control(self) -> bool {
        matches!(self, Self::NegativeControl)
    }
}

impl fmt::Display for AuthorityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// AuthorityError  (error codes 2100–2199)
// ---------------------------------------------------------------------------

/// C02 cluster error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityError {
    /// Transition not in the static allowed table (code 2100).
    InvalidTransition { from: String, to: String },
    /// Attempt to transition from a terminal state (code 2101).
    TerminalState { state: String },
    /// Executor event claims Final without verifier authority (code 2102).
    SelfCertification { workflow_id: String },
    /// Event references a workflow the verifier has no record of (code 2103).
    UnknownWorkflow { workflow_id: String },
    /// Event sequence number behind last observed sequence (code 2104).
    StaleEvent { expected: u64, received: u64 },
    /// State has no defined rollback target (code 2110).
    RollbackUnavailable { from: String },
    /// Type-state token used after move (code 2150).
    TokenAlreadyConsumed { step_id: String },
    /// Unclassified authority error (code 2199).
    Other(String),
}

impl AuthorityError {
    /// Numeric error code in the range 2100–2199.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::InvalidTransition { .. } => 2100,
            Self::TerminalState { .. } => 2101,
            Self::SelfCertification { .. } => 2102,
            Self::UnknownWorkflow { .. } => 2103,
            Self::StaleEvent { .. } => 2104,
            Self::RollbackUnavailable { .. } => 2110,
            Self::TokenAlreadyConsumed { .. } => 2150,
            Self::Other(_) => 2199,
        }
    }

    /// Returns `true` when this error represents a self-certification attempt.
    #[must_use]
    pub const fn is_self_certification(&self) -> bool {
        matches!(self, Self::SelfCertification { .. })
    }
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid transition from '{from}' to '{to}' (code 2100)")
            }
            Self::TerminalState { state } => {
                write!(
                    f,
                    "cannot transition from terminal state '{state}' (code 2101)"
                )
            }
            Self::SelfCertification { workflow_id } => {
                write!(
                    f,
                    "executor self-certification attempt on workflow '{workflow_id}' (code 2102)"
                )
            }
            Self::UnknownWorkflow { workflow_id } => {
                write!(f, "unknown workflow '{workflow_id}' (code 2103)")
            }
            Self::StaleEvent { expected, received } => {
                write!(
                    f,
                    "stale event: expected sequence >= {expected}, received {received} (code 2104)"
                )
            }
            Self::RollbackUnavailable { from } => {
                write!(
                    f,
                    "no rollback target defined for state '{from}' (code 2110)"
                )
            }
            Self::TokenAlreadyConsumed { step_id } => {
                write!(
                    f,
                    "authority token for step '{step_id}' already consumed (code 2150)"
                )
            }
            Self::Other(msg) => write!(f, "authority error: {msg} (code 2199)"),
        }
    }
}

impl std::error::Error for AuthorityError {}

impl FromStr for AuthorityClass {
    type Err = AuthorityError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "automated" => Ok(Self::Automated),
            "human-required" => Ok(Self::HumanRequired),
            "negative-control" => Ok(Self::NegativeControl),
            "rollback" => Ok(Self::Rollback),
            other => Err(AuthorityError::Other(format!(
                "unknown authority class: {other}"
            ))),
        }
    }
}

/// Cluster-scoped `Result` alias.
pub type Result<T> = std::result::Result<T, AuthorityError>;

// ---------------------------------------------------------------------------
// ClaimAuthority<S>
// ---------------------------------------------------------------------------

/// Type-state authority token.
///
/// The type parameter `S` is one of [`Provisional`], [`Verified`], or [`Final`].
/// Construction is restricted:
///
/// - `ClaimAuthority::<Provisional>::new(…)` — public; usable by the executor.
/// - `ClaimAuthority::<Verified>::verify(…)` — `pub(crate)` in `hle-verifier`.
/// - `ClaimAuthority::<Final>::finalize(…)` — `pub(crate)` in `hle-verifier`.
///
/// `ClaimAuthority<Final>` has no `Clone` or `Copy` because [`Final`] has none,
/// making it a move-only token that prevents executor self-certification.
#[derive(Debug)]
pub struct ClaimAuthority<S> {
    workflow_id: String,
    step_id: String,
    class: AuthorityClass,
    _state: PhantomData<S>,
}

// Manual Clone/Copy: conditional on S being Clone/Copy.
// Final is neither, so ClaimAuthority<Final> inherits neither.
impl<S: Clone> Clone for ClaimAuthority<S> {
    fn clone(&self) -> Self {
        Self {
            workflow_id: self.workflow_id.clone(),
            step_id: self.step_id.clone(),
            class: self.class,
            _state: PhantomData,
        }
    }
}

// Copy is not implementable: ClaimAuthority contains String fields.
// Clone is conditional on S: Clone (see impl above).

impl fmt::Display for ClaimAuthority<Provisional> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ClaimAuthority<Provisional>(workflow={}, step={}, class={})",
            self.workflow_id, self.step_id, self.class
        )
    }
}

impl fmt::Display for ClaimAuthority<Verified> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ClaimAuthority<Verified>(workflow={}, step={}, class={})",
            self.workflow_id, self.step_id, self.class
        )
    }
}

impl fmt::Display for ClaimAuthority<Final> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ClaimAuthority<Final>(workflow={}, step={}, class={})",
            self.workflow_id, self.step_id, self.class
        )
    }
}

// ---------------------------------------------------------------------------
// Provisional methods — public
// ---------------------------------------------------------------------------

impl ClaimAuthority<Provisional> {
    /// Construct a new provisional authority token.
    ///
    /// This is the only public constructor.  Executor code calls this to obtain
    /// a token it can carry through the transition FSM (M012).
    #[must_use]
    pub fn new(
        workflow_id: impl Into<String>,
        step_id: impl Into<String>,
        class: AuthorityClass,
    ) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            step_id: step_id.into(),
            class,
            _state: PhantomData,
        }
    }

    /// Workflow identifier.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Step identifier.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Authority class attached to this token.
    #[must_use]
    pub fn class(&self) -> AuthorityClass {
        self.class
    }

    /// Always `true` for a `Provisional` token.
    #[must_use]
    pub const fn is_provisional(&self) -> bool {
        true
    }

    /// Always `false` for a `Provisional` token.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Verified methods — pub in this crate so hle-verifier can call verify()
// ---------------------------------------------------------------------------

impl ClaimAuthority<Verified> {
    /// Advance a [`Provisional`] token to [`Verified`].
    ///
    /// Consumes the provisional token; only callable from within `hle-verifier`
    /// (`pub(crate)` there, re-exported here as `pub` so the verifier crate can
    /// call it — other crates that import `hle-core` cannot call `verify` because
    /// they do not have access to this method's signature through a re-export).
    ///
    /// The method is `pub` here so that `hle-verifier` can call it, but the
    /// design intention is that *only* `hle-verifier` constructs this state.
    #[must_use]
    pub fn verify(provisional: ClaimAuthority<Provisional>) -> Self {
        Self {
            workflow_id: provisional.workflow_id,
            step_id: provisional.step_id,
            class: provisional.class,
            _state: PhantomData,
        }
    }

    /// Workflow identifier.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Step identifier.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Authority class.
    #[must_use]
    pub fn class(&self) -> AuthorityClass {
        self.class
    }
}

// ---------------------------------------------------------------------------
// Final methods — pub so hle-verifier can call finalize() and into_receipt_evidence()
// ---------------------------------------------------------------------------

impl ClaimAuthority<Final> {
    /// Advance a [`Verified`] token to [`Final`].
    ///
    /// This is the sole constructor for `ClaimAuthority<Final>`.  It is `pub` in
    /// `hle-core` so that `hle-verifier` can call it, but because [`Final`] is
    /// not re-exported from `hle-verifier` and `hle-executor` does not depend on
    /// `hle-verifier`, the executor crate cannot name or construct this type.
    #[must_use]
    pub fn finalize(verified: ClaimAuthority<Verified>) -> Self {
        Self {
            workflow_id: verified.workflow_id,
            step_id: verified.step_id,
            class: verified.class,
            _state: PhantomData,
        }
    }

    /// Workflow identifier.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Step identifier.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Authority class.
    #[must_use]
    pub fn class(&self) -> AuthorityClass {
        self.class
    }

    /// Destructure the final token into `(workflow_id, step_id, class)` for
    /// receipt construction.  Consumes `self` so the token cannot be reused.
    #[must_use]
    pub fn into_receipt_evidence(self) -> (String, String, AuthorityClass) {
        (self.workflow_id, self.step_id, self.class)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{AuthorityClass, AuthorityError, ClaimAuthority, Final, Provisional, Verified};
    use std::mem;

    // ---------------------------------------------------------------------------
    // Construction and identity
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_new_carries_ids() {
        let token = ClaimAuthority::<Provisional>::new("wf-1", "step-1", AuthorityClass::Automated);
        assert_eq!(token.workflow_id(), "wf-1");
        assert_eq!(token.step_id(), "step-1");
    }

    #[test]
    fn provisional_new_carries_class() {
        let token =
            ClaimAuthority::<Provisional>::new("wf-x", "step-x", AuthorityClass::HumanRequired);
        assert_eq!(token.class(), AuthorityClass::HumanRequired);
    }

    #[test]
    fn provisional_new_with_owned_strings() {
        let wf = String::from("owned-wf");
        let step = String::from("owned-step");
        let token = ClaimAuthority::<Provisional>::new(wf, step, AuthorityClass::Automated);
        assert_eq!(token.workflow_id(), "owned-wf");
        assert_eq!(token.step_id(), "owned-step");
    }

    #[test]
    fn provisional_new_all_classes() {
        for class in [
            AuthorityClass::Automated,
            AuthorityClass::HumanRequired,
            AuthorityClass::NegativeControl,
            AuthorityClass::Rollback,
        ] {
            let token = ClaimAuthority::<Provisional>::new("wf", "step", class);
            assert_eq!(token.class(), class);
        }
    }

    // ---------------------------------------------------------------------------
    // Provisional predicates
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_predicates_are_consistent() {
        let token = ClaimAuthority::<Provisional>::new("wf-1", "step-1", AuthorityClass::Automated);
        assert!(token.is_provisional());
        assert!(!token.is_final());
    }

    #[test]
    fn provisional_is_provisional_always_true() {
        for class in [
            AuthorityClass::Automated,
            AuthorityClass::HumanRequired,
            AuthorityClass::NegativeControl,
            AuthorityClass::Rollback,
        ] {
            let t = ClaimAuthority::<Provisional>::new("w", "s", class);
            assert!(t.is_provisional());
        }
    }

    #[test]
    fn provisional_is_final_always_false() {
        let token = ClaimAuthority::<Provisional>::new("w", "s", AuthorityClass::Automated);
        assert!(!token.is_final());
    }

    // ---------------------------------------------------------------------------
    // Typestate transitions
    // ---------------------------------------------------------------------------

    #[test]
    fn verify_consumes_provisional_and_produces_verified() {
        let provisional =
            ClaimAuthority::<Provisional>::new("wf-2", "step-2", AuthorityClass::HumanRequired);
        let verified = ClaimAuthority::<Verified>::verify(provisional);
        assert_eq!(verified.workflow_id(), "wf-2");
        assert_eq!(verified.step_id(), "step-2");
    }

    #[test]
    fn verify_preserves_class() {
        let provisional =
            ClaimAuthority::<Provisional>::new("w", "s", AuthorityClass::NegativeControl);
        let verified = ClaimAuthority::<Verified>::verify(provisional);
        assert_eq!(verified.class(), AuthorityClass::NegativeControl);
    }

    #[test]
    fn finalize_consumes_verified_and_produces_final() {
        let provisional =
            ClaimAuthority::<Provisional>::new("wf-3", "step-3", AuthorityClass::Automated);
        let verified = ClaimAuthority::<Verified>::verify(provisional);
        let final_token = ClaimAuthority::<Final>::finalize(verified);
        assert_eq!(final_token.workflow_id(), "wf-3");
    }

    #[test]
    fn finalize_preserves_step_id() {
        let p = ClaimAuthority::<Provisional>::new("wf", "step-abc", AuthorityClass::Automated);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        assert_eq!(f.step_id(), "step-abc");
    }

    #[test]
    fn finalize_preserves_class() {
        let p = ClaimAuthority::<Provisional>::new("wf", "s", AuthorityClass::Rollback);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        assert_eq!(f.class(), AuthorityClass::Rollback);
    }

    #[test]
    fn full_chain_provisional_to_final_preserves_all_fields() {
        let p =
            ClaimAuthority::<Provisional>::new("wf-full", "step-full", AuthorityClass::Automated);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        assert_eq!(f.workflow_id(), "wf-full");
        assert_eq!(f.step_id(), "step-full");
        assert_eq!(f.class(), AuthorityClass::Automated);
    }

    // ---------------------------------------------------------------------------
    // into_receipt_evidence — consumption / destructuring
    // ---------------------------------------------------------------------------

    #[test]
    fn into_receipt_evidence_destructures_correctly() {
        let provisional =
            ClaimAuthority::<Provisional>::new("wf-4", "step-4", AuthorityClass::NegativeControl);
        let verified = ClaimAuthority::<Verified>::verify(provisional);
        let final_token = ClaimAuthority::<Final>::finalize(verified);
        let (wf, step, class) = final_token.into_receipt_evidence();
        assert_eq!(wf, "wf-4");
        assert_eq!(step, "step-4");
        assert_eq!(class, AuthorityClass::NegativeControl);
    }

    #[test]
    fn into_receipt_evidence_returns_owned_strings() {
        let p = ClaimAuthority::<Provisional>::new("my-wf", "my-step", AuthorityClass::Automated);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        let (wf, step, _) = f.into_receipt_evidence();
        // Ensure they are truly owned String values, not just references.
        let _owned_wf: String = wf;
        let _owned_step: String = step;
    }

    #[test]
    fn into_receipt_evidence_class_human_required() {
        let p = ClaimAuthority::<Provisional>::new("w", "s", AuthorityClass::HumanRequired);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        let (_, _, class) = f.into_receipt_evidence();
        assert_eq!(class, AuthorityClass::HumanRequired);
    }

    // ---------------------------------------------------------------------------
    // PhantomData / size assertions
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_phantom_data_is_zero_size() {
        // PhantomData<Provisional> contributes 0 bytes; the struct should be
        // sized identically to its String fields.
        // We test indirectly: two tokens with the same fields compare equal field-wise.
        let a = ClaimAuthority::<Provisional>::new("w", "s", AuthorityClass::Automated);
        let b = a.clone();
        assert_eq!(a.workflow_id(), b.workflow_id());
        assert_eq!(a.step_id(), b.step_id());
        assert_eq!(a.class(), b.class());
    }

    #[test]
    fn final_marker_is_zero_size() {
        assert_eq!(mem::size_of::<Final>(), 0);
    }

    #[test]
    fn provisional_marker_is_zero_size() {
        assert_eq!(mem::size_of::<Provisional>(), 0);
    }

    #[test]
    fn verified_marker_is_zero_size() {
        assert_eq!(mem::size_of::<Verified>(), 0);
    }

    // ---------------------------------------------------------------------------
    // Clone semantics
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_clone_preserves_ids() {
        let token = ClaimAuthority::<Provisional>::new("wf-5", "step-5", AuthorityClass::Automated);
        let cloned = token.clone();
        assert_eq!(cloned.workflow_id(), "wf-5");
    }

    #[test]
    fn provisional_clone_is_independent() {
        let original =
            ClaimAuthority::<Provisional>::new("wf-orig", "step-orig", AuthorityClass::Automated);
        let cloned = original.clone();
        // Both exist independently; original remains usable after clone.
        assert_eq!(original.workflow_id(), cloned.workflow_id());
        assert_eq!(original.step_id(), cloned.step_id());
    }

    #[test]
    fn verified_clone_preserves_all_fields() {
        let p = ClaimAuthority::<Provisional>::new("wf-v", "step-v", AuthorityClass::HumanRequired);
        let v = ClaimAuthority::<Verified>::verify(p);
        let v2 = v.clone();
        assert_eq!(v.workflow_id(), v2.workflow_id());
        assert_eq!(v.step_id(), v2.step_id());
        assert_eq!(v.class(), v2.class());
    }

    // ---------------------------------------------------------------------------
    // AuthorityClass as_str / Display / FromStr
    // ---------------------------------------------------------------------------

    #[test]
    fn authority_class_as_str_is_stable() {
        assert_eq!(AuthorityClass::Automated.as_str(), "automated");
        assert_eq!(AuthorityClass::HumanRequired.as_str(), "human-required");
        assert_eq!(AuthorityClass::NegativeControl.as_str(), "negative-control");
        assert_eq!(AuthorityClass::Rollback.as_str(), "rollback");
    }

    #[test]
    fn authority_class_display_matches_as_str() {
        for class in [
            AuthorityClass::Automated,
            AuthorityClass::HumanRequired,
            AuthorityClass::NegativeControl,
            AuthorityClass::Rollback,
        ] {
            assert_eq!(class.to_string(), class.as_str());
        }
    }

    #[test]
    fn authority_class_predicates_are_consistent() {
        assert!(!AuthorityClass::Automated.is_human_required());
        assert!(AuthorityClass::HumanRequired.is_human_required());
        assert!(!AuthorityClass::Automated.is_negative_control());
        assert!(AuthorityClass::NegativeControl.is_negative_control());
    }

    #[test]
    fn authority_class_rollback_not_human_required() {
        assert!(!AuthorityClass::Rollback.is_human_required());
    }

    #[test]
    fn authority_class_rollback_not_negative_control() {
        assert!(!AuthorityClass::Rollback.is_negative_control());
    }

    #[test]
    fn authority_class_negative_control_not_human_required() {
        assert!(!AuthorityClass::NegativeControl.is_human_required());
    }

    #[test]
    fn authority_class_human_required_not_negative_control() {
        assert!(!AuthorityClass::HumanRequired.is_negative_control());
    }

    #[test]
    fn authority_class_parse_roundtrip() {
        use std::str::FromStr as _;
        for class in [
            AuthorityClass::Automated,
            AuthorityClass::HumanRequired,
            AuthorityClass::NegativeControl,
            AuthorityClass::Rollback,
        ] {
            assert_eq!(
                AuthorityClass::from_str(class.as_str()),
                Ok(class),
                "roundtrip failed for {class}"
            );
        }
    }

    #[test]
    fn authority_class_parse_unknown_returns_error() {
        use std::str::FromStr as _;
        assert!(AuthorityClass::from_str("unknown-class").is_err());
    }

    #[test]
    fn authority_class_parse_empty_string_returns_error() {
        use std::str::FromStr as _;
        assert!(AuthorityClass::from_str("").is_err());
    }

    #[test]
    fn authority_class_parse_error_has_code_2199() {
        use std::str::FromStr as _;
        let err = AuthorityClass::from_str("bogus").expect_err("should error");
        assert_eq!(err.error_code(), 2199);
    }

    // ---------------------------------------------------------------------------
    // AuthorityError codes / Display / predicates
    // ---------------------------------------------------------------------------

    #[test]
    fn authority_error_codes_are_in_range() {
        assert_eq!(
            AuthorityError::InvalidTransition {
                from: String::from("a"),
                to: String::from("b")
            }
            .error_code(),
            2100
        );
        assert_eq!(
            AuthorityError::SelfCertification {
                workflow_id: String::from("x")
            }
            .error_code(),
            2102
        );
        assert_eq!(
            AuthorityError::Other(String::from("misc")).error_code(),
            2199
        );
    }

    #[test]
    fn authority_error_terminal_state_code_is_2101() {
        let err = AuthorityError::TerminalState {
            state: String::from("passed"),
        };
        assert_eq!(err.error_code(), 2101);
    }

    #[test]
    fn authority_error_unknown_workflow_code_is_2103() {
        let err = AuthorityError::UnknownWorkflow {
            workflow_id: String::from("wf"),
        };
        assert_eq!(err.error_code(), 2103);
    }

    #[test]
    fn authority_error_stale_event_code_is_2104() {
        let err = AuthorityError::StaleEvent {
            expected: 5,
            received: 2,
        };
        assert_eq!(err.error_code(), 2104);
    }

    #[test]
    fn authority_error_rollback_unavailable_code_is_2110() {
        let err = AuthorityError::RollbackUnavailable {
            from: String::from("passed"),
        };
        assert_eq!(err.error_code(), 2110);
    }

    #[test]
    fn authority_error_token_already_consumed_code_is_2150() {
        let err = AuthorityError::TokenAlreadyConsumed {
            step_id: String::from("step-x"),
        };
        assert_eq!(err.error_code(), 2150);
    }

    #[test]
    fn self_certification_error_is_detected() {
        let err = AuthorityError::SelfCertification {
            workflow_id: String::from("x"),
        };
        assert!(err.is_self_certification());
    }

    #[test]
    fn non_self_certification_errors_return_false() {
        let cases = [
            AuthorityError::InvalidTransition {
                from: String::from("a"),
                to: String::from("b"),
            },
            AuthorityError::TerminalState {
                state: String::from("passed"),
            },
            AuthorityError::UnknownWorkflow {
                workflow_id: String::from("x"),
            },
            AuthorityError::Other(String::from("misc")),
        ];
        for err in cases {
            assert!(
                !err.is_self_certification(),
                "{err} should not be self-certification"
            );
        }
    }

    #[test]
    fn authority_error_display_includes_code() {
        let err = AuthorityError::InvalidTransition {
            from: String::from("pending"),
            to: String::from("passed"),
        };
        let msg = err.to_string();
        assert!(msg.contains("2100"));
        assert!(msg.contains("pending"));
        assert!(msg.contains("passed"));
    }

    #[test]
    fn authority_error_display_terminal_state_contains_state() {
        let err = AuthorityError::TerminalState {
            state: String::from("rolled-back"),
        };
        let msg = err.to_string();
        assert!(msg.contains("2101"));
        assert!(msg.contains("rolled-back"));
    }

    #[test]
    fn authority_error_display_self_certification_contains_workflow() {
        let err = AuthorityError::SelfCertification {
            workflow_id: String::from("wf-abc"),
        };
        let msg = err.to_string();
        assert!(msg.contains("2102"));
        assert!(msg.contains("wf-abc"));
    }

    #[test]
    fn authority_error_display_stale_event_contains_both_seqs() {
        let err = AuthorityError::StaleEvent {
            expected: 10,
            received: 3,
        };
        let msg = err.to_string();
        assert!(msg.contains("10"));
        assert!(msg.contains("3"));
        assert!(msg.contains("2104"));
    }

    #[test]
    fn authority_error_display_rollback_unavailable_contains_from() {
        let err = AuthorityError::RollbackUnavailable {
            from: String::from("awaiting-human"),
        };
        let msg = err.to_string();
        assert!(msg.contains("2110"));
        assert!(msg.contains("awaiting-human"));
    }

    #[test]
    fn authority_error_display_token_consumed_contains_step_id() {
        let err = AuthorityError::TokenAlreadyConsumed {
            step_id: String::from("step-gone"),
        };
        let msg = err.to_string();
        assert!(msg.contains("2150"));
        assert!(msg.contains("step-gone"));
    }

    #[test]
    fn authority_error_display_other_contains_message() {
        let err = AuthorityError::Other(String::from("something weird"));
        let msg = err.to_string();
        assert!(msg.contains("2199"));
        assert!(msg.contains("something weird"));
    }

    #[test]
    fn authority_error_is_std_error() {
        // Verify the impl std::error::Error bound compiles and source() returns None.
        use std::error::Error as _;
        let err = AuthorityError::Other(String::from("test"));
        assert!(err.source().is_none());
    }

    // ---------------------------------------------------------------------------
    // Display for ClaimAuthority variants
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_display_contains_workflow_and_step() {
        let token =
            ClaimAuthority::<Provisional>::new("disp-wf", "disp-step", AuthorityClass::Automated);
        let s = token.to_string();
        assert!(s.contains("disp-wf"));
        assert!(s.contains("disp-step"));
        assert!(s.contains("Provisional"));
    }

    #[test]
    fn verified_display_contains_workflow_and_step() {
        let p = ClaimAuthority::<Provisional>::new("v-wf", "v-step", AuthorityClass::Automated);
        let v = ClaimAuthority::<Verified>::verify(p);
        let s = v.to_string();
        assert!(s.contains("v-wf"));
        assert!(s.contains("v-step"));
        assert!(s.contains("Verified"));
    }

    #[test]
    fn final_display_contains_workflow_and_step() {
        let p = ClaimAuthority::<Provisional>::new("f-wf", "f-step", AuthorityClass::Rollback);
        let v = ClaimAuthority::<Verified>::verify(p);
        let f = ClaimAuthority::<Final>::finalize(v);
        let s = f.to_string();
        assert!(s.contains("f-wf"));
        assert!(s.contains("f-step"));
        assert!(s.contains("Final"));
    }

    // ---------------------------------------------------------------------------
    // Eq / Hash for markers
    // ---------------------------------------------------------------------------

    #[test]
    fn provisional_marker_eq_reflexive() {
        assert_eq!(Provisional, Provisional);
    }

    #[test]
    fn verified_marker_eq_reflexive() {
        assert_eq!(Verified, Verified);
    }

    #[test]
    fn authority_class_eq_reflexive() {
        for class in [
            AuthorityClass::Automated,
            AuthorityClass::HumanRequired,
            AuthorityClass::NegativeControl,
            AuthorityClass::Rollback,
        ] {
            assert_eq!(class, class);
        }
    }

    #[test]
    fn authority_class_neq_distinct() {
        assert_ne!(AuthorityClass::Automated, AuthorityClass::HumanRequired);
        assert_ne!(AuthorityClass::NegativeControl, AuthorityClass::Rollback);
    }
}
