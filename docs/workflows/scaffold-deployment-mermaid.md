# Habitat Loop Engine Scaffold Workflow Visualizations

Created UTC: 2026-05-10T00:05:15Z
Status: text-first scaffold preservation diagrams
Boundary: diagrams describe scaffold authorization and verification only; M0/runtime execution remains blocked until explicit `begin M0`.
Related narrative: [scaffold-deployment-workflow.md](scaffold-deployment-workflow.md)
Related Kanban map: [scaffold-deployment-kanban-map.md](scaffold-deployment-kanban-map.md)

## How to read these diagrams

These Mermaid blocks are source artifacts, not generated binary attachments. They are intended to remain reviewable in plain markdown and renderable by Mermaid-aware viewers. Each diagram preserves one view of the same scaffold-only workflow:

- authorization flow: phrase-gated progression from read-only assimilation to `m0_waiting`;
- artifact topology: files and receipts created by the scaffold packet;
- Kanban graph: bounded fan-out/fan-in review work;
- verification sequence: command/review order used to close the scaffold receipt;
- state machine: allowed scaffold states and the only transition that can open M0.

Visual convention: solid arrows are required workflow transitions; dashed arrows are provenance/comment links; decision diamonds are authorization or verifier gates. Any failed gate loops back to scaffold-only correction, never to live runtime behavior.

## Authorization flow

```mermaid
flowchart TD
    A[Read-only framework assimilation] --> B{Command-2 Cycle 3 receipt exists?}
    B -- yes --> D[Scaffold preflight ready]
    B -- no --> C{Exact Luke waiver phrase?}
    C -- no --> X[Write blocked preflight receipt and stop]
    C -- yes --> W[Write Command-2 waiver receipt]
    W --> D
    D --> E{Exact phrase: begin scaffold?}
    E -- no --> Y[Hold no-code boundary]
    E -- yes --> F[Create scaffold root]
    F --> G[Generate scaffold-only substrate]
    G --> H[Run quality gate]
    H --> I{Gate PASS?}
    I -- no --> J[Fix scaffold-only defects]
    J --> H
    I -- yes --> K[Independent read-only review]
    K --> L{Review blocking?}
    L -- yes --> J
    L -- no --> M[Refresh manifests and receipts]
    M --> N[State: scaffold verified, M0 waiting]
```

Notes:

- `begin scaffold` authorizes only file/substrate creation and verification.
- A failing quality gate or blocking review can only produce bounded scaffold fixes.
- The terminal state is `M0 waiting`, not deployment or runtime operation.

## Artifact topology

```mermaid
graph LR
    FW[Deployment framework] --> R1[Waiver and scaffold receipts]
    FW --> V[Dedicated review vault]
    V --> CS[Canonical receipt status]
    Root[Scaffold root] --> Docs[Root docs]
    Root --> Specs[S01-S13 specs]
    Root --> AIDocs[Layers/modules/patterns]
    Root --> Rust[4 Rust skeleton crates]
    Root --> Scripts[Verification scripts]
    Root --> Tests[Python + cargo tests]
    Root --> Manifests[SHA256SUMS]
    Scripts --> Gate[quality-gate --scaffold]
    Gate --> Receipt[scaffold-created receipt]
    Receipt --> FW
    Receipt --> V
```

Topology boundary:

- Rust crates are compile-safe skeletons, not executors.
- Scripts are verifier/safety surfaces, not daemons or cron jobs.
- Receipts and manifests are durable evidence that the scaffold is closed and M0 remains blocked.

## Kanban graph

```mermaid
flowchart LR
    K0[K0 Parent triage/provenance] -. comments .-> K1[K1 Workflow narrative audit]
    K0 -. comments .-> K2[K2 Visualizations]
    K0 -. comments .-> K3[K3 Kanban orchestration map]
    K0 -. comments .-> K4[K4 Boundary review]
    K1 --> K5[K5 Synthesis closure]
    K2 --> K5
    K3 --> K5
    K4 --> K5
```

Kanban reading rules:

- K0 is provenance and should stay parked unless Luke asks for board cleanup.
- K1-K4 may run in parallel because each is docs/review-only.
- K5 is the only fan-in card and should read parent handoffs before writing closure.
- No card may spawn live integrations, background services, cron jobs, or M0 runtime work.

## Verification sequence

```mermaid
sequenceDiagram
    participant Luke
    participant Weaver
    participant Scaffold as Scaffold Root
    participant Gate as Quality Gate
    participant Reviewer
    participant Vault
    Luke->>Weaver: begin scaffold
    Weaver->>Weaver: preflight waiver + root absence
    Weaver->>Scaffold: create scaffold-only files
    Weaver->>Gate: scripts/quality-gate.sh --scaffold
    Gate-->>Weaver: PASS
    Weaver->>Reviewer: independent read-only review
    Reviewer-->>Weaver: PASS + nonblocking safety scanner suggestion
    Weaver->>Scaffold: add bounded/safety scanners
    Weaver->>Gate: rerun gate
    Gate-->>Weaver: PASS
    Weaver->>Vault: update receipts/status/manifests
    Weaver-->>Luke: SCAFFOLD_CREATED_AND_VERIFIED
```

Sequence guardrails:

- Review feedback can enrich bounded verifiers only.
- Receipt/status updates document the closed scaffold state.
- `SCAFFOLD_CREATED_AND_VERIFIED` is a preservation claim, not a deployment claim.

## State machine

```mermaid
stateDiagram-v2
    [*] --> PreScaffold
    PreScaffold --> BlockedOnCommand2: no receipt/waiver
    BlockedOnCommand2 --> WaivedForScaffoldOnly: exact waiver phrase
    WaivedForScaffoldOnly --> ScaffoldAuthorized: begin scaffold
    ScaffoldAuthorized --> ScaffoldCreated: root generated
    ScaffoldCreated --> ScaffoldGateFailed: gate fails
    ScaffoldGateFailed --> ScaffoldCreated: scaffold-only fix
    ScaffoldCreated --> ScaffoldVerified: gate + review pass
    ScaffoldVerified --> M0Waiting: receipts/manifests refreshed
    M0Waiting --> M0Authorized: begin M0
    M0Waiting --> [*]
```

State boundary:

- `M0Authorized` is shown only as a future gated state.
- The scaffold workflow stops at `M0Waiting` unless Luke separately gives the exact `begin M0` phrase.
