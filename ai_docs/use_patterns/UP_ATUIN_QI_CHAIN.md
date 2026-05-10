# UP_ATUIN_QI_CHAIN

Status: scaffold use-pattern contract. Atuin-QI is a provenance and recall aid, not a substitute for verifier receipts.

Predicate ID: `HLE-UP-004`

## Intent

The Loop Engine may use shell-history intelligence to reconstruct operator context, command provenance, and waiver history. That context can guide review, but it cannot independently certify a workflow PASS.

## Scaffold-time interpretation

- Atuin-derived command evidence may support receipt narratives.
- Command history must be reduced to bounded, relevant excerpts before entering a receipt.
- Secret-safe handling is mandatory: no API keys, tokens, or credentials may be copied into scaffold receipts.
- A Command-2 waiver can be recorded only when Luke explicitly grants it or when an approved supersession receipt exists.

## Future M0 integration boundary

A future Atuin adapter may emit typed evidence records such as command, timestamp, cwd, exit class, and redacted output hash. The verifier must still check these records against schemas and negative controls.

## Non-authority rule

A command appearing in shell history is not proof that it succeeded, nor proof that its side effects are acceptable. The receipt graph must include verifier evidence and artifact hashes.

## Review checklist

- Is Atuin evidence redacted and bounded?
- Is the difference between observed command and verified outcome explicit?
- Does any waiver cite an exact human-authorized phrase or supersession receipt?
- Are command excerpts tied to source hashes when they support claims?
