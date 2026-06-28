//! Bounded single-producer single-consumer ring of `u64` tokens, busy-wait.
//!
//! Mirrored verbatim from `rust/thread-handoff/ring/src/spsc.rs` (the shipped,
//! AWS-validated optimized ring is the source of truth) — a pinned snapshot for
//! this comparison study — plus a batch `batch_publish`/`drain` API for bursts.
//!
//! Layout: `head` (consumer) and `tail` (producer) on separate 64-byte cache
//! lines; each side caches the opposite index and only re-loads the contended
//! atomic when the ring appears full/empty.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[repr(align(64))]
struct CacheLine(AtomicUsize);

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

/// Create a bounded SPSC ring and return its single producer / consumer handles.
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

/// The single producer. Owns `tail` and a cached snapshot of the consumer head.
pub struct Producer {
    shared: Arc<Spsc>,
    tail: usize,
    cached_head: usize,
}

impl Producer {
    /// Push one token, busy-waiting while the ring is full.
    pub fn push(&mut self, v: u64) {
        let shared = &*self.shared;
        if self.tail - self.cached_head == shared.cap {
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
        shared.tail.0.store(self.tail, Ordering::Release);
    }

    /// Reserve `n` contiguous slots (busy-waiting for space), fill them via
    /// `fill(k)` for k in 0..n, then publish all `n` with a single Release. One
    /// barrier per burst instead of per element. `n` must be `<= cap`.
    pub fn batch_publish<F: Fn(usize) -> u64>(&mut self, n: usize, fill: F) {
        let shared = &*self.shared;
        debug_assert!(n <= shared.cap, "burst exceeds ring capacity");
        // Need `n` free slots: outstanding (tail-head) + n <= cap.
        if self.tail + n - self.cached_head > shared.cap {
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.tail + n - self.cached_head <= shared.cap {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        for k in 0..n {
            shared.buf[(self.tail + k) % shared.cap].store(fill(k), Ordering::Relaxed);
        }
        self.tail += n;
        shared.tail.0.store(self.tail, Ordering::Release);
    }

    /// Total tokens popped so far (reads the real shared head).
    pub fn consumed(&self) -> usize {
        self.shared.head.0.load(Ordering::Acquire)
    }
}

/// The single consumer. Owns `head` and a cached snapshot of the producer tail.
pub struct Consumer {
    shared: Arc<Spsc>,
    head: usize,
    cached_tail: usize,
}

impl Consumer {
    /// Pop one token, busy-waiting while the ring is empty.
    pub fn pop(&mut self) -> u64 {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
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
        shared.head.0.store(self.head, Ordering::Release);
        v
    }

    /// Drain up to `max` currently-available tokens, calling `f` per token;
    /// advance `head` once at the end. Returns the number drained (0 if empty).
    /// Non-blocking: refreshes the cached tail only when it appears empty.
    pub fn drain<F: FnMut(u64)>(&mut self, max: usize, mut f: F) -> usize {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
            self.cached_tail = shared.tail.0.load(Ordering::Acquire);
        }
        let take = (self.cached_tail - self.head).min(max);
        for k in 0..take {
            f(shared.buf[(self.head + k) % shared.cap].load(Ordering::Relaxed));
        }
        self.head += take;
        if take > 0 {
            shared.head.0.store(self.head, Ordering::Release);
        }
        take
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

    #[test]
    fn spsc_batch_preserves_order_and_count_with_wrap() {
        // n far exceeds cap (wrap-around); burst is non-divisible vs cap and n.
        let n = 100_000usize;
        let burst = 7usize;
        let (mut prod, mut cons) = channel(64);
        let consumer = thread::spawn(move || {
            let mut got = Vec::with_capacity(n);
            while got.len() < n {
                cons.drain(usize::MAX, |v| got.push(v));
            }
            got
        });
        let mut sent = 0usize;
        while sent < n {
            let b = burst.min(n - sent);
            let base = sent;
            prod.batch_publish(b, |k| (base + k) as u64);
            sent += b;
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "batch token {i} out of order");
        }
    }
}
