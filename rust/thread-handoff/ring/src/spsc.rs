//! A bounded single-producer single-consumer ring of `u64` tokens with
//! busy-wait (no parking). `head`/`tail` are monotonic counters; `head` doubles
//! as the consumed-count. Safe: each slot is an `AtomicU64`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct Spsc {
    buf: Box<[AtomicU64]>,
    cap: usize,
    head: AtomicUsize, // total popped (consumer writes)
    tail: AtomicUsize, // total pushed (producer writes)
}

impl Spsc {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "ring capacity must be positive");
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Spsc {
            buf,
            cap,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Producer: push one token, busy-waiting while the ring is full.
    pub fn push(&self, v: u64) {
        let tail = self.tail.load(Ordering::Relaxed);
        while tail - self.head.load(Ordering::Acquire) == self.cap {
            std::hint::spin_loop();
        }
        self.buf[tail % self.cap].store(v, Ordering::Relaxed);
        self.tail.store(tail + 1, Ordering::Release);
    }

    /// Consumer: pop one token, busy-waiting while the ring is empty.
    pub fn pop(&self) -> u64 {
        let head = self.head.load(Ordering::Relaxed);
        while head == self.tail.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        let v = self.buf[head % self.cap].load(Ordering::Relaxed);
        self.head.store(head + 1, Ordering::Release);
        v
    }

    /// Total tokens popped so far (consumer progress).
    pub fn consumed(&self) -> usize {
        self.head.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn spsc_preserves_order_and_count() {
        let n = 100_000usize;
        let ring = Arc::new(Spsc::new(64));
        let consumer = {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                let mut got = Vec::with_capacity(n);
                for _ in 0..n {
                    got.push(ring.pop());
                }
                got
            })
        };
        for i in 0..n {
            ring.push(i as u64);
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "token {i} out of order");
        }
        assert_eq!(ring.consumed(), n);
    }
}
