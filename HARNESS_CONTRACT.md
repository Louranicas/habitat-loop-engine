# Harness Contract

Status: scaffold contract only.

Future S13 execution requires:

- `vault/CONVENTIONS.md` complete;
- JSON schemas under `schemas/`;
- md-to-jsonl emitter;
- jsonl-to-md verifier;
- negative controls proving false passes fail;
- anchored receipts with `^Verdict`, split hash anchors (`^Manifest_sha256` for scaffold manifest evidence and `^Framework_sha256` for source/framework provenance), and `^Counter_evidence_locator`.

`^Source_sha256` is a legacy compatibility alias only. New scaffold receipts must use the split hash anchors so CI/Watcher readers can distinguish manifest integrity from framework provenance.

Scaffold gate only verifies these files exist and are internally linked.
