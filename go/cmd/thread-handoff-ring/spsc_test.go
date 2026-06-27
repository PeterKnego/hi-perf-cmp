package main

import (
	"sync"
	"testing"
)

func TestSPSCPreservesOrderAndCount(t *testing.T) {
	const n = 100000
	ring := newSPSC(64)
	var got []uint64
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		got = make([]uint64, 0, n)
		for i := 0; i < n; i++ {
			got = append(got, ring.pop())
		}
	}()
	for i := 0; i < n; i++ {
		ring.push(uint64(i))
	}
	wg.Wait()
	if len(got) != n {
		t.Fatalf("want %d tokens, got %d", n, len(got))
	}
	for i := 0; i < n; i++ {
		if got[i] != uint64(i) {
			t.Fatalf("token %d: want %d, got %d", i, i, got[i])
		}
	}
	if ring.consumed() != n {
		t.Fatalf("want consumed %d, got %d", n, ring.consumed())
	}
}
