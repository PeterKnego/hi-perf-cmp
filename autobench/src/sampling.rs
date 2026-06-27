// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Peter Knego

//! Contract-line parsing, `median`, and the two-process `Network` fitness
//! driver.
//!
//! The fitness binary is the experiment artifact itself — it prints one
//! result-contract JSON object per line on stdout (see `docs/result-contract.md`).
//! We parse those lines into a metrics map keyed `<metric>_ns`, filtered to the
//! task's `focus_area`/`experiment`, and take the median over `--samples` runs.

use std::collections::BTreeMap;
use std::io::Read;
use std::net::{TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Transport of a `Network` cell — selects the port env var and the
/// server-readiness probe. TCP detects readiness by connecting to the
/// listener; UDP (connectionless) sends a probe datagram and waits for the echo
/// server to reflect it; QUIC (over UDP, but speaks a TLS handshake so it won't
/// echo a raw datagram) detects readiness by a UDP bind-probe — the port
/// becoming `AddrInUse` means the server has bound it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tcp,
    Udp,
    Quic,
}

impl Transport {
    /// Map a TaskSpec `experiment` to a transport. Unknown experiments default
    /// to TCP.
    pub fn from_experiment(experiment: &str) -> Self {
        match experiment {
            "udp" => Transport::Udp,
            "quic" => Transport::Quic,
            _ => Transport::Tcp,
        }
    }

    /// The env var the cell reads for its port, per the `RTT_*_PORT` contract.
    fn port_env(self) -> &'static str {
        match self {
            Transport::Tcp => "RTT_TCP_PORT",
            Transport::Udp => "RTT_UDP_PORT",
            Transport::Quic => "RTT_QUIC_PORT",
        }
    }
}

/// Median of `xs`. For an even count, returns the lower-middle of the two
/// central elements (nearest-rank style), matching the bench convention. Empty
/// input returns 0.0.
pub fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in samples"));
    v[(v.len() - 1) / 2]
}

/// Parse result-contract JSON lines from a captured stdout `String` into a
/// metrics map keyed `<metric>_<unit>` (e.g. `rtt_p50`+`ns` -> `"rtt_p50_ns"`).
///
/// Lines whose `focus_area`/`experiment` do not match the task are ignored, as
/// are non-JSON lines (diagnostics belong on stderr, but tolerate strays). The
/// `value` is read as an f64 so integer and float forms (`42000`, `0.0`) both
/// parse; the optional `notes` field is ignored.
pub fn parse_contract_metrics(
    stdout: &str,
    focus_area: &str,
    experiment: &str,
) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("focus_area").and_then(|f| f.as_str()) != Some(focus_area) {
            continue;
        }
        if v.get("experiment").and_then(|e| e.as_str()) != Some(experiment) {
            continue;
        }
        let Some(metric) = v.get("metric").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(unit) = v.get("unit").and_then(|u| u.as_str()) else {
            continue;
        };
        let Some(value) = v.get("value").and_then(serde_json::Value::as_f64) else {
            continue;
        };
        out.insert(format!("{metric}_{unit}"), value);
    }
    out
}

/// A spawned child that is killed on drop. Guarantees the server process is
/// reaped even if the driver returns early via `?` or panics.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Build the artifact-launch `Command` for the given run argv, cwd, and env.
fn run_command(run: &[&str], run_dir: &str, env: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(run[0]);
    cmd.args(&run[1..]).current_dir(crate::resolve_dir(run_dir));
    for (k, val) in env {
        cmd.env(k, val);
    }
    cmd
}

