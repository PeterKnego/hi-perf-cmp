//! thread-handoff **condvar** experiment (Rust): mutex + condition-variable
//! rendezvous. Isolates the park/unpark + signal cost. Three `handoff_rtt_*`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "condvar";

/// Bounded number of `spin_loop` iterations to busy-check the readiness flag
/// before falling back to the blocking condvar `wait`. In a tight ping-pong the
/// counterpart usually replies within nanoseconds, so this avoids the
/// park/unpark syscall round-trip on the common path; an exhausted budget still
/// parks, keeping this a parking handoff rather than an unbounded spin.
const SPIN_BUDGET: u32 = 4_000;

/// A one-slot mutex+condvar mailbox carrying a single token, fronted by a
/// lightweight atomic readiness flag so `recv` can spin before parking.
struct Mailbox {
    /// Set (Release) by `send` after the slot is filled, cleared under the lock
    /// by `recv` once the token is taken. Lets `recv` detect readiness without
    /// touching the mutex on the fast path.
    full: AtomicBool,
    slot: Mutex<Option<u64>>,
    cv: Condvar,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox {
            full: AtomicBool::new(false),
            slot: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    fn send(&self, v: u64) {
        let mut g = self.slot.lock().unwrap();
        *g = Some(v);
        // Publish readiness; pairs with the Acquire load in `recv`'s spin.
        self.full.store(true, Ordering::Release);
        drop(g);
        self.cv.notify_one();
    }

    fn recv(&self) -> u64 {
        // Bounded spin fast-path: busy-check the readiness flag. The Acquire
        // load synchronizes with `send`'s Release store, so seeing `true`
        // guarantees the slot write is visible once we take the lock.
        for _ in 0..SPIN_BUDGET {
            if self.full.load(Ordering::Acquire) {
                let mut g = self.slot.lock().unwrap();
                if let Some(v) = g.take() {
                    self.full.store(false, Ordering::Release);
                    return v;
                }
            }
            std::hint::spin_loop();
        }
        // Slow path: park on the condvar until the predicate holds under lock.
        let mut g = self.slot.lock().unwrap();
        loop {
            if let Some(v) = g.take() {
                self.full.store(false, Ordering::Release);
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
