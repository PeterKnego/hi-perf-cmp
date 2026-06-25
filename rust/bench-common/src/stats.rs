//! Pure statistics over a slice of elapsed-nanosecond samples.
//!
//! Comparability-critical: the percentile and mean formulas must match the
//! Go and Java implementations exactly. See the network-rtt design doc.

/// Nearest-rank percentile over an **already-sorted-ascending** slice.
///
/// `percentile(p)` = `sorted[ floor( p/100 * (n - 1) ) ]` — no interpolation.
///
/// Panics if `sorted` is empty.
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    assert!(!sorted.is_empty(), "percentile of empty slice");
    let n = sorted.len();
    // floor( p/100 * (n - 1) ); cast truncates toward zero == floor for >= 0.
    let idx = (p / 100.0 * (n as f64 - 1.0)) as usize;
    sorted[idx]
}

/// Arithmetic mean as a (possibly fractional) number of nanoseconds.
///
/// Panics if `samples` is empty.
pub fn mean(samples: &[u64]) -> f64 {
    assert!(!samples.is_empty(), "mean of empty slice");
    let sum: u128 = samples.iter().map(|&x| x as u128).sum();
    sum as f64 / samples.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_p50_p99_of_100000() {
        // sorted[i] == i, so the returned value equals the index it maps to.
        let sorted: Vec<u64> = (0..100_000).collect();
        // p50 → floor(0.5 * 99999) = floor(49999.5) = 49999
        assert_eq!(percentile(&sorted, 50.0), 49_999);
        // p99 → floor(0.99 * 99999) = floor(98999.01) = 98999
        assert_eq!(percentile(&sorted, 99.0), 98_999);
    }

    #[test]
    fn percentile_known_small_array() {
        // n = 10, values equal indices.
        let sorted: Vec<u64> = (0..10).collect();
        assert_eq!(percentile(&sorted, 0.0), 0); // floor(0) = 0
        assert_eq!(percentile(&sorted, 50.0), 4); // floor(0.5*9)=floor(4.5)=4
        assert_eq!(percentile(&sorted, 99.0), 8); // floor(0.99*9)=floor(8.91)=8
        assert_eq!(percentile(&sorted, 100.0), 9); // floor(9) = 9
    }

    #[test]
    fn percentile_uses_values_not_indices() {
        // Distinct values to ensure we return the element, not the index.
        let sorted: [u64; 5] = [10, 20, 30, 40, 50];
        assert_eq!(percentile(&sorted, 50.0), 30); // floor(0.5*4)=2 → 30
        assert_eq!(percentile(&sorted, 99.0), 40); // floor(0.99*4)=floor(3.96)=3 → 40
    }

    #[test]
    fn percentile_single_element() {
        let sorted = [42u64];
        assert_eq!(percentile(&sorted, 0.0), 42);
        assert_eq!(percentile(&sorted, 50.0), 42);
        assert_eq!(percentile(&sorted, 99.0), 42);
        assert_eq!(percentile(&sorted, 100.0), 42);
    }

    #[test]
    fn percentile_odd_size() {
        let sorted: [u64; 7] = [1, 2, 3, 4, 5, 6, 7];
        // p50 → floor(0.5*6) = 3 → value 4
        assert_eq!(percentile(&sorted, 50.0), 4);
        // p99 → floor(0.99*6) = floor(5.94) = 5 → value 6
        assert_eq!(percentile(&sorted, 99.0), 6);
    }

    #[test]
    fn mean_basic() {
        assert_eq!(mean(&[1, 2, 3, 4]), 2.5);
    }

    #[test]
    fn mean_single_element() {
        assert_eq!(mean(&[7]), 7.0);
    }

    #[test]
    fn mean_no_overflow_large_values() {
        // Values near u64::MAX/n; u128 accumulation must not overflow.
        let big = u64::MAX / 4;
        let samples = [big, big, big, big];
        assert_eq!(mean(&samples), big as f64);
    }
}
