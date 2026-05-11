# M025 Pool — Connection Pool and Connection Discipline

> **Module ID:** M025 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/pool.rs`
> **Error Codes:** 2400–2402
> **Role:** Provides the sole entry point for SQLite connections across all C05 modules.
> Enforces WAL mode, foreign-key pragma, and pool-size limits on every connection.

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `Pool` | trait | Abstract pool interface; implemented by `LocalPool` |
| `LocalPool` | struct | rusqlite-backed connection pool, thread-safe |
| `PoolConfig` | struct (builder) | Size limits, path, timeouts |
| `ConnectionGuard<'_>` | struct | RAII guard; enforces release on drop |
| `StorageError` | enum | Cluster error type, codes 2400–2499 |

---

## `Pool` Trait

```rust
/// Abstract connection pool. Implemented by [`LocalPool`].
/// All methods take `&self` — interior mutability via `Mutex<VecDeque<Connection>>`.
pub trait Pool: Send + Sync {
    /// Execute a closure with an exclusive connection from the pool.
    /// The connection is returned to the pool after the closure returns.
    ///
    /// # Errors
    /// Returns [`StorageError::ConnectionAcquire`] (2401) if the pool is exhausted
    /// before `acquire_timeout` elapses.
    #[must_use]
    fn with_conn<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, StorageError>;

    /// Current count of idle connections in the pool.
    #[must_use]
    fn idle_count(&self) -> usize;

    /// Maximum pool size as configured.
    #[must_use]
    fn max_size(&self) -> usize;

    /// Run a basic connectivity check (SELECT 1).
    ///
    /// # Errors
    /// Returns [`StorageError::PoolInit`] (2400) if the check query fails.
    fn health_check(&self) -> Result<(), StorageError>;
}
```

---

## `PoolConfig` (Builder)

```rust
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Absolute path to the SQLite database file, or `:memory:` for tests.
    pub path: String,
    /// Maximum number of concurrent connections. Default: 4.
    pub max_connections: usize,
    /// Minimum idle connections kept open. Default: 1.
    pub min_connections: usize,
    /// Maximum wait time to acquire a connection. Default: 5 seconds.
    pub acquire_timeout_secs: u64,
}

impl PoolConfig {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self;

    #[must_use]
    pub const fn with_max_connections(self, n: usize) -> Self;

    #[must_use]
    pub const fn with_min_connections(self, n: usize) -> Self;

    #[must_use]
    pub const fn with_acquire_timeout_secs(self, secs: u64) -> Self;

    /// Validate configuration invariants.
    ///
    /// # Errors
    /// Returns [`StorageError::PoolInit`] if `min > max` or `max == 0`.
    pub fn validate(&self) -> Result<(), StorageError>;
}
```

Default values are compile-time constants:

```rust
pub const DEFAULT_MAX_CONNECTIONS: usize = 4;
pub const DEFAULT_MIN_CONNECTIONS: usize = 1;
pub const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 5;
```

---

## `LocalPool` — Concrete Implementation

```rust
#[derive(Debug)]
pub struct LocalPool {
    inner: std::sync::Mutex<std::collections::VecDeque<rusqlite::Connection>>,
    config: PoolConfig,
}

impl LocalPool {
    /// Open the database file and pre-populate the idle pool with `min_connections`
    /// connections. Each connection immediately runs the WAL and FK pragmas.
    ///
    /// # Errors
    /// Returns [`StorageError::PoolInit`] (2400) if the file cannot be opened or
    /// a pragma fails.
    pub fn open(config: PoolConfig) -> Result<Self, StorageError>;
}

impl Pool for LocalPool { /* ... */ }
```

### Connection Lifecycle

Every connection returned to callers via `with_conn` has already had these pragmas applied
at open time and verified to return the expected values:

```sql
PRAGMA journal_mode = WAL;   -- must return 'wal'
PRAGMA foreign_keys = ON;    -- must return 1
PRAGMA busy_timeout = 5000;  -- 5 s busy wait before SQLITE_BUSY
```

Connections are validated (SELECT 1) before being handed out. A connection that fails
validation is discarded; a fresh connection is opened in its place up to `max_connections`.

### Pool Size Discipline

| Limit | Behaviour |
|-------|-----------|
| `idle >= min_connections` | Keep-alive: connections are not closed between calls |
| `total < max_connections` | Open a new connection when pool is empty |
| `total == max_connections` | Spin-wait up to `acquire_timeout_secs`; then `StorageError::ConnectionAcquire` (2401) |

The pool never grows beyond `max_connections`. For the HLE local-M0 use case the default
`max_connections = 4` is deliberately conservative — the engine is one-shot, not a daemon.

---

## `ConnectionGuard`

```rust
/// RAII wrapper returned by `with_conn`. Returns the connection to the pool on drop.
/// Not constructable outside this module — obtained only through `Pool::with_conn`.
pub struct ConnectionGuard<'pool> {
    conn: Option<rusqlite::Connection>,
    pool: &'pool LocalPool,
}

impl<'pool> std::ops::Deref for ConnectionGuard<'pool> {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target;
}

impl<'pool> Drop for ConnectionGuard<'pool> {
    /// Returns the connection to the idle pool. If the pool is full
    /// (should not occur under normal use), the connection is closed.
    fn drop(&mut self);
}
```

