// smr-collections-live_mvcc (Go): writer-observed latency while a serializer
// goroutine encodes captured CoW roots concurrently.
package main

import (
	"runtime"
	"sync/atomic"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_mvcc"

type capMsg struct {
	root *smrcoll.CowRoot
	t0   time.Time
}

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	n := cfg.Steady
	for i := 0; i < cfg.Warmup; i++ {
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
	}

	var busy atomic.Bool
	ch := make(chan capMsg, 1)
	done := make(chan struct{})
	var snapNs []int64
	var snapLen int64
	go func() {
		s := smrcoll.NewSnapshotter()
		for m := range ch {
			img := s.EncodeRoot(m.root)
			snapLen = int64(len(img))
			snapNs = append(snapNs, time.Since(m.t0).Nanoseconds())
			busy.Store(false)
		}
		close(done)
	}()

	// Warm handshake: push one untimed capture through the serializer before
	// the timed loop starts, so first-touch page faults of its encode buffer
	// land here instead of in the first timed sample. Same message shape as
	// the timed sends; the first recorded duration is dropped below.
	busy.Store(true)
	ch <- capMsg{root: book.Capture(), t0: time.Now()}
	for busy.Load() {
		runtime.Gosched()
	}

	writerNs := make([]int64, cfg.LiveIters)
	var skipped int64
	for k := 0; k < cfg.LiveIters; k++ {
		t0 := time.Now()
		if k%cfg.SnapEvery == 0 {
			if busy.Load() {
				skipped++
			} else {
				busy.Store(true)
				ch <- capMsg{root: book.Capture(), t0: t0}
			}
		}
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
		writerNs[k] = time.Since(t0).Nanoseconds()
	}
	close(ch)
	<-done
	// The first recorded duration is the warm handshake above (the
	// serializer is a single-consumer FIFO over the channel, so send order
	// == process order) — exclude it from the emitted stats and counts.
	if len(snapNs) > 0 {
		snapNs = snapNs[1:]
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, skipped, snapLen)
}
