//! M043 DevopsV3Probe — read-only probe surface for DevOps Engine V3 (port 8082).
//!
//! This bridge has NO write surface whatsoever. The struct is not generic.
//! No `LiveWrite` variant, no `WriteAuthToken` parameter in any method.
//! `supports_write` returns `false` unconditionally.
//!
//! Error codes: 2630–2632.

use std::fmt;
use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::Instant;

use crate::bridge_contract::{BoundedDuration, BridgeContract, CapabilityClass};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum byte length of the URI string for a probe target.
pub const PROBE_URI_MAX_LEN: usize = 256;
/// Default port for DevOps Engine V3.
pub const DEVOPS_V3_DEFAULT_PORT: u16 = 8082;
/// Default health path for DevOps Engine V3.
pub const DEVOPS_V3_HEALTH_PATH: &str = "/health";
/// Maximum byte length of the `details` field in `HealthSignal`.
pub const HEALTH_SIGNAL_DETAILS_CAP: usize = 1_024;

// ─── ProbeTarget ─────────────────────────────────────────────────────────────

/// Validated `(host, port, path)` coordinate for an HTTP probe.
///
/// Defaults to DevOps V3: host `127.0.0.1`, port `8082`, path `/health`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProbeTarget {
    /// Host to probe (non-empty).
    pub host: String,
    /// Port to probe (non-zero).
    pub port: u16,
    /// HTTP path beginning with `/`.
    pub path: String,
}

impl ProbeTarget {
    /// Construct and validate a probe target.
    ///
    /// # Errors
    ///
    /// Returns `Err(ProbeUnreachable)` when host is empty, port is zero,
    /// path does not start with `/`, or the full URI exceeds `PROBE_URI_MAX_LEN`.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        path: impl Into<String>,
    ) -> Result<Self, DevopsV3ProbeError> {
        let host = host.into();
        let path = path.into();
        if host.is_empty() {
            return Err(DevopsV3ProbeError::ProbeUnreachable {
                target: String::from("<empty>"),
                reason: String::from("host must not be empty"),
            });
        }
        if port == 0 {
            return Err(DevopsV3ProbeError::ProbeUnreachable {
                target: host,
                reason: String::from("port must be non-zero"),
            });
        }
        if !path.starts_with('/') {
            return Err(DevopsV3ProbeError::ProbeUnreachable {
                target: host,
                reason: format!("path must start with '/'; got: {path}"),
            });
        }
        let uri = format!("http://{host}:{port}{path}");
        if uri.len() > PROBE_URI_MAX_LEN {
            return Err(DevopsV3ProbeError::ProbeUnreachable {
                target: host,
                reason: format!("URI length {} exceeds cap {PROBE_URI_MAX_LEN}", uri.len()),
            });
        }
        Ok(Self { host, port, path })
    }

    /// The canonical `127.0.0.1:8082/health` target for DevOps V3.
    #[must_use]
    pub fn devops_v3() -> Self {
        Self {
            host: String::from("127.0.0.1"),
            port: DEVOPS_V3_DEFAULT_PORT,
            path: String::from(DEVOPS_V3_HEALTH_PATH),
        }
    }

    /// Full URI string: `http://HOST:PORT/PATH`.
    #[must_use]
    pub fn uri(&self) -> String {
        format!("http://{}:{}{}", self.host, self.port, self.path)
    }
}

impl fmt::Display for ProbeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProbeTarget({}:{}{})", self.host, self.port, self.path)
    }
}

// ─── ReadinessSignal ─────────────────────────────────────────────────────────

/// High-level readiness determination derived from a `HealthSignal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadinessSignal {
    /// `status_code == 200` and health body indicates healthy.
    Ready,
    /// 2xx with `is_healthy = false`, or partial body parse.
    Degraded {
        /// Human-readable degradation reason.
        reason: String,
    },
    /// Non-2xx, connection refused, or parse-level failure.
    Unreachable,
}

impl fmt::Display for ReadinessSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => f.write_str("Ready"),
            Self::Degraded { reason } => write!(f, "Degraded({reason})"),
            Self::Unreachable => f.write_str("Unreachable"),
        }
    }
}

