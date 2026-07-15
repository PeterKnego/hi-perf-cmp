//! Deterministic workload RNG (splitmix64). Identical across Rust/Go/Java.

/// The fixed workload seed (see the plan's Appendix A.2).
pub const SEED: u64 = 0x1234_5678_9ABC_DEF0;

/// A splitmix64 generator.
pub struct SplitMix {
    state: u64,
}

impl SplitMix {
    pub fn new(seed: u64) -> Self {
        SplitMix { state: seed }
    }

    // Named `next` to match Appendix A's normative pseudocode / the R1 brief's
    // interface signature (mirrored identically in Go/Java); it isn't meant to
    // satisfy `Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_is_deterministic_and_known() {
        // Golden first output for SEED — pins the sequence across languages.
        let mut a = SplitMix::new(SEED);
        let mut b = SplitMix::new(SEED);
        let first = a.next();
        assert_eq!(first, b.next(), "two instances agree");
        // Golden value: pins the exact splitmix64 output for SEED so a subtly
        // wrong shift/multiply order fails here (cross-language determinism pin).
        assert_eq!(first, 0x161922c645ce50e8, "splitmix64 golden first output");
    }
}
