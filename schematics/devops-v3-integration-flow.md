# devops-v3-integration-flow

```text
HLE verifier contract
  |
  v
read-only DevOps V3 observation request
  |
  v
bounded health / metric snapshot
  |
  v
hash-addressed observation artifact
  |
  v
verifier receipt cites snapshot hash
  |
  v
human review decides whether runtime action is authorized
```

## Scaffold boundary

This repository may document future DevOps Engine integration, but it must not perform live writes, service restarts, deployment actions, cron creation, or daemon management before explicit authorization.

## Future M0 safety rule

DevOps observations and DevOps actions must be different claim classes. A green health snapshot may support a recommendation; it cannot by itself authorize deployment.
