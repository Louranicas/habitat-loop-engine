# FP_FALSE_PASS_CLASSES

Status: scaffold detector predicate. Runtime detector implementation is explicitly deferred until M0 authorization.

Predicate ID: `HLE-SP-001`

## Detector predicate

A future detector for `FP_FALSE_PASS_CLASSES` must identify evidence of false PASS classes where scaffold receipts appear green without implementation-grade evidence in source, configuration, verifier receipts, or scaffold review artifacts. The detector predicate is reviewable at scaffold time even though executable runtime detection is not implemented here.

## Negative control

A compliant example must not fire the detector when the same concern is handled by an explicit boundary, bounded contract, documented verifier gate, or typed/isolated implementation path. Negative controls are required before any future runtime detector can claim PASS.

## Remediation expectation

A remediation receipt must name the affected file, describe the semantic correction, and point to verifier evidence. Count-only evidence such as file presence or registry size is insufficient for `FP_FALSE_PASS_CLASSES`.
