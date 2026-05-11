# M0 Implementation Authorization Receipt

Generated UTC: 2026-05-11T07:00:00Z
User phrase received: `start coding` (Luke, 2026-05-11, anchored against AskUserQuestion scope answer selecting "Begin M0 implementation" with C01 Evidence Integrity as the first cluster pass)

^Verdict: M0_IMPLEMENTATION_AUTHORIZED_C01_ONLY
^Manifest_sha256: 23ac68cd5aae9a639bf412eb60afa029f0941a6a478e103f3200d3609d289fe9
^Framework_sha256: a26a628055bdc52c51a5655a23a98b93cdcaa78bdcf91567476ef06f2b9c79a1
^Counter_evidence_locator: FALSE_100_TRAPS.md; .deployment-work/status/quality-gate-scaffold-post-expansion.json; .deployment-work/status/quality-gate-m0-post-expansion.json
^M0_authorized: true (now extended to C01 production logic)
^Live_integrations_authorized: false
^Cron_daemons_authorized: false
^Parent_authorization: scaffold-expansion-20260511T060000Z.md (start coding — scaffold expansion)
^Source_sha256: a26a628055bdc52c51a5655a23a98b93cdcaa78bdcf91567476ef06f2b9c79a1

## Scope

M0 implementation of cluster C01 Evidence Integrity ONLY. Subsequent clusters (C02-C09) remain at compile-safe stub depth pending separate explicit authorization per cluster pass.

### Authorized changes

1. **Add `sha2 = "0.10"` external crate dependency to `hle-core`** — first external dep in the workspace. Justification: real SHA-256 is load-bearing for receipt integrity; hand-rolling is unnecessary supply-chain risk avoidance and the crate is well-audited.
2. **M005 `receipt_hash`** — replace XOR-fold stub with real `sha2::Sha256::digest(canonical_bytes)`. Add known-vector tests using documented SHA-256 test vectors.
3. **M006 `claims_store`** — review and harden the claim graph state machine (provisional → verified → final) if any stub-isms remain.
4. **M007 `receipts_store`** — keep abstract Pool injection (real persistence deferred to C05 M025 cluster pass).
5. **M008 `receipt_sha_verifier`** — verify M005 SHA-256 recompute matches when invoked through M008.
6. **M009 `final_claim_evaluator`** — review typestate transitions and harden VerifierToken construction guards.

### Out of scope

- C02-C08 module implementations (stay at stub depth)
- C05 real SQLite Pool (deferred)
- C07 real HTTP bridge clients (deferred)
- Live Habitat write integrations, cron, systemd, unbounded daemons (forbidden under any current phrase)
- Production deployment claims

## Acceptance gates

- `cargo test --workspace --all-targets` continues to PASS (1,263 baseline tests + any new C01 vector tests)
- `cargo clippy --workspace --all-targets -- -D warnings` continues to PASS
- `cargo fmt --check` clean
- `scripts/quality-gate.sh --scaffold --json` verdict PASS
- `scripts/quality-gate.sh --m0 --json` verdict PASS
- `sha256sum -c SHA256SUMS.txt` clean (after manifest refresh)
- M005 known-vector test: `SHA256(canonical_bytes("workflow", "s1", "PASS", "", ""))` matches documented hex output

## Cross-references

- Parent: `.deployment-work/receipts/scaffold-expansion-20260511T060000Z.md`
- Spec: `ai_specs/modules/c01-evidence-integrity/M005_RECEIPT_HASH.md`
- Master Index: `MASTER_INDEX.md`
- Authorization phrases: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`

---

*Receipt v1.0 | filed by Claude on 2026-05-11 under `start coding` authorization scoped to C01 M0 implementation*
