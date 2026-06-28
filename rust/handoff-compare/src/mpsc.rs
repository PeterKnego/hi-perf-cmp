//! Lock-free multi-producer single-consumer ring of `u64` tokens (the LMAX
//! Disruptor multi-producer algorithm), busy-wait. Producers claim a contiguous
//! range with `fetch_add` on a shared cursor, write their slots, then publish
//! via a per-slot **availability buffer** recording the round number; the single
//! consumer scans the contiguous published prefix. std-only.
//!
//! Invariant: every claimed sequence is delivered to the consumer exactly once,
//! in sequence order. Items from different producers interleave by claim order
//! (no per-producer global FIFO — same as disruptor multi-producer). No loss, no
//! duplication, no slot overwritten before it is consumed.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

#[repr(align(64))]
struct CacheLine(AtomicUsize);

struct Mpsc {
    buf: Box<[AtomicU64]>,
    /// Per-slot published round number (`seq / cap`); -1 = never published.
    avail: Box<[AtomicI64]>,
    cap: usize,
    claim: CacheLine, // next sequence to claim (producers fetch_add)
    head: CacheLine,  // consumer cursor (total consumed)
}

impl Mpsc {
    fn new(cap: usize) -> Self {
        assert!(
            cap > 0 && cap.is_power_of_two(),
            "cap must be a power of two"
        );
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let avail = (0..cap)
            .map(|_| AtomicI64::new(-1))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Mpsc {
            buf,
            avail,
            cap,
            claim: CacheLine(AtomicUsize::new(0)),
            head: CacheLine(AtomicUsize::new(0)),
        }
    }
}

/// Create a bounded MPSC ring: one consumer, any number of (cloned) producers.
pub fn ring(cap: usize) -> (MpProducer, MpConsumer) {
    let shared = Arc::new(Mpsc::new(cap));
    (
        MpProducer {
            shared: Arc::clone(&shared),
            cached_head: 0,
        },
        MpConsumer { shared, head: 0 },
    )
}

/// A producer handle. `Clone` it once per producer thread; all clones share the
/// claim cursor and availability buffer. Each clone keeps its own cached head.
#[derive(Clone)]
pub struct MpProducer {
    shared: Arc<Mpsc>,
    cached_head: usize, // per-producer cached consumer head (backpressure)
}

impl MpProducer {
    /// Claim `n` contiguous slots, fill via `fill(k)` for k in 0..n, and publish
    /// each. `n` must be `<= cap`. Busy-waits for backpressure (the slots it will
    /// overwrite must already have been consumed).
    pub fn batch_publish<F: Fn(usize) -> u64>(&mut self, n: usize, fill: F) {
        let shared = &*self.shared;
        debug_assert!(n <= shared.cap, "burst exceeds ring capacity");
        // Claim a disjoint contiguous range [seq, seq+n) (Relaxed: ordering of
        // the data is established by the availability buffer, not this counter).
        let seq = shared.claim.0.fetch_add(n, Ordering::Relaxed);
        // Backpressure: the slot for the highest sequence (seq+n-1) aliases the
        // occupant of sequence (seq+n-1) - cap, which must be consumed first =>
        // wait until consumer head >= seq + n - cap.
        let need = (seq + n).saturating_sub(shared.cap);
        if self.cached_head < need {
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.cached_head >= need {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        for k in 0..n {
            let s = seq + k;
            shared.buf[s % shared.cap].store(fill(k), Ordering::Relaxed);
            // Publish: Release pairs with the consumer's Acquire load of avail.
            shared.avail[s % shared.cap].store((s / shared.cap) as i64, Ordering::Release);
        }
    }
}

/// The single consumer.
pub struct MpConsumer {
    shared: Arc<Mpsc>,
    head: usize,
}

impl MpConsumer {
    /// Drain the contiguous published prefix (up to `max`), calling `f` per
    /// token; advance the consumer cursor once at the end. Returns the count
    /// drained (0 if the next sequence is not yet published).
    pub fn drain<F: FnMut(u64)>(&mut self, max: usize, mut f: F) -> usize {
        let shared = &*self.shared;
        let mut count = 0usize;
        while count < max {
            let s = self.head;
            let expected = (s / shared.cap) as i64;
            // `s` is published iff its slot carries `s`'s round number.
            if shared.avail[s % shared.cap].load(Ordering::Acquire) != expected {
                break;
            }
            f(shared.buf[s % shared.cap].load(Ordering::Relaxed));
            self.head += 1;
            count += 1;
        }
        if count > 0 {
            shared.head.0.store(self.head, Ordering::Release);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::thread;

    fn run_stress(producers: usize, per: usize, cap: usize) {
        let total = producers * per;
        let (prod, mut cons) = ring(cap);
        let mut handles = Vec::new();
        for p in 0..producers {
            let mut pr = prod.clone();
            handles.push(thread::spawn(move || {
                let base = (p * per) as u64; // unique value range per producer
                let burst = 13usize;
                let mut sent = 0usize;
                while sent < per {
                    let b = burst.min(per - sent);
                    let s0 = sent;
                    pr.batch_publish(b, |k| base + (s0 + k) as u64);
                    sent += b;
                }
            }));
        }
        drop(prod); // only the clones produce

        let mut seen: HashSet<u64> = HashSet::with_capacity(total);
        let mut dups = 0usize;
        let mut received = 0usize;
        while received < total {
            received += cons.drain(usize::MAX, |v| {
                if !seen.insert(v) {
                    dups += 1;
                }
            });
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(dups, 0, "duplicate delivery");
        assert_eq!(seen.len(), total, "missing elements (loss)");
        for p in 0..producers {
            for i in 0..per {
                assert!(seen.contains(&((p * per + i) as u64)), "missing value");
            }
        }
    }

    #[test]
    fn mpsc_no_loss_no_dup_under_contention() {
        // Small cap vs large volume exercises wrap + backpressure heavily.
        // Repeat to shake out races in the lock-free publish/availability path.
        for _ in 0..5 {
            run_stress(4, 50_000, 256);
        }
    }
}
