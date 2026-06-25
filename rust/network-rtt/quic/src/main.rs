//! network-rtt **quic** experiment (Rust).
//!
//! Measures synchronous request/response round-trip latency over a single
//! long-lived QUIC bidirectional stream, then emits three result-contract JSON
//! lines (`rtt_p50`/`rtt_p99`/`rtt_mean`, `experiment="quic"`) on stdout.
//! stdout is results-only; diagnostics go to stderr.
//!
//! Methodology mirrors TCP for comparability: one connection, ONE long-lived
//! bidirectional stream, strict ping-pong (write `payload_bytes`, read the full
//! echo back), one outstanding request at a time, warmup discarded, then
//! `RTT_ITERATIONS` timed round trips. The server echoes stream bytes back.
//!
//! QUIC needs TLS; for this loopback/private-network benchmark the server
//! generates an in-memory self-signed certificate (rcgen) and the client skips
//! certificate verification (insecure — we measure latency, not security).
//! ALPN is a fixed `hperf-rtt`.
//!
//! quinn is async; this crate uses a tokio current-thread runtime and keeps the
//! ping-pong strictly sequential so each timing reflects one round-trip latency.

mod quic;

use std::process::ExitCode;

use bench_common::config::{Config, Mode};
use bench_common::measure;

const EXPERIMENT: &str = "quic";

fn main() -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(msg) => {
            eprintln!("network-rtt-quic: configuration error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    // A current-thread runtime: the ping-pong is strictly sequential, so a
    // single-threaded reactor is enough and avoids cross-thread scheduling
    // noise in the timed path.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("network-rtt-quic: failed to build tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = runtime.block_on(async {
        match cfg.mode {
            Mode::Loopback => run_loopback(&cfg).await,
            Mode::Server => run_server(&cfg).await,
            Mode::Client => run_client(&cfg).await,
        }
    });

    if let Err(msg) = result {
        eprintln!("network-rtt-quic: {msg}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Loopback: bind an in-process echo responder on an ephemeral 127.0.0.1 port,
/// run the client against it, and emit the three result lines.
async fn run_loopback(cfg: &Config) -> Result<(), String> {
    let endpoint = quic::server_endpoint("127.0.0.1:0")
        .map_err(|e| format!("quic server bind failed: {e}"))?;
    let addr = endpoint
        .local_addr()
        .map_err(|e| format!("quic local_addr failed: {e}"))?;

    // Responder runs detached on the runtime; the process exits once the client
    // is done.
    tokio::spawn(quic::serve(endpoint));

    let samples = quic::client(addr, cfg)
        .await
        .map_err(|e| format!("quic benchmark failed: {e}"))?;
    measure::emit_rtt(EXPERIMENT, &samples);
    Ok(())
}

/// Server: run the QUIC echo responder bound to `0.0.0.0:RTT_QUIC_PORT`,
/// forever. Emits nothing to stdout.
async fn run_server(cfg: &Config) -> Result<(), String> {
    let bind = format!("0.0.0.0:{}", cfg.quic_port);
    let endpoint =
        quic::server_endpoint(&bind).map_err(|e| format!("quic server bind failed: {e}"))?;
    eprintln!("network-rtt-quic: serving quic on {bind} (until killed)");
    quic::serve(endpoint).await;
    Err("quic responder ended unexpectedly".to_string())
}

/// Client: connect to `RTT_HOST:RTT_QUIC_PORT`, measure, emit the three lines.
async fn run_client(cfg: &Config) -> Result<(), String> {
    let host = cfg.require_host()?;
    let addr = quic::resolve(host, cfg.quic_port)?;
    let samples = quic::client(addr, cfg)
        .await
        .map_err(|e| format!("quic benchmark failed: {e}"))?;
    measure::emit_rtt(EXPERIMENT, &samples);
    Ok(())
}
