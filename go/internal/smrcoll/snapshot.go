package smrcoll

import (
	"encoding/binary"
	"fmt"
	"hash/crc32"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll/booksnap"
)

var crc32cTable = crc32.MakeTable(crc32.Castagnoli)

// sbeMeta is an unwrapped flyweight used only for the schema constants.
var sbeMeta booksnap.BookSnapshot

// messageHeaderLength is the SBE frame header preceding the message body.
const messageHeaderLength = 8

// Snapshotter holds the reusable output buffer and message flyweight so that
// repeated Encode calls avoid re-allocating the SBE machinery.
//
// The codec is generated in flyweight mode
// (`-Dsbe.go.generate.generate.flyweights=true`, see regen-booksnap.sh), so
// fields are written straight into the output buffer at computed offsets. The
// default owned-struct codegen instead materializes every level and order as a
// Go struct and writes each field through an io.Writer, which costs ~5.5x more
// on this image — the same flyweight-vs-struct gap the serialization focus area
// measures on a single record.
type Snapshotter struct {
	buf []byte
	msg booksnap.BookSnapshot
}

func NewSnapshotter() *Snapshotter { return &Snapshotter{} }

func sideEnum(side uint8) booksnap.Side {
	if side == 0 {
		return booksnap.Side_BID
	}
	return booksnap.Side_ASK
}

func sideU8(s booksnap.Side) uint8 {
	if s == booksnap.Side_ASK {
		return 1
	}
	return 0
}

// encodedLength returns the exact image size for nLevels levels and nOrders
// orders, so the buffer is sized once rather than grown.
func encodedLength(nLevels, nOrders int) int {
	return messageHeaderLength + int(sbeMeta.SbeBlockLength()) +
		groupHeaderLength + nLevels*levelBlockLength +
		groupHeaderLength + nOrders*orderBlockLength + crcLength
}

const (
	groupHeaderLength = 4
	levelBlockLength  = 25
	orderBlockLength  = 45
	crcLength         = 4
)

// Encode serializes the book (header + body + 4-byte crc32c) into the reused
// buffer and returns the bytes (valid until the next Encode call).
func (s *Snapshotter) Encode(b *Book) []byte {
	nLevels := 0
	for _, lane := range [2][]Level{b.Bids, b.Asks} {
		for i := range lane {
			if lane[i].Head != NIL {
				nLevels++
			}
		}
	}
	buf := s.grow(encodedLength(nLevels, int(b.Hwm)))

	m := s.msg.WrapAndApplyHeader(buf, 0, uint64(len(buf)))
	m.SetPriceMin(b.PriceMin).SetTickSize(b.Tick).SetNLevels(b.NLevels).
		SetCapacity(uint32(len(b.Pool))).SetHwm(b.Hwm).
		SetBestBid(b.BestBid).SetBestAsk(b.BestAsk).SetFreeHead(b.FreeHead)

	lg := m.LevelsCount(uint16(nLevels))
	for side, lane := range [2][]Level{b.Bids, b.Asks} {
		for t := range lane {
			lvl := lane[t]
			if lvl.Head == NIL {
				continue
			}
			lg.Next().SetSide(sideEnum(uint8(side))).SetLevelTick(uint32(t)).
				SetQtyTotal(lvl.QtyTotal).SetOrderCount(lvl.Count).
				SetHead(lvl.Head).SetTail(lvl.Tail)
		}
	}
	og := m.OrdersCount(uint16(b.Hwm))
	for slot := uint32(0); slot < b.Hwm; slot++ {
		o := &b.Pool[slot]
		og.Next().SetSlot(slot).SetOrderId(o.OrderID).SetPrice(o.Price).
			SetQty(o.Qty).SetFilled(o.Filled).SetSide(sideEnum(o.Side)).
			SetNextSlot(o.Next).SetPrev(o.Prev)
	}
	return s.seal(buf, m.EncodedLength())
}

// grow returns a buffer of exactly n bytes, reusing the previous allocation
// when it is large enough.
func (s *Snapshotter) grow(n int) []byte {
	if cap(s.buf) < n {
		s.buf = make([]byte, n)
	}
	return s.buf[:n]
}

// seal appends the crc32c trailer over everything written so far.
func (s *Snapshotter) seal(buf []byte, bodyLength uint64) []byte {
	end := messageHeaderLength + int(bodyLength)
	binary.LittleEndian.PutUint32(buf[end:], crc32.Checksum(buf[:end], crc32cTable))
	return buf[:end+crcLength]
}

// decodeHeader verifies the crc32c trailer and schema version, then wraps the
// message body for decoding.
func decodeHeader(data []byte, msg *booksnap.BookSnapshot) error {
	if len(data) < crcLength {
		return fmt.Errorf("snapshot too short")
	}
	sbeLen := len(data) - crcLength
	want := binary.LittleEndian.Uint32(data[sbeLen:])
	if crc32.Checksum(data[:sbeLen], crc32cTable) != want {
		return fmt.Errorf("crc32c mismatch")
	}
	var hdr booksnap.MessageHeader
	hdr.Wrap(data, 0, uint64(sbeMeta.SbeSchemaVersion()), uint64(sbeLen))
	if hdr.Version() != sbeMeta.SbeSchemaVersion() {
		return fmt.Errorf("unsupported snapshot schema version %d (expected %d)",
			hdr.Version(), sbeMeta.SbeSchemaVersion())
	}
	msg.WrapForDecode(data, messageHeaderLength,
		uint64(hdr.BlockLength()), uint64(hdr.Version()), uint64(sbeLen))
	return nil
}

// Restore rebuilds a fresh book from an encoded image, verifying the crc32c.
func Restore(data []byte, cfg bench.SmrConfig) (*Book, error) {
	var msg booksnap.BookSnapshot
	if err := decodeHeader(data, &msg); err != nil {
		return nil, err
	}

	b := NewBook(cfg)
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
		lane := b.Bids
		if sideU8(lv.Side()) == 1 {
			lane = b.Asks
		}
		lane[lv.LevelTick()] = Level{
			Head: lv.Head(), Tail: lv.Tail(), QtyTotal: lv.QtyTotal(), Count: lv.OrderCount(),
		}
	}
	for og := msg.Orders(); og.HasNext(); {
		o := og.Next()
		b.Pool[o.Slot()] = Order{
			OrderID: o.OrderId(), Price: o.Price(), Qty: o.Qty(), Filled: o.Filled(),
			Next: o.NextSlot(), Prev: o.Prev(), Side: sideU8(o.Side()),
		}
	}
	b.rebuildIDs()
	return b, nil
}
