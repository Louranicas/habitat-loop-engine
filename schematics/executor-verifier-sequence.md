# executor-verifier-sequence

```text
participant Human
participant Executor
participant ArtifactStore
participant Verifier
participant ReceiptGraph

Human -> Executor: future authorized workflow step
Executor -> ArtifactStore: write bounded artifact
Executor -> ReceiptGraph: write CLAIM receipt with artifact sha256
Verifier -> ArtifactStore: read artifact by path and sha256
Verifier -> ReceiptGraph: read claim receipt
Verifier -> Verifier: run independent checks and negative controls
Verifier -> ReceiptGraph: write VERIFIER receipt with PASS or BLOCKED
Human -> ReceiptGraph: review verdict and counter-evidence locator
```

## Scaffold boundary

This sequence is a contract diagram. The current scaffold does not implement a runtime executor. It documents that future M0 work must keep executor and verifier authority separate.

## Failure semantics

- Missing artifact hash -> BLOCKED.
- Executor and verifier identity collapse -> BLOCKED.
- Negative control unexpectedly passes -> BLOCKED.
- Human phrase gate absent for runtime work -> M0 waiting, not PASS.
