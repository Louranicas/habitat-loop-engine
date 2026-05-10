# AP28_COMPOSITIONAL_INTEGRITY_DRIFT

Status: scaffold detector predicate. Runtime detector implementation is explicitly deferred until M0 authorization.

Predicate ID: `HLE-SP-001`

## Detector predicate

A future detector for `AP28_COMPOSITIONAL_INTEGRITY_DRIFT` must identify evidence of compositional integrity drift: declared scaffold surfaces stop agreeing across plan, map, docs, and verifier receipts in source, configuration, verifier receipts, or scaffold review artifacts. The detector predicate is reviewable at scaffold time even though executable runtime detection is not implemented here.

## Negative control

A compliant example must not fire the detector when the same concern is handled by an explicit boundary, bounded contract, documented verifier gate, or typed/isolated implementation path. Negative controls are required before any future runtime detector can claim PASS.

## Remediation expectation

A remediation receipt must name the affected file, describe the semantic correction, and point to verifier evidence. Count-only evidence such as file presence or registry size is insufficient for `AP28_COMPOSITIONAL_INTEGRITY_DRIFT`.
