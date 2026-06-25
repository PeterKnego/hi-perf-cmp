//! Benchmark configuration parsed from environment variables.
//!
//! Numeric values must be positive integers; invalid input produces an `Err`
//! describing the offending variable. The run mode (`RTT_MODE`) selects between
//! the in-process loopback benchmark and the cross-host server/client roles.
//!
//! Every experiment artifact shares this contract; each only needs its own
//! port, but all ports are parsed and exposed so the type is uniform.

use std::env;

/// Which role this process plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// In-process echo responder + client on an ephemeral loopback port (default).
    Loopback,
    /// Long-lived echo responder bound to `0.0.0.0` (no stdout).
    Server,
    /// Connect to a remote responder, measure, emit the result lines.
    Client,
}

/// Parsed, validated benchmark configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Which role to run.
    pub mode: Mode,
    /// Responder host/IP (required in client mode).
    pub host: Option<String>,
    /// TCP echo port (server binds / client dials).
    pub tcp_port: u16,
    /// UDP echo port (server binds / client dials).
    pub udp_port: u16,
    /// QUIC echo port (server binds / client dials).
    pub quic_port: u16,
    /// Payload size per request, in bytes.
    pub payload_bytes: usize,
    /// Number of discarded warmup round trips.
    pub warmup: usize,
    /// Number of measured round trips (== sample count).
    pub iterations: usize,
}

impl Config {
    /// Read configuration from the environment, applying defaults and
    /// validating every value.
    pub fn from_env() -> Result<Config, String> {
        let mode = parse_mode("RTT_MODE")?;
        let host = match env::var("RTT_HOST") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Err(_) => None,
        };
        let tcp_port = parse_port("RTT_TCP_PORT", 9100)?;
        let udp_port = parse_port("RTT_UDP_PORT", 9101)?;
        let quic_port = parse_port("RTT_QUIC_PORT", 9102)?;
        let payload_bytes = parse_positive("RTT_PAYLOAD_BYTES", 64)?;
        let warmup = parse_positive("RTT_WARMUP", 10_000)?;
        let iterations = parse_positive("RTT_ITERATIONS", 100_000)?;

        if mode == Mode::Client && host.is_none() {
            return Err("RTT_HOST: required in client mode (set RTT_HOST=<responder>)".to_string());
        }

        Ok(Config {
            mode,
            host,
            tcp_port,
            udp_port,
            quic_port,
            payload_bytes,
            warmup,
            iterations,
        })
    }

    /// The host required in client mode, or an `Err` describing the omission.
    pub fn require_host(&self) -> Result<&str, String> {
        self.host
            .as_deref()
            .ok_or_else(|| "RTT_HOST: required in client mode".to_string())
    }
}

/// Parse the `RTT_MODE` selector, defaulting to loopback when unset.
fn parse_mode(name: &str) -> Result<Mode, String> {
    match env::var(name) {
        Err(_) => Ok(Mode::Loopback),
        Ok(raw) => match raw.trim() {
            "" | "loopback" => Ok(Mode::Loopback),
            "server" => Ok(Mode::Server),
            "client" => Ok(Mode::Client),
            other => Err(format!(
                "{name}: invalid value {other:?} (expected loopback, server, or client)"
            )),
        },
    }
}

/// Parse a port env var, falling back to `default` when unset.
fn parse_port(name: &str, default: u16) -> Result<u16, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let trimmed = raw.trim();
            let value: u16 = trimmed
                .parse()
                .map_err(|_| format!("{name}: invalid value {raw:?} (expected a port 1-65535)"))?;
            if value == 0 {
                return Err(format!("{name}: must be a non-zero port, got 0"));
            }
            Ok(value)
        }
    }
}

/// Parse a positive-integer env var, falling back to `default` when unset.
fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let trimmed = raw.trim();
            let value: usize = trimmed.parse().map_err(|_| {
                format!("{name}: invalid value {raw:?} (expected a positive integer)")
            })?;
            if value == 0 {
                return Err(format!("{name}: must be positive, got 0"));
            }
            Ok(value)
        }
    }
}
