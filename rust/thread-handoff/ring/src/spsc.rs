//! A bounded single-producer single-consumer ring of `u64` tokens with
//! busy-wait (no parking). `head`/`tail` are monotonic counters; `head` doubles
//! as the consumed-count. Safe: each slot is an `AtomicU64`.
//!
//! Two throughput-critical layout choices (canonical LMAX/Disruptor SPSC):
//!  * `head` (consumer-written) and `tail` (producer-written) live on **separate
//!    64-byte cache lines** so the two cores don't ping-pong a shared line.
//!  * Each side keeps a **local cached snapshot of the opposite index** and only
//!    re-loads the real cross-core atomic when the ring *appears* full (producer)
//!    or empty (consumer) by that cached value. That single-owner cached state
//!    lives in the [`Producer`] / [`Consumer`] handles, not on the shared struct.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// An `AtomicUsize` pinned to its own 64-byte cache line to avoid false sharing
/// with the adjacent index written by the other core.
#[repr(align(64))]
struct CacheLine(AtomicUsize);

/// Shared ring state. The monotonic indices are split onto separate cache lines;
/// the per-side cached mirrors are *not* here (they are single-owner — see
/// [`Producer`]/[`Consumer`]).
struct Spsc {
    buf: Box<[AtomicU64]>,
    cap: usize,
    head: CacheLine, // total popped (consumer writes)
    tail: CacheLine, // total pushed (producer writes)
}

impl Spsc {
    fn new(cap: usize) -> Self {
        assert!(cap > 0, "ring capacity must be positive");
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Spsc {
            buf,
            cap,
            head: CacheLine(AtomicUsize::new(0)),
            tail: CacheLine(AtomicUsize::new(0)),
        }
    }
}

/// Create a bounded SPSC ring and return its single producer / single consumer
/// handles. The cached opposite-index snapshots are owned by these handles, so
/// the single-owner contract is enforced by `&mut self` on the hot paths.
pub fn channel(cap: usize) -> (Producer, Consumer) {
    let shared = Arc::new(Spsc::new(cap));
    let producer = Producer {
        shared: Arc::clone(&shared),
        tail: 0,
        cached_head: 0,
    };
    let consumer = Consumer {
        shared,
        head: 0,
        cached_tail: 0,
    };
    (producer, consumer)
}

/// The single producer. Owns the authoritative `tail` and a cached snapshot of
/// the consumer's `head`.
pub struct Producer {
    shared: Arc<Spsc>,
    tail: usize,        // producer-owned mirror of the published tail
    cached_head: usize, // last observed consumer head (may lag the real one)
}

impl Producer {
    /// Push one token, busy-waiting while the ring is full. Only re-loads the
    /// contended `head` when the ring *appears* full by the cached snapshot.
    pub fn push(&mut self, v: u64) {
        let shared = &*self.shared;
        if self.tail - self.cached_head == shared.cap {
            // Appears full by the stale snapshot; refresh from the real head.
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.tail - self.cached_head < shared.cap {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        shared.buf[self.tail % shared.cap].store(v, Ordering::Relaxed);
        self.tail += 1;
        // Release publishes the slot write above before exposing the new tail.
        shared.tail.0.store(self.tail, Ordering::Release);
    }

    /// Total tokens popped so far (consumer progress). Reads the real shared
    /// `head`, not the cached snapshot, so the warmup/final barriers are exact.
    pub fn consumed(&self) -> usize {
        self.shared.head.0.load(Ordering::Acquire)
    }
}

/// The single consumer. Owns the authoritative `head` and a cached snapshot of
/// the producer's `tail`.
pub struct Consumer {
    shared: Arc<Spsc>,
    head: usize,        // consumer-owned mirror of the published head
    cached_tail: usize, // last observed producer tail (may lag the real one)
}

impl Consumer {
    /// Pop one token, busy-waiting while the ring is empty. Only re-loads the
    /// contended `tail` when the ring *appears* empty by the cached snapshot.
    pub fn pop(&mut self) -> u64 {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
            // Appears empty by the stale snapshot; refresh from the real tail.
            loop {
                self.cached_tail = shared.tail.0.load(Ordering::Acquire);
                if self.head != self.cached_tail {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        let v = shared.buf[self.head % shared.cap].load(Ordering::Relaxed);
        self.head += 1;
        // Release publishes the consumed slot before exposing the new head.
        shared.head.0.store(self.head, Ordering::Release);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn spsc_preserves_order_and_count() {
        let n = 100_000usize;
        let (mut prod, mut cons) = channel(64);
        let consumer = thread::spawn(move || {
            let mut got = Vec::with_capacity(n);
            for _ in 0..n {
                got.push(cons.pop());
            }
            got
        });
        for i in 0..n {
            prod.push(i as u64);
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "token {i} out of order");
        }
        assert_eq!(prod.consumed(), n);
    }
}
