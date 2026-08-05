//! thread-handoff **backoff** experiment (Rust): paced ping-pong where the
//! responder waits under the Aeron-style backoff ladder (spin -> yield ->
//! `thread::sleep` park doubling 1µs -> 1ms). The requester busy-waits
//! `TH_GAP_NS` between round trips (untimed) so the responder's ladder ramps,
//! then times the round trip while spinning — the requester is the
//! measurement side, the responder the system-under-test. Emits three
//! `handoff_rtt_*` lines. See the backoff design spec.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

mod idle;

const EXPERIMENT: &str = "backoff";

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
            let mut ladder = idle::sleeping();
            for _ in 0..total {
                while slots.req.load(Ordering::Acquire) == 0 {
                    ladder.idle(0);
                }
                ladder.idle(1); // work: reset the ladder
                slots.req.store(0, Ordering::Relaxed);
                slots.resp.store(1, Ordering::Release);
            }
        })
    };

    let samples = handoff::measure_paced(&cfg, || {
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
