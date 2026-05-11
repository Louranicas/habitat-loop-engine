-- M028 / M029 / M030 / M031 — supplementary tables for the C05 Persistence Ledger.
-- All tables use IF NOT EXISTS so re-application is a no-op.

-- M028 — workflow tick ledger (causal chain per run).
CREATE TABLE IF NOT EXISTS workflow_ticks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),
  tick_id INTEGER NOT NULL,
  created_unix INTEGER NOT NULL,
  parent_tick_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_workflow_ticks_run_id ON workflow_ticks(run_id);
CREATE INDEX IF NOT EXISTS idx_workflow_ticks_tick_id ON workflow_ticks(tick_id);

-- M029 — bounded evidence path/blob store.
CREATE TABLE IF NOT EXISTS evidence_store (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),
  evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('stdout','stderr','artifact')),
  path_or_inline TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  created_unix INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_evidence_store_run_id ON evidence_store(run_id);
CREATE INDEX IF NOT EXISTS idx_evidence_store_sha256 ON evidence_store(sha256);

-- M030 — verifier verdict ledger (append-only corrections model).
CREATE TABLE IF NOT EXISTS verifier_results_store (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),
  step_id TEXT NOT NULL,
  verdict TEXT NOT NULL CHECK (verdict IN ('PASS','FAIL','AWAITING_HUMAN')),
  receipt_sha TEXT NOT NULL,
  verifier_version TEXT NOT NULL,
  created_unix INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_verifier_results_run_id ON verifier_results_store(run_id);
CREATE INDEX IF NOT EXISTS idx_verifier_results_step_id ON verifier_results_store(step_id);

-- M031 — blocked / awaiting-human state persistence.
CREATE TABLE IF NOT EXISTS blockers_store (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),
  step_id TEXT NOT NULL,
  blocker_kind TEXT NOT NULL,
  since_unix INTEGER NOT NULL,
  expected_resolver_role TEXT NOT NULL,
  resolved_unix INTEGER
);

CREATE INDEX IF NOT EXISTS idx_blockers_store_run_id ON blockers_store(run_id);
CREATE INDEX IF NOT EXISTS idx_blockers_store_resolved_unix ON blockers_store(resolved_unix);
