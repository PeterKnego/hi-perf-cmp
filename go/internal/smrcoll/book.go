package smrcoll

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

// NIL sentinel handle (empty head/tail, link end).
const NIL uint32 = 0xFFFFFFFF

type Order struct {
	OrderID, Price, Qty, Filled int64
	Next, Prev                  uint32
	Side                        uint8
}

type Level struct {
	Head, Tail uint32
	QtyTotal   int64
	Count      uint32
}

// idMap is a hand-rolled open-addressing int64->uint32 map with fixed
// (Fibonacci) hashing — no map[] boxing/rehash, deterministic probing.
// Empty slot marker: key == 0 (orderIDs are >= 1).
type idMap struct {
	keys []int64
	vals []uint32
	mask uint64
}

func newIDMap(capacity int) *idMap {
	// next power of two >= 2*capacity for a <=50% load factor.
	n := 1
	for n < capacity*2 {
		n <<= 1
	}
	return &idMap{keys: make([]int64, n), vals: make([]uint32, n), mask: uint64(n - 1)}
}

func (m *idMap) put(k int64, v uint32) {
	i := (uint64(k) * 0x9E3779B97F4A7C15) & m.mask
	for m.keys[i] != 0 && m.keys[i] != k {
		i = (i + 1) & m.mask
	}
	m.keys[i] = k
	m.vals[i] = v
}

func (m *idMap) get(k int64) uint32 {
	i := (uint64(k) * 0x9E3779B97F4A7C15) & m.mask
	for m.keys[i] != k {
		i = (i + 1) & m.mask
	}
	return m.vals[i]
}

type Book struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Bids, Asks       []Level
	Pool             []Order
	Hwm              uint32
	BestBid, BestAsk int32
	ids              *idMap
}

func NewBook(cfg bench.SmrConfig) *Book {
	n := int(cfg.Levels)
	bids := make([]Level, n)
	asks := make([]Level, n)
	for i := range bids {
		bids[i] = Level{Head: NIL, Tail: NIL}
		asks[i] = Level{Head: NIL, Tail: NIL}
	}
	return &Book{
		PriceMin: cfg.PriceMin, Tick: cfg.Tick, NLevels: cfg.Levels,
		Bids: bids, Asks: asks, Pool: make([]Order, cfg.Cap),
		Hwm: 0, BestBid: -1, BestAsk: -1, ids: newIDMap(cfg.Cap),
	}
}

func (b *Book) tickOf(price int64) uint32 { return uint32((price - b.PriceMin) / b.Tick) }
func (b *Book) lane(side uint8) []Level {
	if side == 0 {
		return b.Bids
	}
	return b.Asks
}

func (b *Book) Insert(orderID, price, qty int64, side uint8) {
	t := b.tickOf(price)
	slot := b.Hwm
	b.Hwm++
	lane := b.lane(side)
	lvl := &lane[t]
	prevTail := lvl.Tail
	b.Pool[slot] = Order{OrderID: orderID, Price: price, Qty: qty, Filled: 0, Next: NIL, Prev: prevTail, Side: side}
	if prevTail != NIL {
		b.Pool[prevTail].Next = slot
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

func (b *Book) Update(orderID, fillQty int64) {
	slot := b.ids.get(orderID)
	o := &b.Pool[slot]
	add := fillQty
	if rem := o.Qty - o.Filled; add > rem {
		add = rem
	}
	o.Filled += add
	t := b.tickOf(o.Price)
	b.lane(o.Side)[t].QtyTotal -= add
}

func (b *Book) GetSlot(orderID int64) uint32 { return b.ids.get(orderID) }
func (b *Book) BestBidTick() int32           { return b.BestBid }
func (b *Book) BestAskTick() int32           { return b.BestAsk }
func (b *Book) HwmVal() uint32               { return b.Hwm }
func (b *Book) LevelQty(side uint8, tick uint32) int64 {
	return b.lane(side)[tick].QtyTotal
}

// rebuildIDs re-indexes the id-map from the pool (used after restore).
func (b *Book) rebuildIDs() {
	b.ids = newIDMap(len(b.Pool))
	for slot := uint32(0); slot < b.Hwm; slot++ {
		b.ids.put(b.Pool[slot].OrderID, slot)
	}
}
