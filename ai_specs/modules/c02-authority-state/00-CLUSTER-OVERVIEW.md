# C02 Authority and State — Cluster Overview

> **Cluster:** C02_AUTHORITY_STATE | **Layers:** L01 / L03 / L04
> **Module Range:** M010–M014 | **Error Code Range:** 2100–2199
> **Synergy:** type-state authority + transition table prevent executor self-certification

---

## Purpose

C02 is the authority fence that separates "executor proposes" from "verifier decides". The cluster
enforces a compile-time boundary: no value of type `ClaimAuthority<Final>` can be constructed
outside the `hle-verifier` crate. The executor can only hold `ClaimAuthority<Provisional>` or
`ClaimAuthority<Verified>`. This makes `FP_SELF_CERTIFICATION` (HLE-SP-001) physically impossible
rather than merely discouraged by convention.

The five modules form a single causal chain:

1. **M010 claim_authority** — type-state token (L01) with `PhantomData<S>` state markers
2. **M011 workflow_state** — FSM enum extending `substrate_types::StepState` with full predicate set
3. **M012 state_machine** — executor-side transition driver that emits `ExecutorEvent` on every hop
4. **M013 status_transitions** — static allowed-transition table and rollback affordances
5. **M014 claim_authority_verifier** — adversarial L04 consumer; sole producer of `ClaimAuthority<Final>`

---

## File Map

```
crates/hle-core/src/
├── authority/
│   └── claim_authority.rs        # M010 — type-state authority token
└── state/
    └── workflow_state.rs         # M011 — workflow FSM enum + predicates

crates/hle-executor/src/
├── state_machine.rs              # M012 — transition driver + event emission
└── status_transitions.rs        # M013 — static transition table + rollback

crates/hle-verifier/src/
└── claim_authority_verifier.rs   # M014 — adversarial verifier; Final token issuer
```

---

## Dependency Graph (Internal — C02 only)

```
claim_authority.rs   (M010, L01)
  └── consumed by state_machine.rs (M012, L03) — executor holds Provisional/Verified
      └── emits ExecutorEvent
          └── consumed by claim_authority_verifier.rs (M014, L04)
              └── sole producer of ClaimAuthority<Final>

workflow_state.rs    (M011, L01)
  ├── consumed by state_machine.rs (M012, L03) — drives enum transitions
  └── consumed by status_transitions.rs (M013, L03) — transition table key type

status_transitions.rs (M013, L03)
  └── consumed by state_machine.rs (M012, L03) — guards every transition call
```

---

## Cross-Cluster Dependencies

| This module | Depends on | Crate |
|---|---|---|
| M010 claim_authority | `substrate_types::HleError` | substrate-types (M001) |
| M011 workflow_state | `substrate_types::StepState` | substrate-types (M001) |
| M012 state_machine | `substrate_verify::verify_step` | substrate-verify (M002) |
| M013 status_transitions | M011 `WorkflowState` | hle-core (C02 internal) |
| M014 claim_authority_verifier | M010 `ClaimAuthority`, M012 `ExecutorEvent` | hle-core + hle-executor |

**Downstream consumers of C02:**

| Consumer cluster | What it uses |
|---|---|
| C01 Evidence Integrity | M011 `WorkflowState` for claim state anchors |
| C03 Bounded Execution | M012 `ExecutorEvent` to gate phase transitions |
| C04 Anti-Pattern Intelligence | M014 verdicts feed `false_pass_auditor` |
| C06 Runbook Semantics | M011 `WorkflowState::AwaitingHuman` predicate |

**Note:** M012 also emits `ExecutorEvent` that C01's `receipt_sha_verifier` (M008 in renumbered
table / M004 in plan.toml) subscribes to for independent hash verification.

---

## Concurrency Table

| Module | Shared state | Sync strategy |
|---|---|---|
| M010 ClaimAuthority | Zero runtime state — token is a ZST wrapper | None needed |
| M011 WorkflowState | Copy enum — callers own their copies | Value semantics |
| M012 StateMachine | Owns current `WorkflowState`; single-owner FSM | Move-on-transition (consumes self) |
| M013 StatusTransitions | Static `const` array — no runtime allocation | Read-only, no sync |
| M014 ClaimAuthorityVerifier | Receives events over a channel; internal state behind `parking_lot::RwLock` | `parking_lot::RwLock` |

---

## Design Principles

1. **Compile-time authority fencing** — `ClaimAuthority<Final>` is `pub(crate)` in `hle-verifier`.
   The executor crate cannot name the type, let alone construct it.
2. **Move semantics over mutation** — `StateMachine::step` consumes `self` and returns
   `Result<(StateMachine, TransitionEffect), TransitionError>`. No `&mut self`, no internal
   mutation that silently changes observable state.
3. **Static transition table** — `ALLOWED_TRANSITIONS` is a `const` array evaluated at compile
   time. Invalid transitions are caught at `is_allowed` call sites, not discovered at runtime.
4. **Event-sourced verifier** — M014 does not call into the executor; it only reads
   `ExecutorEvent` values the executor emits. The verifier is a pure consumer.
5. **`StepState` supersession** — M011 `WorkflowState` re-exports and extends
   `substrate_types::StepState` with additional predicates; existing `substrate_types` code remains
   unchanged and continues compiling.
6. **`#[must_use]` everywhere** — every predicate, transition result, and token construction
   carries `#[must_use]` so callers cannot accidentally discard authority signals.
7. **Zero `unwrap` / `expect` / `panic`** — all fallible operations return `Result`; the static
   transition table eliminates the only class of runtime surprise (unknown transition).

---

## Error Strategy (codes 2100–2199)

| Code | Variant | Source Module | Meaning |
|---|---|---|---|
| 2100 | `AuthorityError::InvalidTransition` | M012, M013 | Transition not in allowed table |
| 2101 | `AuthorityError::TerminalState` | M012 | Attempt to transition from a terminal state |
| 2102 | `AuthorityError::SelfCertification` | M014 | Executor event claims Final without verifier authority |
| 2103 | `AuthorityError::UnknownWorkflow` | M014 | Event references a workflow the verifier has no record of |
| 2104 | `AuthorityError::StaleEvent` | M014 | Event sequence number behind last observed sequence |
| 2110 | `AuthorityError::RollbackUnavailable` | M013 | State has no defined rollback target |
| 2150 | `AuthorityError::TokenAlreadyConsumed` | M010 | Type-state token used after move |
| 2199 | `AuthorityError::Other(String)` | Any C02 | Unclassified authority error |

All C02 modules expose `type Result<T> = std::result::Result<T, AuthorityError>`.

---

## Quality Gate Results Template

```
cargo check --workspace --all-targets          [ ] 0 errors
cargo clippy --workspace -- -D warnings        [ ] 0 warnings
cargo clippy --workspace -- -W clippy::pedantic [ ] 0 warnings
cargo test --workspace --lib                   [ ] target: ≥ 200 tests (≥ 40 per module)
Zero-tolerance grep (unsafe/unwrap/expect/…)   [ ] 0 hits
Static transition table coverage              [ ] all 6 WorkflowState variants appear as from/to
ClaimAuthority<Final> reachability check       [ ] only hle-verifier crate can name the type
FP_SELF_CERTIFICATION negative control         [ ] executor-only binary cannot produce Final token
```

---

*C02 Authority and State Cluster Overview v1.0 | 2026-05-10*
