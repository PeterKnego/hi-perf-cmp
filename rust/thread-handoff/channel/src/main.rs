//! thread-handoff **channel** experiment (Rust): an async std `mpsc::channel()`
//! in each direction — the idiomatic queue handoff, with a bounded spin
//! fast-path on receive that falls back to a blocking `recv()`.
//! Three `handoff_rtt_*` lines.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "channel";

/// Number of `try_recv()` spins before falling back to a blocking `recv()`.
/// In a tight ping-pong the counterpart usually replies within nanoseconds, so
/// spinning a bounded budget avoids the park/unpark cost; on exhaustion we park
/// via `recv()` so this never degrades into an unbounded busy-wait.
const SPIN_BUDGET: u32 = 256;

/// Bounded spin-then-park receive: poll `try_recv()` up to `SPIN_BUDGET` times,
/// then block on `recv()`. Returns `Err` only when the channel is disconnected.
#[inline]
fn spin_recv(rx: &Receiver<u64>) -> Result<u64, mpsc::RecvError> {
    for _ in 0..SPIN_BUDGET {
        match rx.try_recv() {
            Ok(v) => return Ok(v),
            Err(mpsc::TryRecvError::Empty) => std::hint::spin_loop(),
            Err(mpsc::TryRecvError::Disconnected) => return Err(mpsc::RecvError),
        }
    }
    rx.recv()
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

    // Async (unbounded) channels: `send` does not block, so the spin fast-path's
    // `try_recv()` can observe an in-flight token (a rendezvous `sync_channel(0)`
    // cannot). Each round trip enqueues exactly one token, so the queues never
    // hold more than one element.
    let (req_tx, req_rx) = mpsc::channel::<u64>();
    let (resp_tx, resp_rx) = mpsc::channel::<u64>();

    let responder = thread::spawn(move || {
        for _ in 0..total {
            let v = match spin_recv(&req_rx) {
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
        let _ = spin_recv(&resp_rx).expect("responder gone");
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
