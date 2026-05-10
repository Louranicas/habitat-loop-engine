# UP_RUNBOOK_AWAITING_HUMAN

Status: scaffold use-pattern contract. Awaiting-human runtime behavior is deferred until `begin M0`; the state machine and review contract are scaffold-authoritative.

Predicate ID: `HLE-UP-005`

## Intent

A Habitat workflow loop must be able to stop safely when it reaches a human decision point. It should preserve context, blockers, candidate actions, and verifier evidence without pretending the workflow completed.

## Awaiting-human states

- `ready_for_review`: all scaffold evidence is gathered and a human decision is needed.
- `blocked_on_input`: required parameters, authorization, or scope are missing.
- `blocked_on_verifier`: verifier evidence failed or is incomplete.
- `waiver_requested`: an explicit human waiver is needed before proceeding.
- `m0_waiting`: scaffold is complete but runtime implementation remains unauthorized.

## Scaffold-time evidence

- `runbooks/m0-authorization-boundary.md` names the phrase gate for runtime work.
- `runbooks/scaffold-verification.md` explains how to rerun the quality gate.
- `schematics/runbook-awaiting-human-fsm.md` renders the state transitions.
- `.deployment-work/status/scaffold-status.json` records the current state without claiming deployment.

## Future M0 rule

A future executor may pause at these states and write receipts, but it must not auto-waive, auto-approve, or fabricate human authorization.

## Review checklist

- Is the missing human decision named explicitly?
- Does the receipt preserve enough context to resume?
- Is the workflow blocked rather than marked PASS when authorization is absent?
- Are waiver and verifier failure distinct states?