---

## `StorageError` Enum (Cluster-wide, defined in `pool.rs`)

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("pool init failed (2400): {0}")]
    PoolInit(String),

    #[error("connection acquire timeout (2401): pool exhausted after {secs}s")]
    ConnectionAcquire { secs: u64 },

    #[error("connection leak detected (2402): guard dropped without release")]
    ConnectionLeak,

    #[error("migration not found (2410): {id}")]
    MigrationNotFound { id: u32 },

    #[error("migration checksum mismatch (2411): id={id}")]
    MigrationChecksum { id: u32 },

    #[error("migration order violation (2412): expected {expected}, found {found}")]
    MigrationOrder { expected: u32, found: u32 },

    #[error("run insert failed (2420): {0}")]
    RunInsert(String),

    #[error("run not found (2421): run_id={0}")]
    RunNotFound(i64),

    #[error("run status invalid (2422): {0}")]
    RunStatusInvalid(String),

    #[error("tick insert failed (2430): {0}")]
    TickInsert(String),

    #[error("tick parent missing (2431): parent_tick_id={0}")]
    TickParentMissing(i64),

    #[error("evidence size exceeded (2440): {bytes} > {limit}")]
    EvidenceSizeExceeded { bytes: usize, limit: usize },

    #[error("evidence kind unknown (2441): {0}")]
    EvidenceKindUnknown(String),

    #[error("evidence insert failed (2442): {0}")]
    EvidenceInsert(String),

    #[error("verifier insert failed (2450): {0}")]
    VerifierInsert(String),

    #[error("verifier verdict invalid (2451): {0}")]
    VerifierVerdictInvalid(String),

    #[error("blocker insert failed (2460): {0}")]
    BlockerInsert(String),

    #[error("blocker not found (2461): run_id={run_id}, step_id={step_id}")]
    BlockerNotFound { run_id: i64, step_id: String },

    #[error("storage error (2499): {0}")]
    Storage(#[from] rusqlite::Error),
}
```

`StorageError` implements `From<rusqlite::Error>` (code 2499) so individual modules only
convert to their specific variants and let the blanket impl handle unexpected raw errors.

---

## Method/Trait Table

| Item | Signature | Notes |
|------|-----------|-------|
| `Pool::with_conn` | `fn with_conn<F,T>(&self, f: F) -> Result<T, StorageError>` | Sole connection entry point |
| `Pool::idle_count` | `fn idle_count(&self) -> usize` | For health checks and tests |
| `Pool::max_size` | `fn max_size(&self) -> usize` | Reflects config |
| `Pool::health_check` | `fn health_check(&self) -> Result<(), StorageError>` | SELECT 1 probe |
| `LocalPool::open` | `fn open(config: PoolConfig) -> Result<Self, StorageError>` | Constructor |
| `PoolConfig::new` | `fn new(path: impl Into<String>) -> Self` | Default builder |
| `PoolConfig::validate` | `fn validate(&self) -> Result<(), StorageError>` | Pre-open checks |

---

## Design Notes

1. **No async.** HLE local-M0 is one-shot and foreground. Synchronous `rusqlite` avoids
   Tokio executor dependency in the storage layer. Blocking is bounded by `busy_timeout`
   and pool `acquire_timeout_secs`.

2. **Trait surface enables test injection.** Tests pass an `Arc<dyn Pool>` backed by an
   in-memory database (`":memory:"`). All higher modules depend on `&dyn Pool`, not
   on `LocalPool` directly.

3. **WAL + FK enforced at open, not per-query.** The pragma pair is set once per
   connection at open time. `with_conn` asserts they are still active via a lightweight
   `PRAGMA foreign_keys` read-back in debug builds.

4. **Pool size is deliberately small.** The HLE one-shot model runs a bounded workflow,
   not a multi-tenant server. `max_connections = 4` matches the expected concurrency
   (executor + verifier + evidence writer + blocker updater).

5. **No connection strings.** The pool accepts a file path only. No URI-style options are
   supported to prevent accidental mode changes (e.g., `?mode=memory`).

---

## Test Targets (minimum 50)

- `pool_open_memory`: opens `:memory:`, health_check passes
- `pool_wal_pragma_active`: asserts `PRAGMA journal_mode` returns `wal`
- `pool_fk_pragma_active`: asserts `PRAGMA foreign_keys` returns `1`
- `pool_acquire_returns_connection`: `with_conn` executes `SELECT 1`
- `pool_idle_count_after_release`: idle count returns to `min_connections` after closure
- `pool_max_connections_respected`: saturating pool returns `ConnectionAcquire` error
- `pool_config_validate_min_gt_max`: returns `PoolInit` error
- `pool_config_validate_max_zero`: returns `PoolInit` error
- `pool_health_check_ok`: returns `Ok(())` on healthy database
- `pool_storage_error_from_rusqlite`: `From<rusqlite::Error>` wraps as code 2499

---

*M025 Pool Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
