//! thread-handoff **disruptor** experiment (Rust): SPSC handoff throughput using
//! the `disruptor` crate (a port of the LMAX Disruptor) with the `BusySpin` wait
//! strategy — a reference point for our hand-rolled `ring` cell. Single producer,
//! single event handler (consumer). Emits one `handoff_throughput` line.
//!
//! Methodology mirrors `ring` for an apples-to-apples comparison: warmup, then
//! time `TH_ITERATIONS` published events, waiting until the consumer has
//! processed all of them before stopping the clock (the consumer increments a
//! shared counter). `TH_RING_CAP` is the disruptor buffer size (must be a power
//! of two — the harness default 1024 qualifies).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use bench_common::handoff::{self, HandoffConfig};
use disruptor::{BusySpin, Producer, Sequence};

const EXPERIMENT: &str = "disruptor";

/// The ring-buffer event: a single token, mirroring the `u64` our `ring` passes.
struct Event {
    value: u64,
}

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = (cfg.warmup + cfg.iterations) as u64;

    // The consumer (event handler) runs on a disruptor-spawned thread; it counts
    // processed events so the producer can wait for full drain before timing.
    let consumed = Arc::new(AtomicU64::new(0));
    let processor = {
        let consumed = Arc::clone(&consumed);
        move |_event: &Event, _seq: Sequence, _end_of_batch: bool| {
            consumed.fetch_add(1, Ordering::Release);
        }
    };

    let mut producer =
        disruptor::build_single_producer(cfg.ring_cap, || Event { value: 0 }, BusySpin)
            .handle_events_with(processor)
            .build();

    // Warmup, then a drain barrier so timing excludes warmup.
    for i in 0..cfg.warmup as u64 {
        producer.publish(|e| e.value = i);
    }
    while consumed.load(Ordering::Acquire) < cfg.warmup as u64 {
        std::hint::spin_loop();
    }

    let t_start = Instant::now();
    for i in 0..cfg.iterations as u64 {
        producer.publish(|e| e.value = i);
    }
    while consumed.load(Ordering::Acquire) < total {
        std::hint::spin_loop();
    }
    let elapsed = t_start.elapsed();

    // Dropping the producer shuts the disruptor down (joins the consumer thread).
    drop(producer);

    let throughput = cfg.iterations as f64 / elapsed.as_secs_f64();
    handoff::emit_handoff_throughput(EXPERIMENT, throughput, cfg.iterations);
}
