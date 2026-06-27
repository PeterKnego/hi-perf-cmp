//! thread-handoff **channel** experiment (Rust): a std rendezvous
//! `sync_channel(0)` in each direction — the idiomatic blocking-queue handoff.
//! Three `handoff_rtt_*` lines.

use std::sync::mpsc;
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "channel";

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    // Rendezvous (capacity 0): send blocks until the receiver takes the value.
    let (req_tx, req_rx) = mpsc::sync_channel::<u64>(0);
    let (resp_tx, resp_rx) = mpsc::sync_channel::<u64>(0);

    let responder = thread::spawn(move || {
        for _ in 0..total {
            let v = match req_rx.recv() {
                Ok(v) => v,
                Err(_) => return,
            };
            if resp_tx.send(v).is_err() {
                return;
            }
        }
    });

    let samples = handoff::measure(&cfg, || {
        req_tx.send(1).expect("responder gone");
        let _ = resp_rx.recv().expect("responder gone");
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
