//! thread-handoff **ring** experiment (Rust): bounded SPSC ring, busy-wait,
//! pipelined depth N. Emits one `handoff_throughput` line.

mod spsc;

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use bench_common::handoff::{self, HandoffConfig};
use spsc::Spsc;

const EXPERIMENT: &str = "ring";

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    let ring = Arc::new(Spsc::new(cfg.ring_cap));

    let consumer = {
        let ring = Arc::clone(&ring);
        thread::spawn(move || {
            for _ in 0..total {
                let _ = ring.pop();
            }
        })
    };

    // Warmup pushes, then a drain barrier so timing excludes warmup.
    for _ in 0..cfg.warmup {
        ring.push(1);
    }
    while ring.consumed() < cfg.warmup {
        std::hint::spin_loop();
    }

    let t_start = Instant::now();
    for _ in 0..cfg.iterations {
        ring.push(1);
    }
    while ring.consumed() < total {
        std::hint::spin_loop();
    }
    let elapsed = t_start.elapsed();

    if consumer.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: consumer thread panicked");
        std::process::exit(1);
    }

    let throughput = cfg.iterations as f64 / elapsed.as_secs_f64();
    handoff::emit_handoff_throughput(EXPERIMENT, throughput, cfg.iterations);
}
