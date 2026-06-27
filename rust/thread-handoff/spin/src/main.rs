//! thread-handoff **spin** experiment (Rust): single-slot atomic handoff,
//! busy-wait. Lowest latency, burns a core. Emits three `handoff_rtt_*` lines.
//!
//! Two single-slot mailboxes carry a non-zero token; `0` means empty.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "spin";

struct Slots {
    req: AtomicU64,  // timer -> responder (0 = empty)
    resp: AtomicU64, // responder -> timer (0 = empty)
}

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    let slots = Arc::new(Slots {
        req: AtomicU64::new(0),
        resp: AtomicU64::new(0),
    });

    let responder = {
        let slots = Arc::clone(&slots);
        thread::spawn(move || {
            for _ in 0..total {
                while slots.req.load(Ordering::Acquire) == 0 {
                    std::hint::spin_loop();
                }
                slots.req.store(0, Ordering::Relaxed);
                slots.resp.store(1, Ordering::Release);
            }
        })
    };

    let samples = handoff::measure(&cfg, || {
        slots.req.store(1, Ordering::Release);
        while slots.resp.load(Ordering::Acquire) == 0 {
            std::hint::spin_loop();
        }
        slots.resp.store(0, Ordering::Relaxed);
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
