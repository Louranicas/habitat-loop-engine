# M021 Anti-Pattern Events — anti_pattern_events.rs

> **File:** `crates/hle-storage/src/anti_pattern_events.rs` | **LOC:** ~310 | **Tests:** ~55
> **Layer:** L02 | **Cluster:** C04_ANTI_PATTERN_INTELLIGENCE
> **Role:** Append-only store for scanner-emitted DetectorEvent values with SHA anchoring and JSONL serialization

---

## Types at a Glance

| Type | Kind | Copy | Notes |
|---|---|---|---|
| `EventStore` | struct | No | Append-only event store backed by a connection from M025 pool |
| `EventStoreConfig` | struct | No | Builder-constructed store configuration |
| `EventQuery` | struct | No | Typed query filter for event reads |
| `EventPage` | struct | No | Bounded page of query results |
| `StoredEvent` | struct | No | Persisted event with monotone sequence number |
| `EventStoreStats` | struct | No | Read-only aggregate metrics |

---

## EventStore

```rust
pub struct EventStore {
    pool:   Arc<dyn DatabasePool>,   // C05 M025 pool — read via cross-cluster dep
    config: EventStoreConfig,
}
```

### Construction

```rust
impl EventStore {
    /// Build from a validated config and a pool reference from C05.
    pub fn new(pool: Arc<dyn DatabasePool>, config: EventStoreConfig) -> Result<Self>;
}
```

### Core Methods

| Method | Signature | Notes |
|---|---|---|
| `append` | `fn(&self, event: DetectorEvent) -> Result<StoredEvent>` | Writes one event; assigns monotone sequence number; never updates existing rows |
| `append_batch` | `fn(&self, events: &[DetectorEvent]) -> Result<Vec<StoredEvent>>` | Transactional batch append; all-or-nothing |
| `query` | `fn(&self, q: &EventQuery) -> Result<EventPage>` | Bounded read; `EventQuery::limit` capped at `config.max_page_size` |
| `stats` | `fn(&self) -> Result<EventStoreStats>` | Aggregate counts by pattern and severity |
| `event_count` | `fn(&self) -> Result<u64>` | Total events in store |

**No `update`, `delete`, or `truncate` methods exist.** The store is append-only by design.

---

## EventStoreConfig

```rust
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    pub max_page_size:    usize,   // Default: 500; enforces C12 — no unbounded reads
    pub table_name:       String,  // Default: "anti_pattern_events"
    pub jsonl_export_dir: Option<std::path::PathBuf>,
}
```

| Builder Method | Notes |
|---|---|
| `builder()` | Returns `EventStoreConfigBuilder` |
| `EventStoreConfigBuilder::max_page_size(usize)` | Clamps to 1..=5000 |
| `EventStoreConfigBuilder::table_name(impl Into<String>)` | Validated as SQL-safe identifier |
| `EventStoreConfigBuilder::jsonl_export_dir(PathBuf)` | Must be an existing directory at build time |
| `EventStoreConfigBuilder::build()` | Returns `Result<EventStoreConfig>` |

---

