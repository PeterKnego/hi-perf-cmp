// Package idle implements the Aeron-style spin -> yield -> timed-park backoff
// ladder for the thread-handoff backoff cells. Parameters are fixed constants
// (aeron-go defaults) — cross-language comparability is the point.
//
// Two parkers implement the ladder's park rung:
//
//   - NewBackoff parks with time.Sleep — the idiomatic naive port. Go's
//     runtime timer overshoots sub-millisecond sleeps by orders of magnitude
//     (a 6 µs request costs ~425 µs; >= 8 µs costs ~1 ms — measured in
//     aeron-go 1ce3720), so the ladder collapses to its top rung. That
//     overshoot is deliberately kept: it is what the `backoff` cell measures.
//   - NewYieldingBackoff serves parks shorter than SleepFloorNs by yielding
//     to a deadline (runtime.Gosched), the aeron-go 1ce3720 strategy; parks
//     at or above the floor still sleep. The `backoff_yield` cell.
package idle

import (
	"runtime"
	"time"
)

// Aeron/Agrona BackoffIdleStrategy defaults (aeron-go values).
const (
	MaxSpins  = 10
	MaxYields = 20
	MinParkNs = int64(1000)
	MaxParkNs = int64(time.Millisecond)
	// SleepFloorNs is the shortest park handed to time.Sleep by the yielding
	// variant; shorter parks yield to a deadline instead.
	SleepFloorNs = int64(time.Millisecond)
)

// Backoff walks spin -> yield -> park-doubling on consecutive idle calls and
// resets on work. Single-goroutine use; not safe for concurrent Idle calls.
type Backoff struct {
	spins, yields int
	parkPeriodNs  int64
	// parker performs the park rung; injectable for the ladder tests.
	parker func(ns int64)
}

// NewBackoff returns the naive ladder: every park is a time.Sleep.
func NewBackoff() *Backoff {
	return &Backoff{parkPeriodNs: MinParkNs, parker: sleepPark}
}

// NewYieldingBackoff returns the ladder with sub-floor parks served by
// yielding to a deadline (aeron-go 1ce3720).
func NewYieldingBackoff() *Backoff {
	return &Backoff{parkPeriodNs: MinParkNs, parker: yieldFloorPark}
}

// Idle advances the ladder when workCount == 0 and resets it otherwise.
func (b *Backoff) Idle(workCount int) {
	if workCount > 0 {
		b.spins, b.yields, b.parkPeriodNs = 0, 0, MinParkNs
		return
	}
	switch {
	case b.spins < MaxSpins:
		// Busy rung: count only, matching aeron-go (Go exposes no plain
		// spin hint; procyield is runtime-internal).
		b.spins++
	case b.yields < MaxYields:
		b.yields++
		runtime.Gosched()
	default:
		b.parker(b.parkPeriodNs)
		if next := b.parkPeriodNs * 2; next <= MaxParkNs {
			b.parkPeriodNs = next
		} else {
			b.parkPeriodNs = MaxParkNs
		}
	}
}

func sleepPark(ns int64) {
	time.Sleep(time.Duration(ns))
}

func yieldFloorPark(ns int64) {
	if ns < SleepFloorNs {
		deadline := time.Now().Add(time.Duration(ns))
		for time.Now().Before(deadline) {
			runtime.Gosched()
		}
		return
	}
	time.Sleep(time.Duration(ns))
}
