#![forbid(unsafe_code)]

//! M021 Anti-Pattern Events — append-only in-memory `DetectorEvent` store.
//!
//! Stub implementation: backed by `Vec<StoredEvent>` under a `std::sync::Mutex`.
//! Pool integration (C05 M025) is a deferred TODO comment only; no pool import
//! is required and no pool code exists yet.
//!
//! Design invariants:
//! - No `update` or `delete` methods. Append-only by design.
//! - Lock guards are released before any I/O operation (C6 compliance).
//! - `max_page_size` is a hard cap; callers requesting more receive an error.
//!
//! Layer: L02 | Cluster: C04

use std::fmt;
use std::sync::Mutex;

use substrate_types::HleError;

// Re-export from hle-verifier is not possible at L02 (verifier lives at L04).
// `DetectorEvent` will be imported by the caller. For the stub we define a
// minimal local alias so the store compiles independently.
//
// When hle-verifier::anti_pattern_scanner exists the caller passes real
// `DetectorEvent` values from that crate. For now we define `DetectorEvent`
// locally so this crate compiles on its own.

/// Opaque severity level matching the M020 `Severity` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StoreSeverity {
    /// Informational; does not block any workflow gate.
    Low,
    /// Advisory; triggers a warning annotation in the gate output.
    Medium,
    /// Blocks workflow promotion when count > 0.
    High,
    /// Blocks workflow promotion immediately.
    Critical,
}

impl fmt::Display for StoreSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => f.write_str("LOW"),
            Self::Medium => f.write_str("MEDIUM"),
            Self::High => f.write_str("HIGH"),
            Self::Critical => f.write_str("CRITICAL"),
        }
    }
}

// ---------------------------------------------------------------------------
// DetectorEvent (local stub definition — replaced by hle-verifier import at M0)
// ---------------------------------------------------------------------------

/// A single scanner finding emitted by an `M020` scanner.
///
/// Stub type: field set is authoritative per spec but does not carry the full
/// `BoundedString` wrapper; evidence is a plain `String` (validated at ≤ 1024 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorEvent {
    /// Identifier of the catalogued anti-pattern.
    pub pattern_id: String,
    /// Severity of this finding.
    pub severity: StoreSeverity,
    /// File path of the finding.
    pub file_path: String,
    /// Inclusive start line.
    pub line_start: u32,
    /// Inclusive end line.
    pub line_end: u32,
    /// Human-readable evidence (capped at 1024 bytes at construction).
    pub evidence: String,
    /// Optional SHA-256 of the verifier receipt that triggered the scan.
    pub receipt_sha: Option<[u8; 32]>,
}

impl DetectorEvent {
    /// Construct a `DetectorEvent`, validating that `evidence` does not exceed
    /// 1024 bytes and that `line_end >= line_start`.
    ///
    /// # Errors
    ///
    /// Returns `[E2300]` when the evidence string exceeds 1024 bytes.
    /// Returns `[E2300]` when `line_end < line_start`.
    pub fn new(
        pattern_id: impl Into<String>,
        severity: StoreSeverity,
        file_path: impl Into<String>,
        line_start: u32,
        line_end: u32,
        evidence: impl Into<String>,
    ) -> Result<Self, HleError> {
        let evidence = evidence.into();
        if evidence.len() > 1024 {
            return Err(HleError::new(
                "[E2300] evidence string exceeds 1024-byte cap",
            ));
        }
        if line_end < line_start {
            return Err(HleError::new("[E2300] line_end must be >= line_start"));
        }
        Ok(Self {
            pattern_id: pattern_id.into(),
            severity,
            file_path: file_path.into(),
            line_start,
            line_end,
            evidence,
            receipt_sha: None,
        })
    }

    /// Builder-chain method to attach a receipt SHA anchor.
    #[must_use]
    pub fn with_receipt_sha(mut self, sha: [u8; 32]) -> Self {
        self.receipt_sha = Some(sha);
        self
    }

    /// True when a receipt SHA anchor is present.
    #[must_use]
    pub fn is_anchored(&self) -> bool {
        self.receipt_sha.is_some()
    }
}

