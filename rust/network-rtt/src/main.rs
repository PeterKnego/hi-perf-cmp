//! network-rtt benchmark (Rust).
//!
//! Measures synchronous request/response round-trip latency for both TCP and
//! UDP transports, then emits six result-contract JSON lines on stdout. stdout
//! is results-only; all diagnostics go to stderr.
//!
//! Three modes, selected by `RTT_MODE`:
//! - `loopback` (default): in-process echo responders on ephemeral loopback
//!   ports + client; emits the six result lines.
//! - `server`: long-lived TCP + UDP echo responders bound to `0.0.0.0` at the
//!   configured ports; serves until killed and emits nothing to stdout.
//! - `client`: connect to `RTT_HOST` on both ports, measure, emit the six
//!   result lines.
//!
//! Std-only — zero external dependencies. See
//! docs/superpowers/specs/2026-06-25-bench-infra-aws-design.md (Part A).

mod config;
mod stats;
mod tcp;
mod udp;

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::process::ExitCode;
use std::thread;

use config::{Config, Mode};

fn main() -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(msg) => {
            eprintln!("network-rtt: configuration error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let result = match cfg.mode {
        Mode::Loopback => run_loopback(&cfg),
        Mode::Server => run_server(&cfg),
        Mode::Client => run_client(&cfg),
    };

    if let Err(msg) = result {
        eprintln!("network-rtt: {msg}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Loopback: spawn in-process echo responders on ephemeral 127.0.0.1 ports,
/// run the client against them, and emit the six result lines.
fn run_loopback(cfg: &Config) -> Result<(), String> {
    // Bind both responder sockets up front so their ephemeral addresses are
    // known before the responder threads start.
    let tcp_listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("tcp bind failed: {e}"))?;
    let tcp_addr = tcp_listener
        .local_addr()
        .map_err(|e| format!("tcp local_addr failed: {e}"))?;
    let udp_sock = UdpSocket::bind("127.0.0.1:0").map_err(|e| format!("udp bind failed: {e}"))?;
    let udp_addr = udp_sock
        .local_addr()
        .map_err(|e| format!("udp local_addr failed: {e}"))?;

    // Responders run detached; the process exits once the client is done.
    thread::spawn(move || {
        if let Err(e) = tcp::serve_listener(tcp_listener) {
            eprintln!("network-rtt: loopback tcp responder ended: {e}");
        }
    });
    thread::spawn(move || {
        if let Err(e) = udp::serve_socket(udp_sock) {
            eprintln!("network-rtt: loopback udp responder ended: {e}");
        }
    });

    let tcp_samples =
        tcp::client(tcp_addr, cfg).map_err(|e| format!("tcp benchmark failed: {e}"))?;
    emit_transport("tcp", &tcp_samples, cfg.iterations);

    let udp_samples =
        udp::client(udp_addr, cfg).map_err(|e| format!("udp benchmark failed: {e}"))?;
    emit_transport("udp", &udp_samples, cfg.iterations);

    Ok(())
}

/// Server: run both the TCP and UDP echo responders bound to `0.0.0.0` at the
/// configured ports, forever. Emits nothing to stdout.
fn run_server(cfg: &Config) -> Result<(), String> {
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], cfg.tcp_port));
    let udp_addr = SocketAddr::from(([0, 0, 0, 0], cfg.udp_port));

    eprintln!("network-rtt: serving tcp on {tcp_addr}, udp on {udp_addr} (until killed)");

    let udp_handle = thread::spawn(move || udp::serve(udp_addr));
    // The TCP responder runs on this thread; it never returns Ok.
    let tcp_result = tcp::serve(tcp_addr).map_err(|e| format!("tcp responder failed: {e}"));

    // If TCP returned, surface its error and (best effort) the UDP one too.
    match udp_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(format!("udp responder failed: {e}")),
        Err(_) => return Err("udp responder thread panicked".to_string()),
    }
    tcp_result
}

/// Client: connect to `RTT_HOST` on both ports, measure, emit the six lines.
fn run_client(cfg: &Config) -> Result<(), String> {
    let host = cfg
        .host
        .as_deref()
        .ok_or_else(|| "RTT_HOST: required in client mode".to_string())?;

    let tcp_addr = resolve(host, cfg.tcp_port)?;
    let udp_addr = resolve(host, cfg.udp_port)?;

    let tcp_samples =
        tcp::client(tcp_addr, cfg).map_err(|e| format!("tcp benchmark failed: {e}"))?;
    emit_transport("tcp", &tcp_samples, cfg.iterations);

    let udp_samples =
        udp::client(udp_addr, cfg).map_err(|e| format!("udp benchmark failed: {e}"))?;
    emit_transport("udp", &udp_samples, cfg.iterations);

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

/// Sort, compute p50/p99/mean, and print the three result lines for a transport.
fn emit_transport(transport: &str, samples: &[u64], expected: usize) {
    debug_assert_eq!(samples.len(), expected);
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();

    let p50 = stats::percentile(&sorted, 50.0);
    let p99 = stats::percentile(&sorted, 99.0);
    let mean = stats::mean(samples);

    emit_int(&format!("{transport}_rtt_p50"), p50, expected);
    emit_int(&format!("{transport}_rtt_p99"), p99, expected);
    emit_num(&format!("{transport}_rtt_mean"), mean, expected);
}

/// Emit a result line with an integer `value`.
fn emit_int(metric: &str, value: u64, samples: usize) {
    println!(
        r#"{{"language":"rust","focus_area":"network-rtt","metric":"{metric}","value":{value},"unit":"ns","samples":{samples}}}"#
    );
}

/// Emit a result line with a (possibly fractional) numeric `value`.
fn emit_num(metric: &str, value: f64, samples: usize) {
    println!(
        r#"{{"language":"rust","focus_area":"network-rtt","metric":"{metric}","value":{value},"unit":"ns","samples":{samples}}}"#
    );
}