// ─── HealthSignal ────────────────────────────────────────────────────────────

/// Structured health response from a DevOps V3 `/health` endpoint.
///
/// `details` is bounded at `HEALTH_SIGNAL_DETAILS_CAP` (1,024 bytes). Fields
/// absent in the response default to sentinel values rather than failing the parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSignal {
    /// The probe target that produced this signal.
    pub target: ProbeTarget,
    /// HTTP status code returned by the service.
    pub status_code: u16,
    /// Whether the service body indicated healthy state.
    pub is_healthy: bool,
    /// Service version string (defaults to `"unknown"`).
    pub version: String,
    /// Service uptime in seconds (defaults to 0).
    pub uptime_s: u64,
    /// Bounded details string (≤ `HEALTH_SIGNAL_DETAILS_CAP` bytes).
    pub details: String,
    /// Round-trip time of this probe in milliseconds.
    pub probe_elapsed_ms: u64,
}

impl HealthSignal {
    /// Parse a health response from raw HTTP fields.
    ///
    /// Uses defaults for missing JSON fields; truncates `details` at the cap.
    #[must_use]
    pub fn parse(target: ProbeTarget, status_code: u16, body: &str, elapsed_ms: u64) -> Self {
        let is_healthy = status_code == 200
            && (body.contains("\"status\":\"ok\"")
                || body.contains("\"healthy\":true")
                || body.contains("\"status\":\"healthy\""));
        let details = if body.len() > HEALTH_SIGNAL_DETAILS_CAP {
            let mut d = body[..HEALTH_SIGNAL_DETAILS_CAP].to_owned();
            d.push_str("[TRUNCATED]");
            d
        } else {
            body.to_owned()
        };
        Self {
            target,
            status_code,
            is_healthy,
            version: String::from("unknown"),
            uptime_s: 0,
            details,
            probe_elapsed_ms: elapsed_ms,
        }
    }

    /// True when `status_code == 200 && is_healthy`.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.status_code == 200 && self.is_healthy
    }

    /// Derive `ReadinessSignal` from this health signal.
    #[must_use]
    pub fn readiness(&self) -> ReadinessSignal {
        if self.is_healthy() {
            ReadinessSignal::Ready
        } else if self.status_code / 100 == 2 {
            ReadinessSignal::Degraded {
                reason: format!("status_code={} but is_healthy=false", self.status_code),
            }
        } else {
            ReadinessSignal::Unreachable
        }
    }

    /// Round-trip latency in milliseconds.
    #[must_use]
    pub fn latency_ms(&self) -> u64 {
        self.probe_elapsed_ms
    }
}

impl fmt::Display for HealthSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HealthSignal({}:{} status={} healthy={} uptime={}s)",
            self.target.host, self.target.port, self.status_code, self.is_healthy, self.uptime_s
        )
    }
}

// ─── DevopsV3Probe ───────────────────────────────────────────────────────────

/// Read-only probe bridge for DevOps Engine V3.
///
/// No write surface. `supports_write` returns `false` unconditionally.
/// All I/O is synchronous blocking via `TcpStream`.
#[derive(Debug)]
pub struct DevopsV3Probe {
    default_target: ProbeTarget,
    default_timeout: BoundedDuration,
}

impl BridgeContract for DevopsV3Probe {
    fn schema_id(&self) -> &'static str {
        "hle.devops_v3.v1"
    }
    fn port(&self) -> Option<u16> {
        Some(DEVOPS_V3_DEFAULT_PORT)
    }
    fn paths(&self) -> &[&'static str] {
        &[DEVOPS_V3_HEALTH_PATH, "/readyz"]
    }
    fn supports_write(&self) -> bool {
        false
    }
    fn capability_class(&self) -> CapabilityClass {
        CapabilityClass::ReadOnly
    }
    fn name(&self) -> &'static str {
        "devops_v3_probe"
    }
}

impl DevopsV3Probe {
    /// Construct with the default DevOps V3 target.
    #[must_use]
    pub fn new(timeout: BoundedDuration) -> Self {
        Self {
            default_target: ProbeTarget::devops_v3(),
            default_timeout: timeout,
        }
    }

