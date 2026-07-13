//! Shared bench-common library for the Rust benchmarks.
//!
//! Owns the comparability-critical and boilerplate code so every experiment
//! artifact stays thin and identical in methodology:
//! - [`stats`] — percentile + mean (must match Go and Java exactly).
//! - [`config`] — the `RTT_*` env contract parsing.
//! - [`result`] — hand-rendered result-contract JSON line emission.
//! - [`measure`] — the warmup + timed ping-pong loop into a pre-allocated buffer.
//! - [`fswrite`] — the filesystem-write durable-append harness (config, loop, emission).
//! - [`handoff`] — the thread-handoff harness (TH_* config, timed round-trip loop, latency + throughput emission).
//! - [`serial`] — the serialization journal harness (SER_* config, counting allocator, encode/decode timed loop + emission).
//!
//! Std-only — zero external dependencies. See
//! docs/superpowers/specs/2026-06-25-experiment-dimension-design.md.

pub mod config;
pub mod fswrite;
pub mod handoff;
pub mod measure;
pub mod result;
pub mod serial;
pub mod stats;
