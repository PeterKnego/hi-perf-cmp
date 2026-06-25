package bench

// Percentile returns the value at the given percentile p (0..100) of sorted
// using nearest-rank with no interpolation:
//
//	Percentile(p) = sorted[ floor( p/100 * (n-1) ) ]
//
// sorted must be sorted ascending and non-empty.
func Percentile(sorted []int64, p float64) int64 {
	n := len(sorted)
	idx := int(p / 100 * float64(n-1)) // truncation toward zero == floor for p>=0
	if idx < 0 {
		idx = 0
	}
	if idx >= n {
		idx = n - 1
	}
	return sorted[idx]
}

// Mean returns the arithmetic mean of samples as a float64.
// samples must be non-empty.
func Mean(samples []int64) float64 {
	var sum int64
	for _, v := range samples {
		sum += v
	}
	return float64(sum) / float64(len(samples))
}