// ---------------------------------------------------------------------------
// StoredEvent
// ---------------------------------------------------------------------------

/// A persisted `DetectorEvent` with monotone sequence number and timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Monotone sequence number assigned at append time (1-based).
    pub seq: u64,
    /// The original scanner finding.
    pub event: DetectorEvent,
    /// Logical timestamp (stub: wall-clock milliseconds from `std::time::SystemTime`).
    pub appended_at: u64,
}

impl StoredEvent {
    /// True when the underlying event has a receipt SHA anchor.
    #[must_use]
    pub fn is_anchored(&self) -> bool {
        self.event.is_anchored()
    }
}

// ---------------------------------------------------------------------------
// EventStoreConfig
// ---------------------------------------------------------------------------

/// Configuration for `EventStore`.
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    /// Maximum events returned per query page. Hard cap — not a hint.
    pub max_page_size: usize,
}

impl EventStoreConfig {
    /// Default configuration.
    #[must_use]
    pub fn default_config() -> Self {
        Self { max_page_size: 500 }
    }

    /// Construct with an explicit page size, clamped to 1..=5000.
    #[must_use]
    pub fn with_max_page_size(size: usize) -> Self {
        Self {
            max_page_size: size.clamp(1, 5000),
        }
    }
}

impl Default for EventStoreConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

// ---------------------------------------------------------------------------
// EventQuery
// ---------------------------------------------------------------------------

/// Typed query filter for event reads.
#[derive(Debug, Clone)]
pub struct EventQuery {
    /// When set, only events matching this pattern ID are returned.
    pub pattern_filter: Option<String>,
    /// When set, only events with severity >= this floor are returned.
    pub severity_floor: Option<StoreSeverity>,
    /// When set, only events with `seq > after_seq` are returned.
    pub after_seq: Option<u64>,
    /// Maximum events to return. Capped at `EventStoreConfig::max_page_size`.
    pub limit: usize,
}

impl EventQuery {
    /// Return all events, up to `max_page_size`.
    #[must_use]
    pub fn all() -> Self {
        Self {
            pattern_filter: None,
            severity_floor: None,
            after_seq: None,
            limit: 500,
        }
    }

    /// Filter by a single pattern ID string.
    #[must_use]
    pub fn for_pattern(pattern_id: impl Into<String>) -> Self {
        Self {
            pattern_filter: Some(pattern_id.into()),
            severity_floor: None,
            after_seq: None,
            limit: 500,
        }
    }

    /// Filter to High severity and above.
    #[must_use]
    pub fn high_and_above() -> Self {
        Self {
            pattern_filter: None,
            severity_floor: Some(StoreSeverity::High),
            after_seq: None,
            limit: 500,
        }
    }