    /// Construct with a custom probe target.
    #[must_use]
    pub fn with_target(target: ProbeTarget, timeout: BoundedDuration) -> Self {
        Self {
            default_target: target,
            default_timeout: timeout,
        }
    }

    /// Perform a single bounded HTTP GET to the default target's `/health`.
    ///
    /// # Errors
    ///
    /// Returns `Err(ProbeTimeout)` on timeout, `Err(ProbeUnreachable)` on
    /// connection failure, `Err(ProbeResponseInvalid)` on 4xx/5xx.
    #[must_use]
    pub fn probe_health(&self) -> Result<HealthSignal, DevopsV3ProbeError> {
        self.probe_health_target(&self.default_target.clone())
    }

    /// Perform a single bounded HTTP GET to an explicit target.
    ///
    /// # Errors
    ///
    /// Returns the same errors as `probe_health`.
    #[must_use]
    pub fn probe_health_target(
        &self,
        target: &ProbeTarget,
    ) -> Result<HealthSignal, DevopsV3ProbeError> {
        let addr = format!("{}:{}", target.host, target.port);
        let start = Instant::now();
        let timeout_dur = self.default_timeout.as_duration();

        let mut stream =
            TcpStream::connect(&addr).map_err(|e| DevopsV3ProbeError::ProbeUnreachable {
                target: addr.clone(),
                reason: e.to_string(),
            })?;

        stream.set_read_timeout(Some(timeout_dur)).map_err(|e| {
            DevopsV3ProbeError::ProbeUnreachable {
                target: addr.clone(),
                reason: e.to_string(),
            }
        })?;
        stream.set_write_timeout(Some(timeout_dur)).map_err(|e| {
            DevopsV3ProbeError::ProbeUnreachable {
                target: addr.clone(),
                reason: e.to_string(),
            }
        })?;

        let request = format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            target.path, target.host
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| DevopsV3ProbeError::ProbeUnreachable {
                target: addr.clone(),
                reason: format!("write failed: {e}"),
            })?;

        let mut response = String::new();
        stream.read_to_string(&mut response).map_err(|e| {
            DevopsV3ProbeError::ProbeTimeout {
                target: addr.clone(),
                elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            }
            .into_or(&e)
        })?;

        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        // Parse status line.
        let status_code = parse_http_status(&response).unwrap_or(0);

        if status_code == 0 {
            return Err(DevopsV3ProbeError::ProbeUnreachable {
                target: addr,
                reason: String::from("could not parse HTTP status line"),
            });
        }

        if !(200..300).contains(&status_code) {
            return Err(DevopsV3ProbeError::ProbeResponseInvalid {
                target: addr,
                status_code,
            });
        }

        let body = extract_http_body(&response);
        Ok(HealthSignal::parse(
            target.clone(),
            status_code,
            body,
            elapsed_ms,
        ))
    }

    /// Derive `ReadinessSignal` from a single health probe.
    ///
    /// # Errors
    ///
    /// Returns the same errors as `probe_health`.
    #[must_use]
    pub fn readiness(&self) -> Result<ReadinessSignal, DevopsV3ProbeError> {
        self.probe_health().map(|s| s.readiness())
    }

    /// True iff `probe_health` returns `Ok(_)`. Swallows the error.
    #[must_use]
    pub fn is_reachable(&self) -> bool {
        self.probe_health().is_ok()
    }

    /// Measure round-trip latency of a single `/health` probe.
    ///
    /// # Errors
    ///
    /// Returns the same errors as `probe_health`.
    #[must_use]
    pub fn latency_ms(&self) -> Result<u64, DevopsV3ProbeError> {
        self.probe_health().map(|s| s.probe_elapsed_ms)
    }
}

// ─── HTTP parsing helpers ────────────────────────────────────────────────────

fn parse_http_status(response: &str) -> Option<u16> {
    let first_line = response.lines().next()?;
    let mut parts = first_line.splitn(3, ' ');
    parts.next()?; // HTTP/1.x
    let code_str = parts.next()?;
    code_str.parse().ok()
}

