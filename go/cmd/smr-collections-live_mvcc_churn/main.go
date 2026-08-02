// smr-collections-live_mvcc_churn (Go): writer-observed latency under the
// churn workload while a serializer goroutine encodes captured CoW roots
// concurrently.
package main

import (
	"runtime"
	"sync/atomic"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_mvcc_churn"

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
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	for i := 0; i < cfg.Warmup; i++ {
		smrcoll.ApplyChurn(book, churn.NextOp())
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
	var ins, can, fil []int64
	rssPeak := bench.RSSBytes()
	for k := 0; k < cfg.LiveIters; k++ {
		op := churn.NextOp()
		fired := k%cfg.SnapEvery == 0
		var captured bool
		t0 := time.Now()
		if fired {
			if busy.Load() {
				skipped++
			} else {
				busy.Store(true)
				ch <- capMsg{root: book.Capture(), t0: t0}
				captured = true
			}
		}
		smrcoll.ApplyChurn(book, op)
		ns := time.Since(t0).Nanoseconds()
		// Sample RSS only AFTER the clock closes: RSSBytes reads
		// /proc/self/statm — microseconds against sub-microsecond ops — so
		// calling it inside the timed region would inflate writer_max, the one
		// metric this cell exists to report precisely. Gated on captured (not
		// fired) so a skipped iteration — no capture sent, no capture-triggered
		// work — doesn't spend a sample.
		if captured {
			if r := bench.RSSBytes(); r > rssPeak {
				rssPeak = r
			}
		}
		writerNs[k] = ns
		switch op.Kind {
		case smrcoll.ChurnInsert:
			ins = append(ins, ns)
		case smrcoll.ChurnCancel:
			can = append(can, ns)
		case smrcoll.ChurnFill:
			fil = append(fil, ns)
		}
	}
	close(ch)
	<-done
	// Catch growth from the final in-flight window: the last capture's
	// EncodeRoot and its CoW chunk copies run concurrently and can still be
	// growing memory after the loop's last fired iteration.
	if r := bench.RSSBytes(); r > rssPeak {
		rssPeak = r
	}
	// The first recorded duration is the warm handshake above (the
	// serializer is a single-consumer FIFO over the channel, so send order
	// == process order) — exclude it from the emitted stats and counts.
	if len(snapNs) > 0 {
		snapNs = snapNs[1:]
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, skipped, snapLen)
	if len(ins) > 0 {
		bench.EmitSmrLatency(experiment, "insert", ins)
	}
	if len(can) > 0 {
		bench.EmitSmrLatency(experiment, "cancel", can)
	}
	if len(fil) > 0 {
		bench.EmitSmrLatency(experiment, "fill", fil)
	}
	bench.EmitSmrInt(experiment, "rss_peak_bytes", rssPeak, "bytes", 1)
}
