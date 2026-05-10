# UP_RECEIPT_GRAPH

Status: scaffold use-pattern contract. Durable receipt graph implementation is deferred until `begin M0`, but graph shape and review semantics are scaffold-authoritative.

Predicate ID: `HLE-UP-002`

## Intent

Every important workflow claim must become a typed, hash-addressed receipt node. Receipts do not merely narrate progress; they define what evidence exists, what authority it has, and which earlier claims it supersedes or blocks.

## Minimal graph fields

- `receipt_id`: stable identifier for the receipt node.
- `claim_id`: identifier of the claim being asserted or verified.
- `claim_class`: scaffold, verifier, blocker, waiver, negative-control, or future runtime class.
- `source_artifact_path`: path to the artifact under review.
- `source_sha256`: hash of the artifact at claim time.
- `parent_sha256`: hash edge to the parent receipt or manifest when applicable.
- `verdict`: PASS, BLOCKED, WAIVED, SUPERSEDED, or INFORMATIONAL.
- `counter_evidence_locator`: where a reviewer should look for contradicting evidence.

## Scaffold-time evidence

- `.deployment-work/receipts/scaffold-authorization-*.md` anchors the current scaffold authority.
- `SHA256SUMS.txt` anchors the repository file set.
- `schematics/receipt-graph.md` documents the intended graph topology.
- `scripts/verify-receipt-graph.sh` checks presence and required anchors without claiming runtime completeness.

## Future M0 rule

M0 may append receipt nodes, but it must not mutate historical receipt contents in place. Corrections are represented by superseding nodes with explicit parent edges.

## Review checklist

- Does each PASS have an evidence path and hash?
- Is a waiver distinguishable from a verifier PASS?
- Can a future reader reconstruct why a claim was believed at the time?
- Are stale or superseded claims still auditable?
