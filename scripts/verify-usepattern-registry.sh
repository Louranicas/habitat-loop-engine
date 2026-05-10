#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

count=$(find ai_docs/use_patterns -maxdepth 1 -name '*.md' ! -name INDEX.md | wc -l)
test "$count" -ge 6 || { echo "expected >=6 use-pattern docs"; exit 1; }
printf 'verify-usepattern-registry PASS
'
