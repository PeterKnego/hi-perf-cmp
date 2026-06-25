//! Benchmark configuration parsed from environment variables.
//!
//! All values must be positive integers; invalid input produces an `Err`
//! describing the offending variable.

use std::env;

/// Parsed, validated benchmark configuration.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Payload size per request, in bytes.
    pub payload_bytes: usize,
    /// Number of discarded warmup round trips.
    pub warmup: usize,
    /// Number of measured round trips (== sample count).
    pub iterations: usize,
}

impl Config {
    /// Read configuration from the environment, applying defaults and
    /// validating that every value is a positive integer.
    pub fn from_env() -> Result<Config, String> {
        let payload_bytes = parse_positive("RTT_PAYLOAD_BYTES", 64)?;
        let warmup = parse_positive("RTT_WARMUP", 10_000)?;
        let iterations = parse_positive("RTT_ITERATIONS", 100_000)?;
        Ok(Config {
            payload_bytes,
            warmup,
            iterations,
        })
    }
}

/// Parse a positive-integer env var, falling back to `default` when unset.
fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let trimmed = raw.trim();
            let value: usize = trimmed
                .parse()
                .map_err(|_| format!("{name}: invalid value {raw:?} (expected a positive integer)"))?;
            if value == 0 {
                return Err(format!("{name}: must be positive, got 0"));
            }
            Ok(value)
        }
    }
}
