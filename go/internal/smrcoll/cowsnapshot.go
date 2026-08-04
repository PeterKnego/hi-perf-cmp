package smrcoll

import (
	"fmt"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll/booksnap"
)

// EncodeRoot serializes a frozen CowRoot (header + body + crc32c) into the
// reused buffer; byte-identical to Encode for the same logical state.
func (s *Snapshotter) EncodeRoot(r *CowRoot) []byte {
	nLevels := 0
	for side := uint8(0); side < 2; side++ {
		for t := uint32(0); t < r.NLevels; t++ {
			if r.LevelAt(side, t).Head != NIL {
				nLevels++
			}
		}
	}
	buf := s.grow(encodedLength(nLevels, int(r.Hwm)))

	m := s.msg.WrapAndApplyHeader(buf, 0, uint64(len(buf)))
	m.SetPriceMin(r.PriceMin).SetTickSize(r.Tick).SetNLevels(r.NLevels).
		SetCapacity(r.Capacity).SetHwm(r.Hwm).
		SetBestBid(r.BestBid).SetBestAsk(r.BestAsk).SetFreeHead(r.FreeHead)

	lg := m.LevelsCount(uint16(nLevels))
	for side := uint8(0); side < 2; side++ {
		for t := uint32(0); t < r.NLevels; t++ {
			lvl := r.LevelAt(side, t)
			if lvl.Head == NIL {
				continue
			}
			lg.Next().SetSide(sideEnum(side)).SetLevelTick(t).
				SetQtyTotal(lvl.QtyTotal).SetOrderCount(lvl.Count).
				SetHead(lvl.Head).SetTail(lvl.Tail)
		}
	}
	og := m.OrdersCount(uint16(r.Hwm))
	for slot := uint32(0); slot < r.Hwm; slot++ {
		o := r.OrderAt(slot)
		og.Next().SetSlot(slot).SetOrderId(o.OrderID).SetPrice(o.Price).
			SetQty(o.Qty).SetFilled(o.Filled).SetSide(sideEnum(o.Side)).
			SetNextSlot(o.Next).SetPrev(o.Prev)
	}
	return s.seal(buf, m.EncodedLength())
}

// RestoreCow rebuilds a fresh CowBook from an encoded image, verifying crc32c.
func RestoreCow(data []byte, cfg bench.SmrConfig) (*CowBook, error) {
	var msg booksnap.BookSnapshot
	if err := decodeHeader(data, &msg); err != nil {
		return nil, err
	}

	b := NewCowBook(cfg)
	b.PriceMin = msg.PriceMin()
	b.Tick = msg.TickSize()
	b.NLevels = msg.NLevels()
	b.Hwm = msg.Hwm()
	b.BestBid = msg.BestBid()
	b.BestAsk = msg.BestAsk()
	if int(msg.Capacity()) != cfg.Cap {
		return nil, fmt.Errorf("snapshot capacity %d != SMRC_CAP %d", msg.Capacity(), cfg.Cap)
	}
	b.FreeHead = msg.FreeHead()

	for lg := msg.Levels(); lg.HasNext(); {
		lv := lg.Next()
		lvl := b.levelMut(sideU8(lv.Side()), lv.LevelTick())
		lvl.Head = lv.Head()
		lvl.Tail = lv.Tail()
		lvl.QtyTotal = lv.QtyTotal()
		lvl.Count = lv.OrderCount()
	}
	for og := msg.Orders(); og.HasNext(); {
		o := og.Next()
		*b.orderMut(o.Slot()) = Order{
			OrderID: o.OrderId(), Price: o.Price(), Qty: o.Qty(), Filled: o.Filled(),
			Next: o.NextSlot(), Prev: o.Prev(), Side: sideU8(o.Side()),
		}
	}
	b.rebuildIDs()
	return b, nil
}