/// Wait until `127.0.0.1:port` accepts a connection, or `timeout` elapses.
/// Returns true if the port became connectable. Polls with short backoff.
fn wait_for_bind(port: u16, timeout: Duration) -> bool {
    let addr = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect(&addr).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Wait until the UDP echo server on `127.0.0.1:port` is responsive, or
/// `timeout` elapses. UDP has no connection to probe, so we send a small probe
/// datagram and wait for the echo server to reflect it back; datagrams sent
/// before the server binds are simply dropped and retried. Returns true once an
/// echo is received. This relies only on the cell's echo behavior (reflect any
/// datagram), not on its internal protocol.
fn wait_for_udp_echo(port: u16, timeout: Duration) -> bool {
    let addr = format!("127.0.0.1:{port}");
    let Ok(probe) = UdpSocket::bind("127.0.0.1:0") else {
        return false;
    };
    // Short per-recv timeout so we re-send promptly while the server comes up.
    let _ = probe.set_read_timeout(Some(Duration::from_millis(50)));
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 8];
    while Instant::now() < deadline {
        if probe.send_to(b"PING", &addr).is_ok() && probe.recv_from(&mut buf).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Wait until a QUIC server has bound the UDP `port` on `127.0.0.1`, or
/// `timeout` elapses. A QUIC server won't echo a raw datagram (it expects a TLS
/// handshake), so we instead probe by trying to bind the port ourselves: while
/// our bind SUCCEEDS the server hasn't bound yet (we release immediately and
/// retry); once it FAILS with `AddrInUse` the server holds the port → ready.
/// The QUIC servers don't set SO_REUSEPORT, so this overlap detection is
/// reliable; QUIC's handshake retransmission covers any brief gap between the
/// bind and the endpoint being ready to accept.
fn wait_for_udp_bind_probe(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match UdpSocket::bind(("127.0.0.1", port)) {
            // We could bind it → the server has not bound yet. Release at once
            // (minimizing any window in which we'd block the server's bind) and
            // retry after a short backoff.
            Ok(sock) => {
                drop(sock);
                std::thread::sleep(Duration::from_millis(20));
            }
            // Bind refused → the port is taken by the server → ready.
            Err(_) => return true,
        }
    }
    false
}

/// Outcome of one two-process network run.
pub struct NetworkRun {
    /// True if the client exited 0.
    pub client_ok: bool,
    /// Client stdout (the contract lines).
    pub stdout: String,
    /// Client stderr + a note of any server failure.
    pub stderr: String,
}

/// Run one two-process network fitness sample over `127.0.0.1`.
///
/// Spawns the artifact in `RTT_MODE=server` (plus `extra_env`, e.g. port and
/// iteration counts) as a child, waits for it to become ready (TCP: connect
/// probe; UDP: echo probe), then runs the artifact in `RTT_MODE=client
/// RTT_HOST=127.0.0.1` (plus `extra_env`) and captures its stdout. The port is
/// passed via the transport's env var (`RTT_TCP_PORT` / `RTT_UDP_PORT`). The
/// server child is ALWAYS killed via a drop guard, even if the client errors.
pub fn run_network_once(
    run: &[&str],
    run_dir: &str,
    port: u16,
    extra_env: &[(&str, &str)],
    transport: Transport,
) -> std::io::Result<NetworkRun> {
    let port_s = port.to_string();
    let port_env = transport.port_env();
    // Server env: RTT_MODE=server + the port + caller's extra env.
    let mut server_env: Vec<(&str, &str)> = vec![("RTT_MODE", "server"), (port_env, &port_s)];
    server_env.extend_from_slice(extra_env);

    let mut server_cmd = run_command(run, run_dir, &server_env);
    server_cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut guard = KillOnDrop(server_cmd.spawn()?);

    let ready = match transport {
        Transport::Tcp => wait_for_bind(port, Duration::from_secs(10)),
        Transport::Udp => wait_for_udp_echo(port, Duration::from_secs(10)),
        Transport::Quic => wait_for_udp_bind_probe(port, Duration::from_secs(10)),
    };
    if !ready {
        // Drain whatever the server logged to help diagnose the bind failure.
        let mut server_err = String::new();
        if let Some(mut e) = guard.0.stderr.take() {
            let _ = e.read_to_string(&mut server_err);
        }
        return Ok(NetworkRun {
            client_ok: false,
            stdout: String::new(),
            stderr: format!(
                "server did not become ready on 127.0.0.1:{port} within 10s\n{server_err}"
            ),
        });
    }

    // Client env: RTT_MODE=client RTT_HOST=127.0.0.1 + port + caller's extra env.
    let mut client_env: Vec<(&str, &str)> = vec![
        ("RTT_MODE", "client"),
        ("RTT_HOST", "127.0.0.1"),
        (port_env, &port_s),
    ];
    client_env.extend_from_slice(extra_env);

    let output = run_command(run, run_dir, &client_env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    // `guard` is dropped at end of scope, killing the server.
    Ok(NetworkRun {
        client_ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP_LINES: &str = r#"
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":42000,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p99","value":81000,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_mean","value":50000.5,"unit":"ns","samples":100000}
"#;

    #[test]
    fn parses_three_tcp_lines_ns_keys_unchanged() {
        // Backward compatibility: an `ns` unit still yields the `_ns` key.
        let m = parse_contract_metrics(TCP_LINES, "network-rtt", "tcp");
        assert_eq!(m.len(), 3);
        assert_eq!(m["rtt_p50_ns"], 42000.0);
        assert_eq!(m["rtt_p99_ns"], 81000.0);
        assert_eq!(m["rtt_mean_ns"], 50000.5);
    }

    #[test]
    fn keys_thread_handoff_latency_and_throughput_by_unit() {
        let spin = r#"
{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":182,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_mean","value":186.2,"unit":"ns","samples":100000}
"#;
        let m = parse_contract_metrics(spin, "thread-handoff", "spin");
        assert_eq!(m["handoff_rtt_p50_ns"], 182.0);
        assert_eq!(m["handoff_rtt_mean_ns"], 186.2);

        let ring = r#"{"language":"rust","focus_area":"thread-handoff","experiment":"ring","metric":"handoff_throughput","value":28139265.7,"unit":"ops_per_sec","samples":100000}"#;
        let r = parse_contract_metrics(ring, "thread-handoff", "ring");
        assert_eq!(r["handoff_throughput_ops_per_sec"], 28139265.7);
    }

    #[test]
    fn line_missing_unit_is_skipped() {
        let line = r#"{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":1,"samples":1}"#;
        let m = parse_contract_metrics(line, "thread-handoff", "spin");
        assert!(m.is_empty());
    }

    #[test]
    fn ignores_other_experiments_and_focus_areas() {
        let mixed = format!(
            "{}\n{}\n{}",
            r#"{"language":"rust","focus_area":"network-rtt","experiment":"udp","metric":"rtt_p50","value":99,"unit":"ns","samples":1}"#,
            r#"{"language":"rust","focus_area":"filesystem-write","experiment":"tcp","metric":"rtt_p50","value":99,"unit":"ns","samples":1}"#,
            r#"{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":42000,"unit":"ns","samples":1}"#,
        );
        let m = parse_contract_metrics(&mixed, "network-rtt", "tcp");
        assert_eq!(m.len(), 1);
        assert_eq!(m["rtt_p50_ns"], 42000.0);
    }

    #[test]
    fn tolerates_float_values_and_notes() {
        // Java-style "0.0" floats and a notes field must still parse.
        let line = r#"{"language":"java","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":0.0,"unit":"ns","samples":10,"notes":"warm"}"#;
        let m = parse_contract_metrics(line, "network-rtt", "tcp");
        assert_eq!(m["rtt_p50_ns"], 0.0);
    }

    #[test]
    fn tolerates_non_json_strays() {
        let s = format!("some diagnostic noise\n{}", TCP_LINES.trim());
        let m = parse_contract_metrics(&s, "network-rtt", "tcp");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn median_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    }

    #[test]
    fn median_even_lower_middle() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.0);
    }

    #[test]
    fn median_single() {
        assert_eq!(median(&[42.0]), 42.0);
    }

    #[test]
    fn median_empty_is_zero() {
        assert_eq!(median(&[]), 0.0);
    }

    #[test]
    fn transport_from_experiment_maps_each_and_defaults_tcp() {
        assert_eq!(Transport::from_experiment("udp"), Transport::Udp);
        assert_eq!(Transport::from_experiment("quic"), Transport::Quic);
        assert_eq!(Transport::from_experiment("tcp"), Transport::Tcp);
        // Unknown experiments default to TCP.
        assert_eq!(Transport::from_experiment("mystery"), Transport::Tcp);
    }

    #[test]
    fn transport_selects_port_env() {
        assert_eq!(Transport::Tcp.port_env(), "RTT_TCP_PORT");
        assert_eq!(Transport::Udp.port_env(), "RTT_UDP_PORT");
        assert_eq!(Transport::Quic.port_env(), "RTT_QUIC_PORT");
    }
}
