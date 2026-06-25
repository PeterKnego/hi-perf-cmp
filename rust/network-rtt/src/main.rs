//! network-rtt benchmark (Rust).
//!
//! Measures synchronous request/response round-trip latency over loopback for
//! both TCP and UDP transports, then emits six result-contract JSON lines on
//! stdout. stdout is results-only; all diagnostics go to stderr.
//!
//! Std-only — zero external dependencies. See
//! docs/superpowers/specs/2026-06-25-network-rtt-design.md.

mod config;
mod stats;
mod tcp;
mod udp;

use std::process::ExitCode;

use config::Config;

fn main() -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(msg) => {
            eprintln!("network-rtt: configuration error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(msg) = run(&cfg) {
        eprintln!("network-rtt: {msg}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(cfg: &Config) -> Result<(), String> {
    let tcp_samples = tcp::run(cfg).map_err(|e| format!("tcp benchmark failed: {e}"))?;
    emit_transport("tcp", &tcp_samples, cfg.iterations);

    let udp_samples = udp::run(cfg).map_err(|e| format!("udp benchmark failed: {e}"))?;
    emit_transport("udp", &udp_samples, cfg.iterations);

    Ok(())
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
