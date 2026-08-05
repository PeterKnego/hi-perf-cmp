//! Aeron-style spin -> yield -> timed-park backoff ladder (aeron-go default
//! parameters, fixed constants — cross-language comparability is the point).
//! The park rung is `thread::sleep` (nanosleep): Rust's timed sleep
//! overshoots by ~tens of µs on Linux — honest rungs, unlike Go's
//! `time.Sleep` collapse, and that difference is what the grid compares.
//! The parker is injectable so the ladder's state machine is unit-testable
//! without wall-clock assertions (overshoot is host-dependent; it is what
//! the fleet cell measures, not what CI asserts).

use std::thread;
use std::time::Duration;

pub const MAX_SPINS: u32 = 10;
pub const MAX_YIELDS: u32 = 20;
pub const MIN_PARK_NS: u64 = 1_000;
pub const MAX_PARK_NS: u64 = 1_000_000;

/// Backoff ladder over an injectable parker. Single-thread use.
pub struct Backoff<P: FnMut(u64)> {
    spins: u32,
    yields: u32,
    park_ns: u64,
    parker: P,
}

/// The production ladder: parks with `thread::sleep`.
pub fn sleeping() -> Backoff<impl FnMut(u64)> {
    Backoff::with_parker(|ns| thread::sleep(Duration::from_nanos(ns)))
}

impl<P: FnMut(u64)> Backoff<P> {
    pub fn with_parker(parker: P) -> Self {
        Backoff {
            spins: 0,
            yields: 0,
            park_ns: MIN_PARK_NS,
            parker,
        }
    }

    /// Advance the ladder when `work_count == 0`; reset it otherwise.
    pub fn idle(&mut self, work_count: usize) {
        if work_count > 0 {
            self.spins = 0;
            self.yields = 0;
            self.park_ns = MIN_PARK_NS;
            return;
        }
        if self.spins < MAX_SPINS {
            self.spins += 1;
            std::hint::spin_loop();
        } else if self.yields < MAX_YIELDS {
            self.yields += 1;
            thread::yield_now();
        } else {
            (self.parker)(self.park_ns);
            self.park_ns = (self.park_ns * 2).min(MAX_PARK_NS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn recorded(parks: &RefCell<Vec<u64>>) -> Backoff<impl FnMut(u64) + '_> {
        Backoff::with_parker(move |ns| parks.borrow_mut().push(ns))
    }

    #[test]
    fn spins_and_yields_before_first_park() {
        let parks = RefCell::new(Vec::new());
        let mut b = recorded(&parks);
        for _ in 0..(MAX_SPINS + MAX_YIELDS) {
            b.idle(0);
        }
        assert!(parks.borrow().is_empty(), "parked during spin/yield rungs");
        b.idle(0);
        assert_eq!(*parks.borrow(), vec![MIN_PARK_NS]);
    }

    #[test]
    fn park_period_doubles_and_caps_at_max() {
        let parks = RefCell::new(Vec::new());
        let mut b = recorded(&parks);
        for _ in 0..(MAX_SPINS + MAX_YIELDS + 13) {
            b.idle(0);
        }
        let want = vec![
            1_000, 2_000, 4_000, 8_000, 16_000, 32_000, 64_000, 128_000, 256_000, 512_000,
            1_000_000, 1_000_000, 1_000_000,
        ];
        assert_eq!(*parks.borrow(), want);
    }

    #[test]
    fn work_resets_the_ladder() {
        let parks = RefCell::new(Vec::new());
        let mut b = recorded(&parks);
        for _ in 0..(MAX_SPINS + MAX_YIELDS + 3) {
            b.idle(0);
        }
        assert_eq!(parks.borrow().len(), 3);
        b.idle(1); // work: full reset
        for _ in 0..(MAX_SPINS + MAX_YIELDS) {
            b.idle(0);
        }
        assert_eq!(parks.borrow().len(), 3, "post-reset rungs must not park");
        b.idle(0);
        assert_eq!(parks.borrow()[3], MIN_PARK_NS, "park restarts at MIN");
    }
}
