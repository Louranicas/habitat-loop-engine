#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

root = Path('.')
receipt = root / 'docs/quality/semantic-predicates.md'
quality_bar = root / 'QUALITY_BAR.md'
quality_gate = root / 'scripts/quality-gate.sh'

errors = []

if not receipt.exists():
    errors.append('missing docs/quality/semantic-predicates.md')
else:
    text = receipt.read_text()
    for predicate_id in ['HLE-SP-001', 'HLE-SP-002', 'HLE-SP-003']:
        if predicate_id not in text:
            errors.append(f'missing predicate id in semantic predicate receipt: {predicate_id}')
    required_receipt_terms = [
        'Source files:',
        'Rationale:',
        'Evaluator/check location:',
        'PASS example:',
        'FAIL example:',
        'Predicate-to-check map',
        'Current scaffold bars enumerated first',
    ]
    for term in required_receipt_terms:
        if term not in text:
            errors.append(f'missing semantic predicate receipt term: {term}')
    for evaluator in [
        'scripts/verify-semantic-predicates.sh',
        'scripts/verify-antipattern-registry.sh',
        'scripts/verify-sync.sh',
        'scripts/quality-gate.sh --scaffold',
    ]:
        if evaluator not in text:
            errors.append(f'missing evaluator mapping in semantic predicate receipt: {evaluator}')

if quality_bar.exists():
    qb = quality_bar.read_text()
    for predicate_id in ['HLE-SP-001', 'HLE-SP-002', 'HLE-SP-003']:
        if predicate_id not in qb:
            errors.append(f'QUALITY_BAR.md does not expose semantic predicate: {predicate_id}')
else:
    errors.append('missing QUALITY_BAR.md')

if quality_gate.exists():
    gate = quality_gate.read_text()
    if 'scripts/verify-semantic-predicates.sh' not in gate:
        errors.append('quality gate does not invoke semantic predicate verifier')
else:
    errors.append('missing scripts/quality-gate.sh')

anti_docs = sorted((root / 'ai_docs/anti_patterns').glob('*.md'))
anti_docs = [p for p in anti_docs if p.name != 'INDEX.md']
if len(anti_docs) < 8:
    errors.append(f'expected >=8 anti-pattern docs, found {len(anti_docs)}')
for path in anti_docs:
    text = path.read_text()
    required = [
        'Predicate ID: `HLE-SP-001`',
        '## Detector predicate',
        '## Negative control',
        '## Remediation expectation',
    ]
    for term in required:
        if term not in text:
            errors.append(f'{path}: missing {term}')

specs = sorted((root / 'ai_specs').glob('S*.md'))
if len(specs) != 13:
    errors.append(f'expected exactly 13 ai_specs, found {len(specs)}')
for path in specs:
    text = path.read_text()
    required = [
        '## Acceptance',
        'Present in `ai_specs/INDEX.md`',
        'Referenced by scaffold verification',
        'Not marked execution PASS before implementation evidence exists',
    ]
    for term in required:
        if term not in text:
            errors.append(f'{path}: missing acceptance gate term: {term}')

if errors:
    raise SystemExit('verify-semantic-predicates FAIL\n' + '\n'.join(errors))

print('verify-semantic-predicates PASS')
PY
