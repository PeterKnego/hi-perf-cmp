// The churn workload: a deterministic insert/cancel/fill stream at a
// configurable order-to-trade ratio (default 1 %, the real-exchange figure).
//
// Op generation sits outside the timed region — the driver produces an op,
// the caller times only the store's application of it, so the per-op numbers
// are store work alone. Note this makes them NOT directly comparable with the
// older insert/update cells, which time their own generation; see the design
// spec's "Must be recorded in the next run's journal entry".
package smrcoll

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

// ChurnStore is the store surface a churn stream drives. Book and CowBook
// both satisfy it structurally.
type ChurnStore interface {
	Insert(orderID, price, qty int64, side uint8)
	Cancel(orderID int64)
	Fill(orderID int64)
}

type ChurnOpKind uint8

const (
	ChurnInsert ChurnOpKind = iota
	ChurnCancel
	ChurnFill
)

type ChurnOp struct {
	Kind                ChurnOpKind
	OrderID, Price, Qty int64
	Side                uint8
}

type Churn struct {
	rng *SplitMix
	// live holds the order IDs currently resting, dense so a victim is one
	// uniform draw.
	live []int64
	// i is the global op index: it drives both the insert/depart alternation
	// and the order ID, so IDs are sparse (1, 3, 5, …) but never reused.
	i        int
	otrBps   uint64
	levels   uint32
	tick     int64
	priceMin int64
}

func NewChurn(cfg bench.SmrConfig) *Churn {
	return &Churn{
		rng:      NewSplitMix(SmrSeed),
		live:     make([]int64, 0, cfg.Cap),
		otrBps:   uint64(cfg.OtrBps),
		levels:   cfg.Levels,
		tick:     cfg.Tick,
		priceMin: cfg.PriceMin,
	}
}

func (c *Churn) insertOp() ChurnOp {
	ins := NextInsert(c.rng, c.i, c.levels, c.tick, c.priceMin)
	c.i++
	c.live = append(c.live, ins.OrderID)
	return ChurnOp{Kind: ChurnInsert, OrderID: ins.OrderID, Price: ins.Price, Qty: ins.Qty, Side: ins.Side}
}

// NextOp returns the next op. Even index inserts, odd index departs; a
// departure is a fill with probability otrBps/10000, otherwise a cancel.
func (c *Churn) NextOp() ChurnOp {
	if c.i%2 == 0 || len(c.live) == 0 {
		return c.insertOp()
	}
	c.i++
	v := int(c.rng.Next() % uint64(len(c.live)))
	id := c.live[v]
	isFill := c.rng.Next()%10000 < c.otrBps
	// swap-remove, matching Rust's Vec::swap_remove exactly — the two op
	// streams must be identical.
	c.live[v] = c.live[len(c.live)-1]
	c.live = c.live[:len(c.live)-1]
	if isFill {
		return ChurnOp{Kind: ChurnFill, OrderID: id}
	}
	return ChurnOp{Kind: ChurnCancel, OrderID: id}
}

// Prebuild brings the store to its steady-state live set with inserts only.
func (c *Churn) Prebuild(store ChurnStore, steady int) {
	for i := 0; i < steady; i++ {
		ApplyChurn(store, c.insertOp())
	}
}

func ApplyChurn(store ChurnStore, op ChurnOp) {
	switch op.Kind {
	case ChurnInsert:
		store.Insert(op.OrderID, op.Price, op.Qty, op.Side)
	case ChurnCancel:
		store.Cancel(op.OrderID)
	case ChurnFill:
		store.Fill(op.OrderID)
	}
}

type ChurnSamples struct {
	InsertNs, CancelNs, FillNs []int64
}

// RunChurn warms up, then times cfg.Iters ops into per-op-type sample slices.
// Only the store call is inside the clock. Returns the samples and the RSS
// baseline taken at the clock boundary — after warmup and after the sample
// slices are allocated, so neither is counted as store growth.
func RunChurn(cfg bench.SmrConfig, store ChurnStore, c *Churn) (ChurnSamples, int64) {
	for i := 0; i < cfg.Warmup; i++ {
		ApplyChurn(store, c.NextOp())
	}
	half := cfg.Iters/2 + 1
	s := ChurnSamples{
		InsertNs: make([]int64, half),
		CancelNs: make([]int64, half),
		FillNs:   make([]int64, half),
	}
	// make() zeroes, so the pages are already resident; reslice to empty and
	// keep the capacity so the timed loop never allocates.
	s.InsertNs, s.CancelNs, s.FillNs = s.InsertNs[:0], s.CancelNs[:0], s.FillNs[:0]
	rss0 := bench.RSSBytes()
	for i := 0; i < cfg.Iters; i++ {
		op := c.NextOp()
		t0 := time.Now()
		ApplyChurn(store, op)
		ns := time.Since(t0).Nanoseconds()
		switch op.Kind {
		case ChurnInsert:
			s.InsertNs = append(s.InsertNs, ns)
		case ChurnCancel:
			s.CancelNs = append(s.CancelNs, ns)
		case ChurnFill:
			s.FillNs = append(s.FillNs, ns)
		}
	}
	return s, rss0
}

// EmitChurn emits the per-op-type distributions plus RSS growth. A
// distribution with no samples is skipped rather than emitted as zeros — at
// SMRC_OTR_BPS=0 there are no fills, and a fabricated zero would read as a
// real measurement.
func EmitChurn(experiment string, s ChurnSamples, rssGrowth int64) {
	if len(s.InsertNs) > 0 {
		bench.EmitSmrLatency(experiment, "insert", s.InsertNs)
	}
	if len(s.CancelNs) > 0 {
		bench.EmitSmrLatency(experiment, "cancel", s.CancelNs)
	}
	if len(s.FillNs) > 0 {
		bench.EmitSmrLatency(experiment, "fill", s.FillNs)
	}
	bench.EmitSmrInt(experiment, "rss_growth_bytes", rssGrowth, "bytes", 1)
}
