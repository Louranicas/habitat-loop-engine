# atuin-qi-chain

```text
shell command history
  |
  v
Atuin query / QI recall
  |
  v
bounded, redacted excerpt
  |
  v
operator provenance note
  |
  v
receipt graph support edge
  |
  v
independent verifier checks actual artifacts and hashes
```

## Authority boundary

Atuin evidence can explain what was attempted. It does not prove success. Verifier receipts and artifact hashes remain the authority for PASS.

## Secret-safe handling

Any future Atuin adapter must redact tokens, keys, passwords, and long command payloads before writing evidence records. Redaction failures must block receipt creation.
