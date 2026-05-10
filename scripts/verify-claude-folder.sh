#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for f in .claude/context.json .claude/PROJECT_CONTEXT.md .claude/LOCAL_RULES.md .claude/commands/verify-scaffold.md .claude/agents/scaffold-reviewer.md .claude/rules/no-m0-before-authorization.md; do
  test -f "$f" || { echo "missing $f"; exit 1; }
done
python3 -m json.tool .claude/context.json >/dev/null
printf 'verify-claude-folder PASS
'