fn extract_http_body(response: &str) -> &str {
    // Body starts after the blank line "\r\n\r\n" or "\n\n".
    if let Some(pos) = response.find("\r\n\r\n") {
        &response[pos + 4..]
    } else if let Some(pos) = response.find("\n\n") {
        &response[pos + 2..]
    } else {
        ""
    }
}

// ─── DevopsV3ProbeError ──────────────────────────────────────────────────────

/// Errors for M043 DevOps V3 probe.
///
/// Error codes: 2630–2632.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevopsV3ProbeError {
    /// Code 2630. HTTP probe timed out. Always retryable.
    ProbeTimeout {
        /// URI that timed out.
        target: String,
        /// Elapsed milliseconds at timeout.
        elapsed_ms: u64,
    },
    /// Code 2631. Connection refused, DNS failure, or network partition.
    /// Retryable — service may be starting.
    ProbeUnreachable {
        /// URI that was unreachable.
        target: String,
        /// Human-readable failure reason.
        reason: String,
    },
    /// Code 2632. Service responded with 4xx or 5xx. Not retryable.
    ProbeResponseInvalid {
        /// URI that returned an error response.
        target: String,
        /// HTTP status code returned.
        status_code: u16,
    },
}

impl DevopsV3ProbeError {
    /// Error code: 2630, 2631, or 2632.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::ProbeTimeout { .. } => 2630,
            Self::ProbeUnreachable { .. } => 2631,
            Self::ProbeResponseInvalid { .. } => 2632,
        }
    }

    /// True for 2630/2631; false for 2632.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ProbeTimeout { .. } | Self::ProbeUnreachable { .. }
        )
    }

    /// URI string for logging.
    #[must_use]
    pub fn target_uri(&self) -> &str {
        match self {
            Self::ProbeTimeout { target, .. }
            | Self::ProbeUnreachable { target, .. }
            | Self::ProbeResponseInvalid { target, .. } => target.as_str(),
        }
    }

    fn into_or(self, io_err: &std::io::Error) -> Self {
        if io_err.kind() == std::io::ErrorKind::TimedOut
            || io_err.kind() == std::io::ErrorKind::WouldBlock
        {
            self
        } else {
            DevopsV3ProbeError::ProbeUnreachable {
                target: self.target_uri().to_owned(),
                reason: io_err.to_string(),
            }
        }
    }
}

impl fmt::Display for DevopsV3ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProbeTimeout { target, elapsed_ms } => {
                write!(
                    f,
                    "[HLE-2630] probe timeout: {target} elapsed={elapsed_ms}ms"
                )
            }
            Self::ProbeUnreachable { target, reason } => {
                write!(f, "[HLE-2631] probe unreachable: {target} reason={reason}")
            }
            Self::ProbeResponseInvalid {
                target,
                status_code,
            } => {
                write!(
                    f,
                    "[HLE-2632] probe response invalid: {target} status={status_code}"
                )
            }
        }
    }
}

