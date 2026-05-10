#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

count=$(find ai_docs/anti_patterns -maxdepth 1 -name '*.md' ! -name INDEX.md | wc -l)
test "$count" -ge 8 || { echo "expected >=8 anti-pattern docs"; exit 1; }
find ai_docs/use_patterns -maxdepth 1 -name '*.md' ! -name INDEX.md | grep -q . || { echo 'missing use-pattern docs'; exit 1; }
printf 'verify-antipattern-registry PASS
'
