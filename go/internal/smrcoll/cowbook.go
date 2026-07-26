package smrcoll

// Chunked copy-on-write LOB: same logical behavior as Book, but the pool and
// ladder live in fixed-size chunks behind chunk tables. Capture() clones the
// chunk-ref slices (O(#chunks)) and bumps the generation; the writer copies a
// chunk before its first write after a capture (born < gen), so a frozen
// CowRoot is never mutated. GC reclaims dropped chunks. The copy decision is
// ALWAYS the epoch, never pointer comparison.

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

// levelChunkLen is the fixed ladder chunk size (orders-per-chunk is SMRC_CHUNK).
const levelChunkLen = 256

type orderChunk struct {
	born   uint64
	orders []Order
}

type lvlChunk struct {
	born   uint64
	levels []Level
}

// CowRoot is a frozen point-in-time view: chunk refs + scalars. Safe to hand
// to another goroutine (channel handoff gives the happens-before edge).
type CowRoot struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Capacity, Hwm    uint32
	BestBid, BestAsk int32
	chunk            int
	orderChunks      []*orderChunk
	bidChunks        []*lvlChunk
	askChunks        []*lvlChunk
}

func (r *CowRoot) OrderAt(slot uint32) *Order {
	return &r.orderChunks[int(slot)/r.chunk].orders[int(slot)%r.chunk]
}

func (r *CowRoot) LevelAt(side uint8, t uint32) *Level {
	lane := r.bidChunks
	if side == 1 {
		lane = r.askChunks
	}
	return &lane[int(t)/levelChunkLen].levels[int(t)%levelChunkLen]
}

type CowBook struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Chunk            int
	capacity         int
	gen              uint64
	orderChunks      []*orderChunk
	bidChunks        []*lvlChunk
	askChunks        []*lvlChunk
	Hwm              uint32
	BestBid, BestAsk int32
	ids              *idMap
}

func NewCowBook(cfg bench.SmrConfig) *CowBook {
	nOC := (cfg.Cap + cfg.Chunk - 1) / cfg.Chunk
	ocs := make([]*orderChunk, nOC)
	for ci := range ocs {
		n := cfg.Chunk
		if rem := cfg.Cap - ci*cfg.Chunk; rem < n {
			n = rem
		}
		ocs[ci] = &orderChunk{born: 1, orders: make([]Order, n)}
	}
	mkLane := func() []*lvlChunk {
		nLC := (int(cfg.Levels) + levelChunkLen - 1) / levelChunkLen
		lcs := make([]*lvlChunk, nLC)
		for ci := range lcs {
			n := levelChunkLen
			if rem := int(cfg.Levels) - ci*levelChunkLen; rem < n {
				n = rem
			}
			ls := make([]Level, n)
			for i := range ls {
				ls[i] = Level{Head: NIL, Tail: NIL}
			}
			lcs[ci] = &lvlChunk{born: 1, levels: ls}
		}
		return lcs
	}
	return &CowBook{
		PriceMin: cfg.PriceMin, Tick: cfg.Tick, NLevels: cfg.Levels,
		Chunk: cfg.Chunk, capacity: cfg.Cap, gen: 1,
		orderChunks: ocs, bidChunks: mkLane(), askChunks: mkLane(),
		BestBid: -1, BestAsk: -1, ids: newIDMap(cfg.Cap),
	}
}

func (b *CowBook) tickOf(price int64) uint32 { return uint32((price - b.PriceMin) / b.Tick) }

func (b *CowBook) laneChunks(side uint8) []*lvlChunk {
	if side == 0 {
		return b.bidChunks
	}
	return b.askChunks
}

func (b *CowBook) OrderAt(slot uint32) *Order {
	return &b.orderChunks[int(slot)/b.Chunk].orders[int(slot)%b.Chunk]
}

func (b *CowBook) LevelAt(side uint8, t uint32) *Level {
	lane := b.laneChunks(side)
	return &lane[int(t)/levelChunkLen].levels[int(t)%levelChunkLen]
}

func (b *CowBook) orderMut(slot uint32) *Order {
	ci := int(slot) / b.Chunk
	c := b.orderChunks[ci]
	if c.born < b.gen {
		cp := &orderChunk{born: b.gen, orders: make([]Order, len(c.orders))}
		copy(cp.orders, c.orders)
		b.orderChunks[ci] = cp
		c = cp
	}
	return &c.orders[int(slot)%b.Chunk]
}

func (b *CowBook) levelMut(side uint8, t uint32) *Level {
	lane := b.laneChunks(side)
	ci := int(t) / levelChunkLen
	c := lane[ci]
	if c.born < b.gen {
		cp := &lvlChunk{born: b.gen, levels: make([]Level, len(c.levels))}
		copy(cp.levels, c.levels)
		lane[ci] = cp
		c = cp
	}
	return &c.levels[int(t)%levelChunkLen]
}

// Insert mirrors Book.Insert (keep in lockstep).
func (b *CowBook) Insert(orderID, price, qty int64, side uint8) {
	t := b.tickOf(price)
	slot := b.Hwm
	b.Hwm++
	prevTail := b.LevelAt(side, t).Tail
	*b.orderMut(slot) = Order{OrderID: orderID, Price: price, Qty: qty, Filled: 0, Next: NIL, Prev: prevTail, Side: side}
	lvl := b.levelMut(side, t)
	if prevTail != NIL {
		b.orderMut(prevTail).Next = slot
	} else {
		lvl.Head = slot
	}
	lvl.Tail = slot
	lvl.QtyTotal += qty
	lvl.Count++
	b.ids.put(orderID, slot)
	if side == 0 && (b.BestBid < 0 || int32(t) > b.BestBid) {
		b.BestBid = int32(t)
	}
	if side == 1 && (b.BestAsk < 0 || int32(t) < b.BestAsk) {
		b.BestAsk = int32(t)
	}
}

// Update mirrors Book.Update (keep in lockstep).
func (b *CowBook) Update(orderID, fillQty int64) {
	slot := b.ids.get(orderID)
	o := b.orderMut(slot)
	add := fillQty
	if rem := o.Qty - o.Filled; add > rem {
		add = rem
	}
	o.Filled += add
	t := b.tickOf(o.Price)
	b.levelMut(o.Side, t).QtyTotal -= add
}

// Capture freezes the current state (O(#chunks)) and bumps the generation.
func (b *CowBook) Capture() *CowRoot {
	root := &CowRoot{
		PriceMin: b.PriceMin, Tick: b.Tick, NLevels: b.NLevels,
		Capacity: uint32(b.capacity), Hwm: b.Hwm,
		BestBid: b.BestBid, BestAsk: b.BestAsk, chunk: b.Chunk,
		orderChunks: append([]*orderChunk(nil), b.orderChunks...),
		bidChunks:   append([]*lvlChunk(nil), b.bidChunks...),
		askChunks:   append([]*lvlChunk(nil), b.askChunks...),
	}
	b.gen++
	return root
}

func (b *CowBook) GetSlot(orderID int64) uint32 { return b.ids.get(orderID) }

func (b *CowBook) LevelQty(side uint8, tick uint32) int64 { return b.LevelAt(side, tick).QtyTotal }

// rebuildIDs re-indexes the id-map from the pool (used after restore).
func (b *CowBook) rebuildIDs() {
	b.ids = newIDMap(b.capacity)
	for slot := uint32(0); slot < b.Hwm; slot++ {
		b.ids.put(b.OrderAt(slot).OrderID, slot)
	}
}