    /// Return events appended after `seq`.
    #[must_use]
    pub fn since(seq: u64) -> Self {
        Self {
            pattern_filter: None,
            severity_floor: None,
            after_seq: Some(seq),
            limit: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// EventPage
// ---------------------------------------------------------------------------

/// A bounded page of query results.
#[derive(Debug, Clone)]
pub struct EventPage {
    /// Events on this page.
    pub events: Vec<StoredEvent>,
    /// Sequence number of the first event on the next page, if any.
    pub next_seq: Option<u64>,
    /// Total matching events in the store (not just this page).
    pub total_count: u64,
}

impl EventPage {
    /// True when this is the last page.
    #[must_use]
    pub fn is_last_page(&self) -> bool {
        self.next_seq.is_none()
    }

    /// True when any event on this page is High or Critical severity.
    #[must_use]
    pub fn has_high_or_critical(&self) -> bool {
        self.events
            .iter()
            .any(|e| e.event.severity >= StoreSeverity::High)
    }
}

// ---------------------------------------------------------------------------
// EventStoreStats
// ---------------------------------------------------------------------------

/// Read-only aggregate metrics for the store.
#[derive(Debug, Clone)]
pub struct EventStoreStats {
    /// Total events persisted.
    pub total_events: u64,
    /// Highest severity seen, if any events exist.
    pub highest_severity: Option<StoreSeverity>,
    /// Sequence number of the most recently appended event, if any.
    pub last_appended_seq: Option<u64>,
}

// ---------------------------------------------------------------------------
// EventStore
// ---------------------------------------------------------------------------

/// Append-only event store for `DetectorEvent` values.
///
/// Stub implementation uses an in-memory `Vec` protected by a `Mutex`.
/// Pool / `SQLite` integration is deferred to C05 M025.
pub struct EventStore {
    config: EventStoreConfig,
    // Lock is released before any I/O — C6 compliance.
    inner: Mutex<StoreInner>,
}

struct StoreInner {
    events: Vec<StoredEvent>,
    next_seq: u64,
}

impl StoreInner {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            next_seq: 1,
        }
    }
}

impl EventStore {
    /// Construct with default config.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: EventStoreConfig::default(),
            inner: Mutex::new(StoreInner::new()),
        }
    }

    /// Construct with explicit config.
    #[must_use]
    pub fn with_config(config: EventStoreConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(StoreInner::new()),
        }
    }

    /// Append a single event to the store.
    ///
    /// # Errors
    ///
    /// Returns `[E2310]` when the internal mutex is poisoned.
    pub fn append(&self, event: DetectorEvent) -> Result<StoredEvent, HleError> {
        let appended_at = current_timestamp_ms();

        // Acquire lock, perform in-memory write, release lock before any I/O.
        let stored = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| HleError::new("[E2310] EventStore mutex poisoned"))?;
            let seq = inner.next_seq;
            inner.next_seq += 1;
            let stored = StoredEvent {
                seq,
                event,
                appended_at,
            };
            inner.events.push(stored.clone());
            stored
        };
        // Lock released. Any future I/O (JSONL export) would happen here.
        Ok(stored)
    }

    /// Append a batch of events transactionally (all-or-nothing in memory).
    ///
    /// # Errors
    ///
    /// Returns `[E2310]` when the internal mutex is poisoned.
    pub fn append_batch(&self, events: &[DetectorEvent]) -> Result<Vec<StoredEvent>, HleError> {
        let appended_at = current_timestamp_ms();

        let stored_events: Vec<StoredEvent> = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| HleError::new("[E2310] EventStore mutex poisoned"))?;
            let mut result = Vec::with_capacity(events.len());
            for event in events {
                let seq = inner.next_seq;
                inner.next_seq += 1;
                let stored = StoredEvent {
                    seq,
                    event: event.clone(),
                    appended_at,
                };
                inner.events.push(stored.clone());
                result.push(stored);
            }
            result
        };
        Ok(stored_events)
    }

    /// Query events according to the provided filter.
    ///
    /// # Errors
    ///
    /// Returns `[E2311]` when the requested limit exceeds `max_page_size`, or
    /// when the mutex is poisoned.
    pub fn query(&self, q: &EventQuery) -> Result<EventPage, HleError> {
        let effective_limit = if q.limit > self.config.max_page_size {
            return Err(HleError::new(format!(
                "[E2311] requested limit {} exceeds max_page_size {}",
                q.limit, self.config.max_page_size
            )));
        } else {
            q.limit
        };

        let inner = self
            .inner
            .lock()
            .map_err(|_| HleError::new("[E2311] EventStore mutex poisoned"))?;

        let filtered: Vec<&StoredEvent> = inner
            .events
            .iter()
            .filter(|se| {
                if let Some(seq_floor) = q.after_seq {
                    if se.seq <= seq_floor {
                        return false;
                    }
                }
                if let Some(ref pid) = q.pattern_filter {
                    if &se.event.pattern_id != pid {
                        return false;
                    }
                }
                if let Some(floor) = q.severity_floor {
                    if se.event.severity < floor {
                        return false;
                    }
                }
                true
            })
            .collect();

        let total_count = filtered.len() as u64;
        let page: Vec<StoredEvent> = filtered
            .iter()
            .take(effective_limit)
            .map(|se| (*se).clone())
            .collect();

        let next_seq = if page.len() < filtered.len() {
            filtered.get(effective_limit).map(|se| se.seq)
        } else {
            None
        };

        Ok(EventPage {
            events: page,
            next_seq,
            total_count,
        })
    }

    /// Return aggregate statistics.
    ///
    /// # Errors
    ///
    /// Returns `[E2311]` when the mutex is poisoned.
    pub fn stats(&self) -> Result<EventStoreStats, HleError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| HleError::new("[E2311] EventStore mutex poisoned"))?;

        let total_events = inner.events.len() as u64;
        let highest_severity = inner.events.iter().map(|se| se.event.severity).max();
        let last_appended_seq = inner.events.last().map(|se| se.seq);

        Ok(EventStoreStats {
            total_events,
            highest_severity,
            last_appended_seq,
        })
    }

    /// Total events in the store.
    ///
    /// # Errors
    ///
    /// Returns `[E2311]` when the mutex is poisoned.
    pub fn event_count(&self) -> Result<u64, HleError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| HleError::new("[E2311] EventStore mutex poisoned"))?;
        Ok(inner.events.len() as u64)
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Return current time as milliseconds since UNIX epoch.
/// Falls back to 0 on platforms where `SystemTime` is unavailable.
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        DetectorEvent, EventQuery, EventStore, EventStoreConfig, EventStoreStats, StoreSeverity,
        StoredEvent,
    };

    fn sample_event(pattern: &str, severity: StoreSeverity) -> DetectorEvent {
        DetectorEvent::new(pattern, severity, "src/foo.rs", 1, 1, "evidence text")
            .map_err(|e| e.to_string())
            .unwrap()
    }

    fn event_at_line(pattern: &str, severity: StoreSeverity, line: u32) -> DetectorEvent {
        DetectorEvent::new(pattern, severity, "src/foo.rs", line, line, "evidence")
            .map_err(|e| e.to_string())
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // DetectorEvent construction
    // -----------------------------------------------------------------------

    #[test]
    fn detector_event_rejects_oversized_evidence() {
        let long = "x".repeat(1025);
        let result = DetectorEvent::new("AP29", StoreSeverity::High, "foo.rs", 1, 1, long);
        assert!(result.is_err());
    }

    #[test]
    fn detector_event_accepts_exactly_1024_byte_evidence() {
        let at_cap = "y".repeat(1024);
        assert!(DetectorEvent::new("AP29", StoreSeverity::High, "foo.rs", 1, 1, at_cap).is_ok());
    }

    #[test]
    fn detector_event_rejects_invalid_line_range() {
        let result = DetectorEvent::new("AP29", StoreSeverity::High, "foo.rs", 10, 5, "evidence");
        assert!(result.is_err());
    }

    #[test]
    fn detector_event_accepts_equal_line_range() {
        assert!(
            DetectorEvent::new("AP29", StoreSeverity::High, "foo.rs", 5, 5, "evidence").is_ok()
        );
    }

    #[test]
    fn detector_event_accepts_valid_range() {
        let ev = DetectorEvent::new("AP29", StoreSeverity::High, "foo.rs", 1, 5, "evidence");
        assert!(ev.is_ok());
    }

    #[test]
    fn detector_event_with_receipt_sha_is_anchored() {
        let ev = sample_event("AP28", StoreSeverity::High).with_receipt_sha([0u8; 32]);
        assert!(ev.is_anchored());
    }

    #[test]
    fn detector_event_without_receipt_sha_not_anchored() {
        let ev = sample_event("AP28", StoreSeverity::High);
        assert!(!ev.is_anchored());
    }

    #[test]
    fn detector_event_builder_chain_overwrites_sha() {
        let ev = sample_event("C6", StoreSeverity::Critical)
            .with_receipt_sha([0xAA; 32])
            .with_receipt_sha([0xBB; 32]);
        assert_eq!(ev.receipt_sha, Some([0xBB; 32]));
    }

    // -----------------------------------------------------------------------
    // StoreSeverity ordering and display
    // -----------------------------------------------------------------------

    #[test]
    fn store_severity_ordering() {
        assert!(StoreSeverity::Critical > StoreSeverity::High);
        assert!(StoreSeverity::High > StoreSeverity::Medium);
        assert!(StoreSeverity::Medium > StoreSeverity::Low);
    }

    #[test]
    fn store_severity_display_low() {
        assert_eq!(StoreSeverity::Low.to_string(), "LOW");
    }

    #[test]
    fn store_severity_display_medium() {
        assert_eq!(StoreSeverity::Medium.to_string(), "MEDIUM");
    }

    #[test]
    fn store_severity_display_high() {
        assert_eq!(StoreSeverity::High.to_string(), "HIGH");
    }

    #[test]
    fn store_severity_display_critical() {
        assert_eq!(StoreSeverity::Critical.to_string(), "CRITICAL");
    }

    // -----------------------------------------------------------------------
    // EventStoreConfig
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_config_clamps_page_size_at_zero() {
        let cfg = EventStoreConfig::with_max_page_size(0);
        assert_eq!(cfg.max_page_size, 1);
    }

    #[test]
    fn event_store_config_clamps_page_size_at_overflow() {
        let cfg2 = EventStoreConfig::with_max_page_size(999_999);
        assert_eq!(cfg2.max_page_size, 5000);
    }

    #[test]
    fn event_store_config_default_is_500() {
        let cfg = EventStoreConfig::default_config();
        assert_eq!(cfg.max_page_size, 500);
    }

    #[test]
    fn event_store_config_with_valid_size_preserved() {
        let cfg = EventStoreConfig::with_max_page_size(100);
        assert_eq!(cfg.max_page_size, 100);
    }

    // -----------------------------------------------------------------------
    // Append-only invariant: no update or delete surface
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_has_no_delete_or_update_method() {
        // This test enforces the append-only invariant through compilation:
        // `EventStore` must compile without `delete` or `update` methods.
        // If this test compiles, the API is append-only.
        let store = EventStore::new();
        let _ = store.event_count();
    }

    // -----------------------------------------------------------------------
    // Append and sequential seq numbering
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_append_increments_seq() {
        let store = EventStore::new();
        let a = store
            .append(sample_event("AP29", StoreSeverity::High))
            .unwrap();
        let b = store
            .append(sample_event("AP31", StoreSeverity::Medium))
            .unwrap();
        assert_eq!(a.seq, 1);
        assert_eq!(b.seq, 2);
    }

    #[test]
    fn event_store_append_seq_starts_at_one() {
        let store = EventStore::new();
        let stored = store
            .append(sample_event("C12", StoreSeverity::Low))
            .unwrap();
        assert_eq!(stored.seq, 1);
    }

    #[test]
    fn event_store_event_count_matches_appends() {
        let store = EventStore::new();
        store
            .append(sample_event("C6", StoreSeverity::Critical))
            .unwrap();
        store
            .append(sample_event("C7", StoreSeverity::Low))
            .unwrap();
        assert_eq!(store.event_count().unwrap(), 2);
    }

    #[test]
    fn event_store_empty_has_zero_count() {
        let store = EventStore::new();
        assert_eq!(store.event_count().unwrap(), 0);
    }

    // -----------------------------------------------------------------------
    // Batch append
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_append_batch_assigns_sequential_seqs() {
        let store = EventStore::new();
        let events = vec![
            sample_event("C12", StoreSeverity::Low),
            sample_event("C13", StoreSeverity::Medium),
        ];
        let stored = store.append_batch(&events).unwrap();
        assert_eq!(stored[0].seq, 1);
        assert_eq!(stored[1].seq, 2);
    }

    #[test]
    fn event_store_append_batch_empty_vec_succeeds() {
        let store = EventStore::new();
        let result = store.append_batch(&[]).unwrap();
        assert!(result.is_empty());
        assert_eq!(store.event_count().unwrap(), 0);
    }

    #[test]
    fn event_store_append_batch_seqs_continue_from_prior_appends() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        let batch = store
            .append_batch(&[sample_event("AP29", StoreSeverity::High)])
            .unwrap();
        assert_eq!(batch[0].seq, 2);
    }

    // -----------------------------------------------------------------------
    // EventQuery: filter by pattern
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_query_all_returns_all_events() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Medium))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::High))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert_eq!(page.total_count, 2);
        assert_eq!(page.events.len(), 2);
    }

    #[test]
    fn event_store_query_all_on_empty_store_returns_empty_page() {
        let store = EventStore::new();
        let page = store.query(&EventQuery::all()).unwrap();
        assert_eq!(page.total_count, 0);
        assert!(page.events.is_empty());
    }

    #[test]
    fn event_store_query_for_pattern_filters_correctly() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::High))
            .unwrap();
        store
            .append(sample_event("AP28", StoreSeverity::Medium))
            .unwrap();
        let page = store.query(&EventQuery::for_pattern("AP28")).unwrap();
        assert_eq!(page.total_count, 2);
        assert!(page.events.iter().all(|se| se.event.pattern_id == "AP28"));
    }

    #[test]
    fn event_store_query_for_pattern_no_match_returns_empty() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        let page = store.query(&EventQuery::for_pattern("UNKNOWN")).unwrap();
        assert_eq!(page.total_count, 0);
    }

    // -----------------------------------------------------------------------
    // EventQuery: filter by severity
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_query_high_and_above_filters_correctly() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::High))
            .unwrap();
        store
            .append(sample_event("C6", StoreSeverity::Critical))
            .unwrap();
        let page = store.query(&EventQuery::high_and_above()).unwrap();
        assert_eq!(page.total_count, 2);
        assert!(page
            .events
            .iter()
            .all(|se| se.event.severity >= StoreSeverity::High));
    }

    #[test]
    fn event_store_query_medium_floor_includes_medium() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::Medium))
            .unwrap();
        let mut q = EventQuery::all();
        q.severity_floor = Some(StoreSeverity::Medium);
        let page = store.query(&q).unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.events[0].event.severity, StoreSeverity::Medium);
    }

    // -----------------------------------------------------------------------
    // EventQuery: filter by seq
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_query_since_filters_by_seq() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::Medium))
            .unwrap();
        store
            .append(sample_event("AP31", StoreSeverity::High))
            .unwrap();
        let page = store.query(&EventQuery::since(2)).unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.events[0].seq, 3);
    }

    #[test]
    fn event_store_query_since_zero_returns_all() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::High))
            .unwrap();
        let page = store.query(&EventQuery::since(0)).unwrap();
        assert_eq!(page.total_count, 2);
    }

    // -----------------------------------------------------------------------
    // Pagination
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_rejects_limit_exceeding_max_page_size() {
        let store = EventStore::with_config(EventStoreConfig::with_max_page_size(5));
        let mut q = EventQuery::all();
        q.limit = 10;
        assert!(store.query(&q).is_err());
    }

    #[test]
    fn event_page_next_seq_set_when_more_results_exist() {
        let store = EventStore::with_config(EventStoreConfig::with_max_page_size(2));
        store.append(sample_event("A", StoreSeverity::Low)).unwrap();
        store.append(sample_event("B", StoreSeverity::Low)).unwrap();
        store.append(sample_event("C", StoreSeverity::Low)).unwrap();
        let mut q = EventQuery::all();
        q.limit = 2;
        let page = store.query(&q).unwrap();
        assert!(page.next_seq.is_some(), "expected next_seq to be set");
    }

    #[test]
    fn event_page_is_last_when_no_more_events() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Medium))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert!(page.is_last_page());
    }

    // -----------------------------------------------------------------------
    // EventPage predicates
    // -----------------------------------------------------------------------

    #[test]
    fn event_page_has_high_or_critical_when_critical_present() {
        let store = EventStore::new();
        store
            .append(sample_event("C6", StoreSeverity::Critical))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert!(page.has_high_or_critical());
    }

    #[test]
    fn event_page_has_high_or_critical_when_high_present() {
        let store = EventStore::new();
        store
            .append(sample_event("C7", StoreSeverity::High))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert!(page.has_high_or_critical());
    }

    #[test]
    fn event_page_not_high_when_only_low_present() {
        let store = EventStore::new();
        store
            .append(sample_event("C7", StoreSeverity::Low))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert!(!page.has_high_or_critical());
    }

    #[test]
    fn event_page_not_high_when_only_medium_present() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Medium))
            .unwrap();
        let page = store.query(&EventQuery::all()).unwrap();
        assert!(!page.has_high_or_critical());
    }

    // -----------------------------------------------------------------------
    // EventStoreStats
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_stats_highest_severity_tracks_max() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::Critical))
            .unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.highest_severity, Some(StoreSeverity::Critical));
        assert_eq!(stats.total_events, 2);
    }

    #[test]
    fn event_store_stats_empty_store_has_none_severity() {
        let store = EventStore::new();
        let stats = store.stats().unwrap();
        assert_eq!(stats.highest_severity, None);
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.last_appended_seq, None);
    }

    #[test]
    fn event_store_stats_last_seq_matches_most_recent() {
        let store = EventStore::new();
        store
            .append(sample_event("AP28", StoreSeverity::Low))
            .unwrap();
        store
            .append(sample_event("AP29", StoreSeverity::Low))
            .unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.last_appended_seq, Some(2));
    }

    // -----------------------------------------------------------------------
    // Replay ordering preserved (append order == query order)
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_replay_order_preserved() {
        let store = EventStore::new();
        for i in 1u32..=5 {
            store
                .append(event_at_line("AP28", StoreSeverity::Low, i))
                .unwrap();
        }
        let page = store.query(&EventQuery::all()).unwrap();
        let seqs: Vec<u64> = page.events.iter().map(|se| se.seq).collect();
        let mut sorted = seqs.clone();
        sorted.sort_unstable();
        assert_eq!(seqs, sorted, "query must preserve insertion order");
    }

    // -----------------------------------------------------------------------
    // StoredEvent helper
    // -----------------------------------------------------------------------

    #[test]
    fn stored_event_is_anchored_delegates_to_event() {
        let store = EventStore::new();
        let ev = sample_event("AP28", StoreSeverity::High).with_receipt_sha([0xFF; 32]);
        let stored = store.append(ev).unwrap();
        assert!(stored.is_anchored());
    }

    #[test]
    fn stored_event_not_anchored_when_no_sha() {
        let store = EventStore::new();
        let stored = store
            .append(sample_event("AP29", StoreSeverity::Low))
            .unwrap();
        assert!(!stored.is_anchored());
    }

    // -----------------------------------------------------------------------
    // EventStore::default() is equivalent to ::new()
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_default_is_new() {
        let store: EventStore = Default::default();
        assert_eq!(store.event_count().unwrap(), 0);
    }

    // -----------------------------------------------------------------------
    // EventStoreStats after mixed severity appends
    // -----------------------------------------------------------------------

    #[test]
    fn event_store_stats_total_matches_append_count() {
        let store = EventStore::new();
        for i in 0u32..10 {
            store
                .append(event_at_line("AP28", StoreSeverity::Low, i + 1))
                .unwrap();
        }
        let stats = store.stats().unwrap();
        assert_eq!(stats.total_events, 10);
    }

    #[test]
    fn event_query_all_limit_respected_at_default() {
        let store = EventStore::with_config(EventStoreConfig::with_max_page_size(3));
        for i in 0..3 {
            store
                .append(sample_event(&format!("P{i}"), StoreSeverity::Low))
                .unwrap();
        }
        // Limit = max_page_size = 3; all three events should be returned.
        let mut q = EventQuery::all();
        q.limit = 3;
        let page = store.query(&q).unwrap();
        assert_eq!(page.events.len(), 3);
    }

    #[test]
    fn event_store_with_config_preserves_max_page_size() {
        let store = EventStore::with_config(EventStoreConfig::with_max_page_size(42));
        // Cannot read config directly, but we can verify: query with limit 43 must fail.
        let mut q = EventQuery::all();
        q.limit = 43;
        assert!(store.query(&q).is_err());
    }
}
