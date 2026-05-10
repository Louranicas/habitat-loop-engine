# receipt-graph

```text
[scaffold authorization receipt]
        |
        v
[manifest hash receipt] ---> [quality gate receipt]
        |                           |
        v                           v
[independent review receipt] --> [scaffold verified status]
        |
        v
[M0 waiting blocker]

Future M0 branches:
[claim receipt] -> [verifier receipt] -> [superseding correction if needed]
       \                                  /
        +------ [counter-evidence] -------+
```

## Required split anchors

- `^Manifest_sha256:` anchors the repository manifest or artifact set.
- `^Framework_sha256:` anchors upstream framework material when used.
- `^Source_sha256:` anchors a source artifact under review.
- `^Verdict:` states PASS, BLOCKED, WAIVED, SUPERSEDED, or INFORMATIONAL.

## Invariant

Receipts append evidence. They do not erase previous evidence. A later receipt may supersede an earlier one, but the graph must make the supersession explicit.
