package smrcoll

import (
	"fmt"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

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
		if m.keys[i] == 0 {
			// empty slot marker (order IDs are always >= 1): key absent.
			return NIL
		}
		i = (i + 1) & m.mask
	}
	return m.vals[i]
}

// del removes k and backward-shift compacts the run behind it, so probe
// chains never accumulate tombstones and lookup cost stays flat over uptime.
// The resulting table is a pure function of the operations applied — which
// matters because the id-map is rebuilt from the pool on restore and both
// replicas must agree.
func (m *idMap) del(k int64) {
	i := (uint64(k) * 0x9E3779B97F4A7C15) & m.mask
	for m.keys[i] != k {
		if m.keys[i] == 0 {
			return // absent
		}
		i = (i + 1) & m.mask
	}
	m.keys[i] = 0
	j := i
	for {
		j = (j + 1) & m.mask
		if m.keys[j] == 0 {
			return
		}
		h := (uint64(m.keys[j]) * 0x9E3779B97F4A7C15) & m.mask
		// Move keys[j] into the hole at i iff its ideal slot h does NOT lie
		// cyclically within (i, j] — otherwise the move would place it before
		// its own probe start and make it unreachable.
		if ((j - h) & m.mask) >= ((j - i) & m.mask) {
			m.keys[i] = m.keys[j]
			m.vals[i] = m.vals[j]
			m.keys[j] = 0
			i = j
		}
	}
}

type Book struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Bids, Asks       []Level
	Pool             []Order
	Hwm              uint32
	FreeHead         uint32
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
		Hwm: 0, FreeHead: NIL, BestBid: -1, BestAsk: -1, ids: newIDMap(cfg.Cap),
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
	slot := b.allocSlot()
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
		if b.Pool[slot].OrderID != 0 {
			b.ids.put(b.Pool[slot].OrderID, slot)
		}
	}
}

func (b *Book) allocSlot() uint32 {
	if b.FreeHead != NIL {
		slot := b.FreeHead
		b.FreeHead = b.Pool[slot].Next
		return slot
	}
	if int(b.Hwm) == len(b.Pool) {
		panic(fmt.Sprintf("order pool exhausted: SMRC_CAP=%d reached", len(b.Pool)))
	}
	slot := b.Hwm
	b.Hwm++
	return slot
}

func (b *Book) freeSlot(slot uint32) {
	head := b.FreeHead
	o := &b.Pool[slot]
	o.OrderID = 0 // freed marker: the snapshot walk skips these
	o.Next = head
	o.Prev = NIL
	b.FreeHead = slot
}

// unlink removes slot from its level's intrusive FIFO and debits rem from the
// level's remaining quantity.
func (b *Book) unlink(slot uint32, side uint8, t uint32, rem int64) {
	prev, next := b.Pool[slot].Prev, b.Pool[slot].Next
	if prev != NIL {
		b.Pool[prev].Next = next
	}
	if next != NIL {
		b.Pool[next].Prev = prev
	}
	lvl := &b.lane(side)[t]
	if lvl.Head == slot {
		lvl.Head = next
	}
	if lvl.Tail == slot {
		lvl.Tail = prev
	}
	lvl.QtyTotal -= rem
	lvl.Count--
}

// repairBest restores the cached best for side after a removal emptied level
// t. O(levels) worst case and deliberately on the timed path — real books
// maintain this, and hiding it would hide the worst-case cancel.
func (b *Book) repairBest(side uint8, t uint32) {
	if side == 0 {
		if b.BestBid != int32(t) || b.Bids[t].Head != NIL {
			return
		}
		nb := int32(-1)
		for i := int(t); i >= 0; i-- {
			if b.Bids[i].Head != NIL {
				nb = int32(i)
				break
			}
		}
		b.BestBid = nb
		return
	}
	if b.BestAsk != int32(t) || b.Asks[t].Head != NIL {
		return
	}
	na := int32(-1)
	for i := int(t); i < int(b.NLevels); i++ {
		if b.Asks[i].Head != NIL {
			na = int32(i)
			break
		}
	}
	b.BestAsk = na
}

// Cancel removes a resting order; its remaining quantity leaves the level.
func (b *Book) Cancel(orderID int64) {
	slot := b.ids.get(orderID)
	o := b.Pool[slot]
	rem := o.Qty - o.Filled
	t := b.tickOf(o.Price)
	b.ids.del(orderID)
	b.unlink(slot, o.Side, t, rem)
	b.freeSlot(slot)
	b.repairBest(o.Side, t)
}

// Fill completes an order then removes it. Same structural work as Cancel;
// the difference is that the departing quantity is booked as filled rather
// than withdrawn.
func (b *Book) Fill(orderID int64) {
	slot := b.ids.get(orderID)
	o := &b.Pool[slot]
	rem := o.Qty - o.Filled
	o.Filled = o.Qty
	side, price := o.Side, o.Price
	t := b.tickOf(price)
	b.ids.del(orderID)
	b.unlink(slot, side, t, rem)
	b.freeSlot(slot)
	b.repairBest(side, t)
}
