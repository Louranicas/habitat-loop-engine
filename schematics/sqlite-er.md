# sqlite-er

```text
workflow_runs
  run_id PK
  status
  created_at
  authority_phrase

workflow_ticks
  tick_id PK
  run_id FK -> workflow_runs.run_id
  phase
  started_at
  completed_at

phase_events
  event_id PK
  tick_id FK -> workflow_ticks.tick_id
  event_class
  artifact_sha256

receipts
  receipt_id PK
  claim_id
  claim_class
  verdict
  source_sha256
  parent_sha256

receipt_claims
  claim_id PK
  receipt_id FK -> receipts.receipt_id
  authority_role
  counter_evidence_locator

verifier_results
  verifier_result_id PK
  receipt_id FK -> receipts.receipt_id
  verifier_name
  negative_controls_passed

blockers
  blocker_id PK
  run_id FK -> workflow_runs.run_id
  blocker_class
  human_action_required
```

## Scaffold note

`migrations/0001_scaffold_schema.sql` is a schema sketch and review artifact. It must not be applied to live Habitat databases before M0 authorization.
