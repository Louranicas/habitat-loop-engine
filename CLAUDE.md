# Habitat Loop Engine — Project Context

This repository is scaffold-only until Luke authorizes `begin M0`.

## Rules

- Do not implement runtime executor behavior during scaffold-only work.
- Do not create live Habitat write integrations.
- Do not create cron jobs or daemons.
- Treat verifier as sole PASS/FAIL authority.
- Keep `plan.toml`, `ULTRAMAP.md`, `ai_docs`, `ai_specs`, and scripts aligned.
- Run `scripts/quality-gate.sh --scaffold` after scaffold changes.

## Commands

- `scripts/verify-sync.sh`
- `scripts/quality-gate.sh --scaffold`
- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
