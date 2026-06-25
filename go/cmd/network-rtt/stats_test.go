package main

import "testing"

// percentile uses nearest-rank with no interpolation:
//   percentile(p) = sorted[ floor( p/100 * (n-1) ) ]
// The input is assumed already sorted ascending.

func TestPercentileSingleElement(t *testing.T) {
	s := []int64{42}
	if got := percentile(s, 50); got != 42 {
		t.Errorf("p50 single element = %d, want 42", got)
	}
	if got := percentile(s, 99); got != 42 {
		t.Errorf("p99 single element = %d, want 42", got)
	}
}

func TestPercentileKnownArray(t *testing.T) {
	// n = 10, indices 0..9. sorted = 10,20,...,100.
	s := []int64{10, 20, 30, 40, 50, 60, 70, 80, 90, 100}
	// p50 -> floor(0.50 * 9) = floor(4.5) = 4 -> 50
	if got := percentile(s, 50); got != 50 {
		t.Errorf("p50 = %d, want 50", got)
	}
	// p99 -> floor(0.99 * 9) = floor(8.91) = 8 -> 90
	if got := percentile(s, 99); got != 90 {
		t.Errorf("p99 = %d, want 90", got)
	}
	// p0 -> index 0
	if got := percentile(s, 0); got != 10 {
		t.Errorf("p0 = %d, want 10", got)
	}
	// p100 -> floor(1.0 * 9) = 9 -> 100
	if got := percentile(s, 100); got != 100 {
		t.Errorf("p100 = %d, want 100", got)
	}
}

func TestPercentileSpecExample(t *testing.T) {
	// Spec: p50 of 100000 -> index 49999; p99 -> index 98999.
	n := 100000
	s := make([]int64, n)
	for i := range s {
		s[i] = int64(i) // sorted[i] == i, so the returned value is the index
	}
	if got := percentile(s, 50); got != 49999 {
		t.Errorf("p50 index = %d, want 49999", got)
	}
	if got := percentile(s, 99); got != 98999 {
		t.Errorf("p99 index = %d, want 98999", got)
	}
}

func TestPercentileOddSize(t *testing.T) {
	// n = 5, indices 0..4. sorted = 1,3,5,7,9.
	s := []int64{1, 3, 5, 7, 9}
	// p50 -> floor(0.5 * 4) = 2 -> 5
	if got := percentile(s, 50); got != 5 {
		t.Errorf("p50 = %d, want 5", got)
	}
	// p99 -> floor(0.99 * 4) = floor(3.96) = 3 -> 7
	if got := percentile(s, 99); got != 7 {
		t.Errorf("p99 = %d, want 7", got)
	}
}

func TestMean(t *testing.T) {
	s := []int64{10, 20, 30, 40, 50}
	if got := mean(s); got != 30 {
		t.Errorf("mean = %v, want 30", got)
	}
}

func TestMeanFractional(t *testing.T) {
	s := []int64{1, 2}
	if got := mean(s); got != 1.5 {
		t.Errorf("mean = %v, want 1.5", got)
	}
}

func TestMeanSingle(t *testing.T) {
	s := []int64{7}
	if got := mean(s); got != 7 {
		t.Errorf("mean single = %v, want 7", got)
	}
}
