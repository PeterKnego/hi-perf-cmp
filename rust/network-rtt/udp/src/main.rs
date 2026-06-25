//! network-rtt **udp** experiment (Rust).
//!
//! Measures synchronous request/response round-trip latency over UDP, then
//! emits three result-contract JSON lines (`rtt_p50`/`rtt_p99`/`rtt_mean`,
//! `experiment="udp"`) on stdout. stdout is results-only; diagnostics go to
//! stderr.
//!
//! Three modes, selected by `RTT_MODE`:
//! - `loopback` (default): in-process echo responder on an ephemeral loopback
//!   port + client; emits the three result lines.
//! - `server`: long-lived UDP echo responder bound to `0.0.0.0:RTT_UDP_PORT`;
//!   serves until killed and emits nothing to stdout.
//! - `client`: connect to `RTT_HOST:RTT_UDP_PORT`, measure, emit three lines.
//!
//! Std-only — zero external dependencies (beyond `bench-common`).

mod udp;

use std::net::{SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::thread;

use bench_common::config::{Config, Mode};
use bench_common::measure;

const EXPERIMENT: &str = "udp";

fn main() -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(msg) => {
            eprintln!("network-rtt-udp: configuration error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let result = match cfg.mode {
        Mode::Loopback => run_loopback(&cfg),
        Mode::Server => run_server(&cfg),
        Mode::Client => run_client(&cfg),
    };

    if let Err(msg) = result {
        eprintln!("network-rtt-udp: {msg}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Loopback: spawn an in-process echo responder on an ephemeral 127.0.0.1 port,
/// run the client against it, and emit the three result lines.
fn run_loopback(cfg: &Config) -> Result<(), String> {
    let sock = UdpSocket::bind("127.0.0.1:0").map_err(|e| format!("udp bind failed: {e}"))?;
    let addr = sock
        .local_addr()
        .map_err(|e| format!("udp local_addr failed: {e}"))?;

    // Responder runs detached; the process exits once the client is done.
    thread::spawn(move || {
        if let Err(e) = udp::serve_socket(sock) {
            eprintln!("network-rtt-udp: loopback responder ended: {e}");
        }
    });

    let samples = udp::client(addr, cfg).map_err(|e| format!("udp benchmark failed: {e}"))?;
    measure::emit_rtt(EXPERIMENT, &samples);
    Ok(())
}

/// Server: run the UDP echo responder bound to `0.0.0.0:RTT_UDP_PORT`, forever.
/// Emits nothing to stdout.
fn run_server(cfg: &Config) -> Result<(), String> {
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.udp_port));
    eprintln!("network-rtt-udp: serving udp on {addr} (until killed)");
    udp::serve(addr).map_err(|e| format!("udp responder failed: {e}"))
}

/// Client: connect to `RTT_HOST:RTT_UDP_PORT`, measure, emit the three lines.
fn run_client(cfg: &Config) -> Result<(), String> {
    let host = cfg.require_host()?;
    let addr = resolve(host, cfg.udp_port)?;
    let samples = udp::client(addr, cfg).map_err(|e| format!("udp benchmark failed: {e}"))?;
    measure::emit_rtt(EXPERIMENT, &samples);
    Ok(())
}

/// Resolve `host:port` to a single socket address.
fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("could not resolve {host}:{port}: {e}"))?
        .next()
        .ok_or_else(|| format!("no addresses resolved for {host}:{port}"))
}
