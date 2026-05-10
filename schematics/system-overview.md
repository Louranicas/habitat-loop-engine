# system-overview

```text
                 +-------------------------------------------+
                 |        Luke / Habitat operator            |
                 | explicit phrases, scope, waivers, review  |
                 +---------------------+---------------------+
                                       |
                                       v
+------------------+        +----------------------+        +------------------+
| shared context   | -----> | HLE scaffold repo    | -----> | verifier gates   |
| genesis, specs,  |        | docs, schemas,       |        | quality-gate,    |
| Kanban receipts  |        | crates, runbooks     |        | manifests, tests |
+------------------+        +----------+-----------+        +---------+--------+
                                      |                              |
                                      v                              v
                          +----------------------+        +------------------+
                          | receipt graph        | <----- | independent      |
                          | claims, hashes,      |        | review evidence  |
                          | blockers, waivers    |        +------------------+
                          +----------+-----------+
                                     |
                                     v
                          +----------------------+
                          | M0 waiting boundary  |
                          | no runtime until     |
                          | explicit begin M0    |
                          +----------------------+
```

## Authority notes

- The scaffold repository is a contract and verification substrate, not a live loop engine.
- Shared-context and Kanban artifacts inform the scaffold but do not override phrase gates.
- Verifier gates can reject scaffold drift; they cannot authorize M0.
- Receipt graph evidence preserves why a claim was accepted, blocked, waived, or superseded.