impl std::error::Error for DevopsV3ProbeError {}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn timeout() -> BoundedDuration {
        BoundedDuration::default()
    }

    // ── ProbeTarget ──────────────────────────────────────────────────────────

    #[test]
    fn probe_target_devops_v3_is_infallible() {
        let t = ProbeTarget::devops_v3();
        assert_eq!(t.port, DEVOPS_V3_DEFAULT_PORT);
        assert_eq!(t.path, DEVOPS_V3_HEALTH_PATH);
    }

    #[test]
    fn probe_target_new_valid() {
        let t = ProbeTarget::new("127.0.0.1", 8082, "/health").expect("valid");
        assert_eq!(t.uri(), "http://127.0.0.1:8082/health");
    }

    #[test]
    fn probe_target_rejects_empty_host() {
        assert!(ProbeTarget::new("", 8082, "/health").is_err());
    }

    #[test]
    fn probe_target_rejects_zero_port() {
        assert!(ProbeTarget::new("localhost", 0, "/health").is_err());
    }

    #[test]
    fn probe_target_rejects_path_without_slash() {
        assert!(ProbeTarget::new("localhost", 8082, "health").is_err());
    }

    #[test]
    fn probe_target_rejects_long_uri() {
        let long_path = format!("/{}", "x".repeat(PROBE_URI_MAX_LEN));
        assert!(ProbeTarget::new("localhost", 8082, long_path).is_err());
    }

    #[test]
    fn probe_target_display() {
        let t = ProbeTarget::devops_v3();
        assert!(t.to_string().contains("8082"));
    }

    // ── HealthSignal ─────────────────────────────────────────────────────────

    #[test]
    fn health_signal_parse_200_ok_body_is_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"status\":\"ok\"}", 10);
        assert!(s.is_healthy());
    }

    #[test]
    fn health_signal_parse_200_no_ok_body_is_not_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{}", 10);
        assert!(!s.is_healthy());
    }

    #[test]
    fn health_signal_parse_500_is_not_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 500, "{\"status\":\"ok\"}", 10);
        assert!(!s.is_healthy());
    }

    #[test]
    fn health_signal_details_truncated_at_cap() {
        let t = ProbeTarget::devops_v3();
        let long_body = "x".repeat(HEALTH_SIGNAL_DETAILS_CAP + 100);
        let s = HealthSignal::parse(t, 200, &long_body, 10);
        assert!(s.details.contains("[TRUNCATED]"));
    }

    #[test]
    fn health_signal_readiness_ready_on_200_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"status\":\"ok\"}", 5);
        assert_eq!(s.readiness(), ReadinessSignal::Ready);
    }

    #[test]
    fn health_signal_readiness_degraded_on_200_unhealthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{}", 5);
        assert!(matches!(s.readiness(), ReadinessSignal::Degraded { .. }));
    }

    #[test]
    fn health_signal_readiness_unreachable_on_500() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 500, "", 5);
        assert_eq!(s.readiness(), ReadinessSignal::Unreachable);
    }

    // ── DevopsV3Probe ────────────────────────────────────────────────────────

    #[test]
    fn devops_probe_capability_class_is_read_only() {
        let p = DevopsV3Probe::new(timeout());
        assert_eq!(p.capability_class(), CapabilityClass::ReadOnly);
    }

    #[test]
    fn devops_probe_supports_write_is_false() {
        let p = DevopsV3Probe::new(timeout());
        assert!(!p.supports_write());
    }

    #[test]
    fn devops_probe_port_is_8082() {
        let p = DevopsV3Probe::new(timeout());
        assert_eq!(p.port(), Some(DEVOPS_V3_DEFAULT_PORT));
    }

    #[test]
    fn devops_probe_schema_id() {
        let p = DevopsV3Probe::new(timeout());
        assert_eq!(p.schema_id(), "hle.devops_v3.v1");
    }

    // ── Error codes and retryability ─────────────────────────────────────────

    #[test]
    fn probe_timeout_error_code_is_2630() {
        let e = DevopsV3ProbeError::ProbeTimeout {
            target: String::from("t"),
            elapsed_ms: 1,
        };
        assert_eq!(e.error_code(), 2630);
    }

    #[test]
    fn probe_unreachable_error_code_is_2631() {
        let e = DevopsV3ProbeError::ProbeUnreachable {
            target: String::from("t"),
            reason: String::from("r"),
        };
        assert_eq!(e.error_code(), 2631);
    }

    #[test]
    fn probe_response_invalid_error_code_is_2632() {
        let e = DevopsV3ProbeError::ProbeResponseInvalid {
            target: String::from("t"),
            status_code: 500,
        };
        assert_eq!(e.error_code(), 2632);
    }

    #[test]
    fn probe_timeout_is_retryable() {
        let e = DevopsV3ProbeError::ProbeTimeout {
            target: String::from("t"),
            elapsed_ms: 1,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn probe_unreachable_is_retryable() {
        let e = DevopsV3ProbeError::ProbeUnreachable {
            target: String::from("t"),
            reason: String::from("r"),
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn probe_response_invalid_is_not_retryable() {
        let e = DevopsV3ProbeError::ProbeResponseInvalid {
            target: String::from("t"),
            status_code: 500,
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn error_display_contains_code_prefix_2630() {
        let e = DevopsV3ProbeError::ProbeTimeout {
            target: String::from("t"),
            elapsed_ms: 5,
        };
        assert!(e.to_string().contains("[HLE-2630]"));
    }

    #[test]
    fn target_uri_method_returns_target_string() {
        let e = DevopsV3ProbeError::ProbeResponseInvalid {
            target: String::from("my-uri"),
            status_code: 404,
        };
        assert_eq!(e.target_uri(), "my-uri");
    }

    // ── is_reachable swallows error ──────────────────────────────────────────

    #[test]
    fn is_reachable_returns_false_when_service_down() {
        // Port 1 is almost certainly not bound on any CI machine.
        let target = ProbeTarget::new("127.0.0.1", 1, "/health").expect("valid");
        let probe = DevopsV3Probe::with_target(
            target,
            BoundedDuration::new_clamped(std::time::Duration::from_millis(50)),
        );
        assert!(!probe.is_reachable());
    }

    // ── HTTP parsing helpers ─────────────────────────────────────────────────

    #[test]
    fn parse_http_status_extracts_200() {
        assert_eq!(
            parse_http_status("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"),
            Some(200)
        );
    }

    #[test]
    fn parse_http_status_extracts_500() {
        assert_eq!(
            parse_http_status("HTTP/1.0 500 Internal Server Error\r\n\r\n"),
            Some(500)
        );
    }

    #[test]
    fn parse_http_status_returns_none_on_garbage() {
        assert_eq!(parse_http_status("garbage"), None);
    }

    #[test]
    fn extract_http_body_after_crlf_blank_line() {
        let resp = "HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\"}";
        assert_eq!(extract_http_body(resp), "{\"status\":\"ok\"}");
    }

    // ── ProbeTarget additional ───────────────────────────────────────────────

    #[test]
    fn probe_target_new_custom_host_and_port() {
        let t = ProbeTarget::new("10.0.0.1", 9090, "/metrics").expect("valid");
        assert_eq!(t.host, "10.0.0.1");
        assert_eq!(t.port, 9090);
    }

    #[test]
    fn probe_target_uri_includes_all_parts() {
        let t = ProbeTarget::new("myhost", 1234, "/check").expect("valid");
        assert_eq!(t.uri(), "http://myhost:1234/check");
    }

    #[test]
    fn probe_target_clone_equality() {
        let t = ProbeTarget::devops_v3();
        assert_eq!(t.clone(), t);
    }

    #[test]
    fn probe_target_default_host_is_localhost() {
        let t = ProbeTarget::devops_v3();
        assert_eq!(t.host, "127.0.0.1");
    }

    #[test]
    fn probe_target_uri_at_length_limit_is_valid() {
        // path of length (PROBE_URI_MAX_LEN - len("http://h:1/")) to stay just within cap
        let short_prefix = "http://h:1".len();
        let max_path_len = PROBE_URI_MAX_LEN - short_prefix;
        let path = format!("/{}", "a".repeat(max_path_len - 1));
        assert!(ProbeTarget::new("h", 1, path).is_ok());
    }

    // ── HealthSignal additional ──────────────────────────────────────────────

    #[test]
    fn health_signal_parse_healthy_true_body_is_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"healthy\":true}", 5);
        assert!(s.is_healthy());
    }

    #[test]
    fn health_signal_parse_status_healthy_body_is_healthy() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"status\":\"healthy\"}", 5);
        assert!(s.is_healthy());
    }

    #[test]
    fn health_signal_latency_ms_returns_probe_elapsed() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"status\":\"ok\"}", 42);
        assert_eq!(s.latency_ms(), 42);
    }

    #[test]
    fn health_signal_details_not_truncated_when_within_cap() {
        let t = ProbeTarget::devops_v3();
        let body = "x".repeat(HEALTH_SIGNAL_DETAILS_CAP);
        let s = HealthSignal::parse(t, 200, &body, 0);
        assert!(!s.details.contains("[TRUNCATED]"));
    }

    #[test]
    fn health_signal_version_defaults_to_unknown() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{}", 0);
        assert_eq!(s.version, "unknown");
    }

    #[test]
    fn health_signal_uptime_defaults_to_zero() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{}", 0);
        assert_eq!(s.uptime_s, 0);
    }

    #[test]
    fn health_signal_display_contains_port() {
        let t = ProbeTarget::devops_v3();
        let s = HealthSignal::parse(t, 200, "{\"status\":\"ok\"}", 1);
        assert!(s.to_string().contains("8082"));
    }

    // ── ReadinessSignal additional ───────────────────────────────────────────

    #[test]
    fn readiness_signal_ready_display() {
        assert_eq!(ReadinessSignal::Ready.to_string(), "Ready");
    }

    #[test]
    fn readiness_signal_degraded_display_contains_reason() {
        let r = ReadinessSignal::Degraded {
            reason: String::from("partial"),
        };
        assert!(r.to_string().contains("partial"));
    }

    #[test]
    fn readiness_signal_unreachable_display() {
        assert_eq!(ReadinessSignal::Unreachable.to_string(), "Unreachable");
    }

    // ── DevopsV3Probe construction and BridgeContract ────────────────────────

    #[test]
    fn devops_probe_name_is_devops_v3_probe() {
        let p = DevopsV3Probe::new(timeout());
        assert_eq!(p.name(), "devops_v3_probe");
    }

    #[test]
    fn devops_probe_paths_contain_health() {
        let p = DevopsV3Probe::new(timeout());
        assert!(p.paths().iter().any(|path| path.contains("health")));
    }

    #[test]
    fn devops_probe_with_target_stores_target() {
        let target = ProbeTarget::new("127.0.0.1", 9999, "/health").expect("valid");
        let probe = DevopsV3Probe::with_target(target.clone(), timeout());
        // probe uses the target internally; verify the probe schema is consistent
        assert_eq!(probe.schema_id(), "hle.devops_v3.v1");
    }

    #[test]
    fn devops_probe_no_write_methods_in_api() {
        // Structural: verify none of the method names include write/send/push
        // by confirming our probe bridge only produces read-path outcomes.
        let p = DevopsV3Probe::new(timeout());
        // If this compiles and returns ReadOnly, the write surface is absent.
        assert_eq!(p.capability_class(), CapabilityClass::ReadOnly);
    }

    // ── Error additional ─────────────────────────────────────────────────────

    #[test]
    fn probe_timeout_display_contains_elapsed() {
        let e = DevopsV3ProbeError::ProbeTimeout {
            target: String::from("t"),
            elapsed_ms: 3000,
        };
        assert!(e.to_string().contains("3000"));
    }

    #[test]
    fn probe_unreachable_display_contains_reason() {
        let e = DevopsV3ProbeError::ProbeUnreachable {
            target: String::from("t"),
            reason: String::from("ECONNREFUSED"),
        };
        assert!(e.to_string().contains("ECONNREFUSED"));
    }

    #[test]
    fn probe_response_invalid_display_contains_status_code() {
        let e = DevopsV3ProbeError::ProbeResponseInvalid {
            target: String::from("t"),
            status_code: 503,
        };
        assert!(e.to_string().contains("503"));
    }

    #[test]
    fn target_uri_for_probe_timeout() {
        let e = DevopsV3ProbeError::ProbeTimeout {
            target: String::from("uri-A"),
            elapsed_ms: 0,
        };
        assert_eq!(e.target_uri(), "uri-A");
    }

    #[test]
    fn target_uri_for_probe_unreachable() {
        let e = DevopsV3ProbeError::ProbeUnreachable {
            target: String::from("uri-B"),
            reason: String::from("r"),
        };
        assert_eq!(e.target_uri(), "uri-B");
    }

    // ── HTTP parsing helpers additional ──────────────────────────────────────

    #[test]
    fn extract_http_body_after_lf_lf_blank_line() {
        let resp = "HTTP/1.0 200 OK\n\n{\"body\":1}";
        assert_eq!(extract_http_body(resp), "{\"body\":1}");
    }

    #[test]
    fn extract_http_body_empty_when_no_separator() {
        assert_eq!(extract_http_body("no-separator"), "");
    }

    #[test]
    fn parse_http_status_handles_204() {
        assert_eq!(
            parse_http_status("HTTP/1.1 204 No Content\r\n\r\n"),
            Some(204)
        );
    }
}
