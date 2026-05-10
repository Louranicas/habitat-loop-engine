# Scaffold Operation Receipt

Created UTC: 2026-05-09T23:52:44Z
^Verdict: scaffold receipt registry only
^Manifest_sha256: see `/home/louranicas/claude-code-workspace/habitat-loop-engine/SHA256SUMS.txt`
^Framework_sha256: see `.deployment-work/receipts/scaffold-authorization-20260509T235244Z.md`

Split hash semantics:
- `^Manifest_sha256` anchors the scaffold manifest evidence (`SHA256SUMS.txt` hash in concrete receipts).
- `^Framework_sha256` anchors the source/framework provenance that authorized the scaffold.
- `^Source_sha256` is retained only as a legacy alias when an older receipt reader still expects it.