## StoredEvent

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    pub seq:         u64,              // Monotone sequence number assigned at append time
    pub event:       DetectorEvent,    // The original scanner finding
    pub appended_at: u64,             // Timestamp tick (from M001 substrate-types Timestamp)
    pub row_sha:     [u8; 32],        // SHA-256 of canonical JSONL serialization of this row
}
```

`StoredEvent::row_sha` is recomputed at query time and compared against the stored digest to
detect silent corruption. A mismatch raises error code 2310 (`EventStoreAppendFailed`) on reads
rather than returning silently corrupted data.

---

## EventQuery

```rust
#[derive(Debug, Clone)]
pub struct EventQuery {
    pub pattern_filter:  Option<AntiPatternId>,
    pub severity_floor:  Option<Severity>,
    pub after_seq:       Option<u64>,
    pub limit:           usize,    // Capped at EventStoreConfig::max_page_size
}
```

| Factory | Notes |
|---|---|
| `EventQuery::all()` | No filters, limit = `max_page_size` |
| `EventQuery::for_pattern(AntiPatternId)` | Single-pattern filter |
| `EventQuery::high_and_above()` | `severity_floor = High` |
| `EventQuery::since(seq: u64)` | Reads events appended after `seq` |

---

## EventPage

```rust
#[derive(Debug, Clone)]
pub struct EventPage {
    pub events:      Vec<StoredEvent>,
    pub next_seq:    Option<u64>,    // None when no more pages
    pub total_count: u64,            // Total matching events (not just this page)
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_last_page` | `fn(&self) -> bool` | `next_seq.is_none()` |
| `has_high_or_critical` | `fn(&self) -> bool` | Any event with severity >= High |

---

## EventStoreStats

```rust
#[derive(Debug, Clone)]
pub struct EventStoreStats {
    pub total_events:         u64,
    pub by_pattern:           Vec<(AntiPatternId, u64)>,
    pub by_severity:          Vec<(Severity, u64)>,
    pub highest_severity:     Option<Severity>,
    pub last_appended_seq:    Option<u64>,
}
```

---

## JSONL Export

When `config.jsonl_export_dir` is set, `append` writes a JSONL line to
`{dir}/anti_pattern_events_{date}.jsonl` after each successful database write. The line format
mirrors the `substrate-emit` JSONL pattern:

```jsonl
{"seq":1,"pattern_id":"AP29_BLOCKING_IN_ASYNC","severity":"HIGH","file":"src/foo.rs","line_start":42,"line_end":42,"evidence":"std::thread::sleep inside async fn run","receipt_sha":null,"appended_at":10023}
```

File rotation is by calendar date (UTC). The export is best-effort: a write failure to JSONL does
not roll back the database append. The database row is the authoritative record.

---

## Schema (Planned Migration)

```sql
CREATE TABLE IF NOT EXISTS anti_pattern_events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern_id   TEXT    NOT NULL,
    severity     TEXT    NOT NULL,
    file_path    TEXT    NOT NULL,
    line_start   INTEGER NOT NULL,
    line_end     INTEGER NOT NULL,
    evidence     TEXT    NOT NULL,   -- BoundedString content (max 1024 bytes)
    receipt_sha  BLOB,               -- nullable 32-byte SHA-256
    row_sha      BLOB    NOT NULL,   -- 32-byte SHA-256 of this row's canonical form
    appended_at  INTEGER NOT NULL
);
-- No UPDATE or DELETE triggers are defined; the application layer enforces append-only.
CREATE INDEX idx_ap_events_pattern ON anti_pattern_events(pattern_id);
CREATE INDEX idx_ap_events_severity ON anti_pattern_events(severity);
CREATE INDEX idx_ap_events_seq ON anti_pattern_events(seq);
```

---

## Design Notes

- `EventStore` uses the shared `DatabasePool` from C05 M025. It does not manage its own
  connection lifecycle; the pool handles retry and timeout policies.
- The append-only constraint is enforced both at the application layer (no update/delete methods)
  and documented at the schema level (no DML triggers that would permit modification). Auditors
  can verify by inspecting `sqlite_master` for missing UPDATE/DELETE triggers.
- `row_sha` detection on reads prevents silent storage corruption from being returned as valid
  evidence to downstream auditors (M024). A finding that cannot verify its own provenance must
  not block a claim.
- `max_page_size` is a hard cap, not a hint. Callers requesting a larger page receive an error
  (2311), not a truncated result, because silent truncation would be a C12 instance in the
  store itself.
- The store does not emit signals or events from within a lock scope. Lock guards are held only
  for the minimum duration of the write; JSONL export happens after guard release (C6 compliance).

---

*M021 Anti-Pattern Events Spec v1.0 | 2026-05-10*
