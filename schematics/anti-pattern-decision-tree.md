# anti-pattern-decision-tree

```text
New scaffold or future runtime surface
  |
  v
Does it introduce async work, locks, unbounded state, or claim authority?
  |-- no --> require ordinary doc/schema/test review
  |
  yes
  |
  v
Which predicate class applies?
  |-- AP28 compositional drift --> verify cross-surface alignment
  |-- AP29 blocking in async --> require async-native or spawn_blocking boundary
  |-- AP31 nested locks --> require lock-order proof or redesign
  |-- C6/C7/C12/C13 constraints --> require specific remediation predicate
  |
  v
Is there a negative control and verifier receipt?
  |-- no --> BLOCKED
  |-- yes --> reviewer checks evidence and hash anchors
  |
  v
PASS only if the semantic predicate, not just file count, is satisfied
```

## Review note

The anti-pattern registry is not a checklist trophy. It is a set of detector predicates that future M0 code must keep satisfiable under independent review.
