-- M0 local runtime ledger schema.
-- Safe by default: this schema records HLE-local workflow evidence and does not
-- mutate external Habitat service databases.

CREATE TABLE IF NOT EXISTS workflow_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workflow_name TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('running','pass','fail','awaiting-human','rolled-back')),
  created_unix INTEGER NOT NULL,
  completed_unix INTEGER
);

CREATE TABLE IF NOT EXISTS step_receipts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),
  step_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending','running','awaiting-human','passed','failed','rolled-back')),
  verifier_verdict TEXT NOT NULL,
  message TEXT NOT NULL,
  created_unix INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_step_receipts_run_id ON step_receipts(run_id);
CREATE INDEX IF NOT EXISTS idx_step_receipts_step_id ON step_receipts(step_id);
