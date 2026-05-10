#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for f in runbooks/INDEX.md runbooks/scaffold-verification.md runbooks/m0-authorization-boundary.md schemas/status.schema.json; do
  test -f "$f" || { echo "missing $f"; exit 1; }
done
grep -q 'AwaitingHuman\|M0 Authorization\|Scaffold Verification' runbooks/*.md || { echo 'runbook semantics missing'; exit 1; }
printf 'verify-runbook-schema PASS
'
