# Quickstart

## Prerequisites

- Rust toolchain with `cargo`.
- Python 3 for scaffold verification scripts.
- No external services required for scaffold verification.

## Scaffold-only verification

```bash
scripts/quality-gate.sh --scaffold
```

## Running M0 once implemented

M0 is not implemented in scaffold. Do not add runtime behavior until Luke authorizes `begin M0`.

## Local DB location

Future local DB path: `.deployment-work/hle-local.sqlite3`.

## Receipt location

Scaffold and future runtime receipts live under `.deployment-work/receipts/`.

## Troubleshooting

- If `verify-sync` fails, inspect `plan.toml` and `ULTRAMAP.md` first.
- If doc links fail, run `scripts/verify-doc-links.sh` and fix exact missing paths.
- If cargo fails, this scaffold has accidentally crossed from skeleton into broken implementation.
