# zellij-orchestrator-deployment-flow

```text
Weaver / Hermes planner
  |
  v
verify pane labels and target context
  |
  v
send role-specific Command / Command-2 / Command-3 packet
  |
  v
bounded debate or review interval
  |
  v
collect durable artifacts and hashes
  |
  v
synthesize receipt and Kanban comments
  |
  v
return visible panes only when active dispatch follow-up requires it
```

## Scaffold boundary

Zellij orchestration may be used to gather reviews and debate outputs, but it does not grant runtime authority. A Command-pane recommendation must be captured as evidence and then verified against scaffold gates.

## Targeting rule

Pane labels and layout must be verified before dispatch. Do not assume focus, tab order, or pane numbering from memory.
