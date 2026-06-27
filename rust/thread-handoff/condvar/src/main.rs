//! thread-handoff **condvar** experiment (Rust): mutex + condition-variable
//! rendezvous. Isolates the park/unpark + signal cost. Three `handoff_rtt_*`.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "condvar";

/// A one-slot mutex+condvar mailbox carrying a single token.
struct Mailbox {
    slot: Mutex<Option<u64>>,
    cv: Condvar,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox {
            slot: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    fn send(&self, v: u64) {
        let mut g = self.slot.lock().unwrap();
        *g = Some(v);
        drop(g);
        self.cv.notify_one();
    }

    fn recv(&self) -> u64 {
        let mut g = self.slot.lock().unwrap();
        loop {
            if let Some(v) = g.take() {
                return v;
            }
            g = self.cv.wait(g).unwrap();
        }
    }
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

    let req = Arc::new(Mailbox::new());
    let resp = Arc::new(Mailbox::new());

    let responder = {
        let (req, resp) = (Arc::clone(&req), Arc::clone(&resp));
        thread::spawn(move || {
            for _ in 0..total {
                let v = req.recv();
                resp.send(v);
            }
        })
    };

    let samples = handoff::measure(&cfg, || {
        req.send(1);
        let _ = resp.recv();
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
