// Generated SBE (Simple Binary Encoding) message codec

package booksnap

import (
	"fmt"
	"io"
	"io/ioutil"
	"math"
)

type BookSnapshot struct {
	PriceMin int64
	TickSize int64
	NLevels  uint32
	Capacity uint32
	Hwm      uint32
	BestBid  int32
	BestAsk  int32
	Levels   []BookSnapshotLevels
	Orders   []BookSnapshotOrders
}
type BookSnapshotLevels struct {
	Side       SideEnum
	LevelTick  uint32
	QtyTotal   int64
	OrderCount uint32
	Head       uint32
	Tail       uint32
}
type BookSnapshotOrders struct {
	Slot     uint32
	OrderId  int64
	Price    int64
	Qty      int64
	Filled   int64
	Side     SideEnum
	NextSlot uint32
	Prev     uint32
}

func (b *BookSnapshot) Encode(_m *SbeGoMarshaller, _w io.Writer, doRangeCheck bool) error {
	if doRangeCheck {
		if err := b.RangeCheck(b.SbeSchemaVersion(), b.SbeSchemaVersion()); err != nil {
			return err
		}
	}
	if err := _m.WriteInt64(_w, b.PriceMin); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.TickSize); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.NLevels); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.Capacity); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.Hwm); err != nil {
		return err
	}
	if err := _m.WriteInt32(_w, b.BestBid); err != nil {
		return err
	}
	if err := _m.WriteInt32(_w, b.BestAsk); err != nil {
		return err
	}
	var LevelsBlockLength uint16 = 25
	if err := _m.WriteUint16(_w, LevelsBlockLength); err != nil {
		return err
	}
	var LevelsNumInGroup uint16 = uint16(len(b.Levels))
	if err := _m.WriteUint16(_w, LevelsNumInGroup); err != nil {
		return err
	}
	for i := range b.Levels {
		if err := b.Levels[i].Encode(_m, _w); err != nil {
			return err
		}
	}
	var OrdersBlockLength uint16 = 45
	if err := _m.WriteUint16(_w, OrdersBlockLength); err != nil {
		return err
	}
	var OrdersNumInGroup uint16 = uint16(len(b.Orders))
	if err := _m.WriteUint16(_w, OrdersNumInGroup); err != nil {
		return err
	}
	for i := range b.Orders {
		if err := b.Orders[i].Encode(_m, _w); err != nil {
			return err
		}
	}
	return nil
}

func (b *BookSnapshot) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16, blockLength uint16, doRangeCheck bool) error {
	if !b.PriceMinInActingVersion(actingVersion) {
		b.PriceMin = b.PriceMinNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.PriceMin); err != nil {
			return err
		}
	}
	if !b.TickSizeInActingVersion(actingVersion) {
		b.TickSize = b.TickSizeNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.TickSize); err != nil {
			return err
		}
	}
	if !b.NLevelsInActingVersion(actingVersion) {
		b.NLevels = b.NLevelsNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.NLevels); err != nil {
			return err
		}
	}
	if !b.CapacityInActingVersion(actingVersion) {
		b.Capacity = b.CapacityNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Capacity); err != nil {
			return err
		}
	}
	if !b.HwmInActingVersion(actingVersion) {
		b.Hwm = b.HwmNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Hwm); err != nil {
			return err
		}
	}
	if !b.BestBidInActingVersion(actingVersion) {
		b.BestBid = b.BestBidNullValue()
	} else {
		if err := _m.ReadInt32(_r, &b.BestBid); err != nil {
			return err
		}
	}
	if !b.BestAskInActingVersion(actingVersion) {
		b.BestAsk = b.BestAskNullValue()
	} else {
		if err := _m.ReadInt32(_r, &b.BestAsk); err != nil {
			return err
		}
	}
	if actingVersion > b.SbeSchemaVersion() && blockLength > b.SbeBlockLength() {
		io.CopyN(ioutil.Discard, _r, int64(blockLength-b.SbeBlockLength()))
	}

	if b.LevelsInActingVersion(actingVersion) {
		var LevelsBlockLength uint16
		if err := _m.ReadUint16(_r, &LevelsBlockLength); err != nil {
			return err
		}
		var LevelsNumInGroup uint16
		if err := _m.ReadUint16(_r, &LevelsNumInGroup); err != nil {
			return err
		}
		if cap(b.Levels) < int(LevelsNumInGroup) {
			b.Levels = make([]BookSnapshotLevels, LevelsNumInGroup)
		}
		b.Levels = b.Levels[:LevelsNumInGroup]
		for i := range b.Levels {
			if err := b.Levels[i].Decode(_m, _r, actingVersion, uint(LevelsBlockLength)); err != nil {
				return err
			}
		}
	}

	if b.OrdersInActingVersion(actingVersion) {
		var OrdersBlockLength uint16
		if err := _m.ReadUint16(_r, &OrdersBlockLength); err != nil {
			return err
		}
		var OrdersNumInGroup uint16
		if err := _m.ReadUint16(_r, &OrdersNumInGroup); err != nil {
			return err
		}
		if cap(b.Orders) < int(OrdersNumInGroup) {
			b.Orders = make([]BookSnapshotOrders, OrdersNumInGroup)
		}
		b.Orders = b.Orders[:OrdersNumInGroup]
		for i := range b.Orders {
			if err := b.Orders[i].Decode(_m, _r, actingVersion, uint(OrdersBlockLength)); err != nil {
				return err
			}
		}
	}
	if doRangeCheck {
		if err := b.RangeCheck(actingVersion, b.SbeSchemaVersion()); err != nil {
			return err
		}
	}
	return nil
}

func (b *BookSnapshot) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if b.PriceMinInActingVersion(actingVersion) {
		if b.PriceMin < b.PriceMinMinValue() || b.PriceMin > b.PriceMinMaxValue() {
			return fmt.Errorf("Range check failed on b.PriceMin (%v < %v > %v)", b.PriceMinMinValue(), b.PriceMin, b.PriceMinMaxValue())
		}
	}
	if b.TickSizeInActingVersion(actingVersion) {
		if b.TickSize < b.TickSizeMinValue() || b.TickSize > b.TickSizeMaxValue() {
			return fmt.Errorf("Range check failed on b.TickSize (%v < %v > %v)", b.TickSizeMinValue(), b.TickSize, b.TickSizeMaxValue())
		}
	}
	if b.NLevelsInActingVersion(actingVersion) {
		if b.NLevels < b.NLevelsMinValue() || b.NLevels > b.NLevelsMaxValue() {
			return fmt.Errorf("Range check failed on b.NLevels (%v < %v > %v)", b.NLevelsMinValue(), b.NLevels, b.NLevelsMaxValue())
		}
	}
	if b.CapacityInActingVersion(actingVersion) {
		if b.Capacity < b.CapacityMinValue() || b.Capacity > b.CapacityMaxValue() {
			return fmt.Errorf("Range check failed on b.Capacity (%v < %v > %v)", b.CapacityMinValue(), b.Capacity, b.CapacityMaxValue())
		}
	}
	if b.HwmInActingVersion(actingVersion) {
		if b.Hwm < b.HwmMinValue() || b.Hwm > b.HwmMaxValue() {
			return fmt.Errorf("Range check failed on b.Hwm (%v < %v > %v)", b.HwmMinValue(), b.Hwm, b.HwmMaxValue())
		}
	}
	if b.BestBidInActingVersion(actingVersion) {
		if b.BestBid < b.BestBidMinValue() || b.BestBid > b.BestBidMaxValue() {
			return fmt.Errorf("Range check failed on b.BestBid (%v < %v > %v)", b.BestBidMinValue(), b.BestBid, b.BestBidMaxValue())
		}
	}
	if b.BestAskInActingVersion(actingVersion) {
		if b.BestAsk < b.BestAskMinValue() || b.BestAsk > b.BestAskMaxValue() {
			return fmt.Errorf("Range check failed on b.BestAsk (%v < %v > %v)", b.BestAskMinValue(), b.BestAsk, b.BestAskMaxValue())
		}
	}
	for i := range b.Levels {
		if err := b.Levels[i].RangeCheck(actingVersion, schemaVersion); err != nil {
			return err
		}
	}
	for i := range b.Orders {
		if err := b.Orders[i].RangeCheck(actingVersion, schemaVersion); err != nil {
			return err
		}
	}
	return nil
}

func BookSnapshotInit(b *BookSnapshot) {
	return
}

func (b *BookSnapshotLevels) Encode(_m *SbeGoMarshaller, _w io.Writer) error {
	if err := b.Side.Encode(_m, _w); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.LevelTick); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.QtyTotal); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.OrderCount); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.Head); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.Tail); err != nil {
		return err
	}
	return nil
}

func (b *BookSnapshotLevels) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16, blockLength uint) error {
	if b.SideInActingVersion(actingVersion) {
		if err := b.Side.Decode(_m, _r, actingVersion); err != nil {
			return err
		}
	}
	if !b.LevelTickInActingVersion(actingVersion) {
		b.LevelTick = b.LevelTickNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.LevelTick); err != nil {
			return err
		}
	}
	if !b.QtyTotalInActingVersion(actingVersion) {
		b.QtyTotal = b.QtyTotalNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.QtyTotal); err != nil {
			return err
		}
	}
	if !b.OrderCountInActingVersion(actingVersion) {
		b.OrderCount = b.OrderCountNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.OrderCount); err != nil {
			return err
		}
	}
	if !b.HeadInActingVersion(actingVersion) {
		b.Head = b.HeadNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Head); err != nil {
			return err
		}
	}
	if !b.TailInActingVersion(actingVersion) {
		b.Tail = b.TailNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Tail); err != nil {
			return err
		}
	}
	if actingVersion > b.SbeSchemaVersion() && blockLength > b.SbeBlockLength() {
		io.CopyN(ioutil.Discard, _r, int64(blockLength-b.SbeBlockLength()))
	}
	return nil
}

func (b *BookSnapshotLevels) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if err := b.Side.RangeCheck(actingVersion, schemaVersion); err != nil {
		return err
	}
	if b.LevelTickInActingVersion(actingVersion) {
		if b.LevelTick < b.LevelTickMinValue() || b.LevelTick > b.LevelTickMaxValue() {
			return fmt.Errorf("Range check failed on b.LevelTick (%v < %v > %v)", b.LevelTickMinValue(), b.LevelTick, b.LevelTickMaxValue())
		}
	}
	if b.QtyTotalInActingVersion(actingVersion) {
		if b.QtyTotal < b.QtyTotalMinValue() || b.QtyTotal > b.QtyTotalMaxValue() {
			return fmt.Errorf("Range check failed on b.QtyTotal (%v < %v > %v)", b.QtyTotalMinValue(), b.QtyTotal, b.QtyTotalMaxValue())
		}
	}
	if b.OrderCountInActingVersion(actingVersion) {
		if b.OrderCount < b.OrderCountMinValue() || b.OrderCount > b.OrderCountMaxValue() {
			return fmt.Errorf("Range check failed on b.OrderCount (%v < %v > %v)", b.OrderCountMinValue(), b.OrderCount, b.OrderCountMaxValue())
		}
	}
	if b.HeadInActingVersion(actingVersion) {
		if b.Head < b.HeadMinValue() || b.Head > b.HeadMaxValue() {
			return fmt.Errorf("Range check failed on b.Head (%v < %v > %v)", b.HeadMinValue(), b.Head, b.HeadMaxValue())
		}
	}
	if b.TailInActingVersion(actingVersion) {
		if b.Tail < b.TailMinValue() || b.Tail > b.TailMaxValue() {
			return fmt.Errorf("Range check failed on b.Tail (%v < %v > %v)", b.TailMinValue(), b.Tail, b.TailMaxValue())
		}
	}
	return nil
}

func BookSnapshotLevelsInit(b *BookSnapshotLevels) {
	return
}

func (b *BookSnapshotOrders) Encode(_m *SbeGoMarshaller, _w io.Writer) error {
	if err := _m.WriteUint32(_w, b.Slot); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.OrderId); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.Price); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.Qty); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, b.Filled); err != nil {
		return err
	}
	if err := b.Side.Encode(_m, _w); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.NextSlot); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, b.Prev); err != nil {
		return err
	}
	return nil
}

func (b *BookSnapshotOrders) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16, blockLength uint) error {
	if !b.SlotInActingVersion(actingVersion) {
		b.Slot = b.SlotNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Slot); err != nil {
			return err
		}
	}
	if !b.OrderIdInActingVersion(actingVersion) {
		b.OrderId = b.OrderIdNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.OrderId); err != nil {
			return err
		}
	}
	if !b.PriceInActingVersion(actingVersion) {
		b.Price = b.PriceNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.Price); err != nil {
			return err
		}
	}
	if !b.QtyInActingVersion(actingVersion) {
		b.Qty = b.QtyNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.Qty); err != nil {
			return err
		}
	}
	if !b.FilledInActingVersion(actingVersion) {
		b.Filled = b.FilledNullValue()
	} else {
		if err := _m.ReadInt64(_r, &b.Filled); err != nil {
			return err
		}
	}
	if b.SideInActingVersion(actingVersion) {
		if err := b.Side.Decode(_m, _r, actingVersion); err != nil {
			return err
		}
	}
	if !b.NextSlotInActingVersion(actingVersion) {
		b.NextSlot = b.NextSlotNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.NextSlot); err != nil {
			return err
		}
	}
	if !b.PrevInActingVersion(actingVersion) {
		b.Prev = b.PrevNullValue()
	} else {
		if err := _m.ReadUint32(_r, &b.Prev); err != nil {
			return err
		}
	}
	if actingVersion > b.SbeSchemaVersion() && blockLength > b.SbeBlockLength() {
		io.CopyN(ioutil.Discard, _r, int64(blockLength-b.SbeBlockLength()))
	}
	return nil
}

func (b *BookSnapshotOrders) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if b.SlotInActingVersion(actingVersion) {
		if b.Slot < b.SlotMinValue() || b.Slot > b.SlotMaxValue() {
			return fmt.Errorf("Range check failed on b.Slot (%v < %v > %v)", b.SlotMinValue(), b.Slot, b.SlotMaxValue())
		}
	}
	if b.OrderIdInActingVersion(actingVersion) {
		if b.OrderId < b.OrderIdMinValue() || b.OrderId > b.OrderIdMaxValue() {
			return fmt.Errorf("Range check failed on b.OrderId (%v < %v > %v)", b.OrderIdMinValue(), b.OrderId, b.OrderIdMaxValue())
		}
	}
	if b.PriceInActingVersion(actingVersion) {
		if b.Price < b.PriceMinValue() || b.Price > b.PriceMaxValue() {
			return fmt.Errorf("Range check failed on b.Price (%v < %v > %v)", b.PriceMinValue(), b.Price, b.PriceMaxValue())
		}
	}
	if b.QtyInActingVersion(actingVersion) {
		if b.Qty < b.QtyMinValue() || b.Qty > b.QtyMaxValue() {
			return fmt.Errorf("Range check failed on b.Qty (%v < %v > %v)", b.QtyMinValue(), b.Qty, b.QtyMaxValue())
		}
	}
	if b.FilledInActingVersion(actingVersion) {
		if b.Filled < b.FilledMinValue() || b.Filled > b.FilledMaxValue() {
			return fmt.Errorf("Range check failed on b.Filled (%v < %v > %v)", b.FilledMinValue(), b.Filled, b.FilledMaxValue())
		}
	}
	if err := b.Side.RangeCheck(actingVersion, schemaVersion); err != nil {
		return err
	}
	if b.NextSlotInActingVersion(actingVersion) {
		if b.NextSlot < b.NextSlotMinValue() || b.NextSlot > b.NextSlotMaxValue() {
			return fmt.Errorf("Range check failed on b.NextSlot (%v < %v > %v)", b.NextSlotMinValue(), b.NextSlot, b.NextSlotMaxValue())
		}
	}
	if b.PrevInActingVersion(actingVersion) {
		if b.Prev < b.PrevMinValue() || b.Prev > b.PrevMaxValue() {
			return fmt.Errorf("Range check failed on b.Prev (%v < %v > %v)", b.PrevMinValue(), b.Prev, b.PrevMaxValue())
		}
	}
	return nil
}

func BookSnapshotOrdersInit(b *BookSnapshotOrders) {
	return
}

func (*BookSnapshot) SbeBlockLength() (blockLength uint16) {
	return 36
}

func (*BookSnapshot) SbeTemplateId() (templateId uint16) {
	return 1
}

func (*BookSnapshot) SbeSchemaId() (schemaId uint16) {
	return 8
}

func (*BookSnapshot) SbeSchemaVersion() (schemaVersion uint16) {
	return 1
}

func (*BookSnapshot) SbeSemanticType() (semanticType []byte) {
	return []byte("")
}

func (*BookSnapshot) SbeSemanticVersion() (semanticVersion string) {
	return ""
}

func (*BookSnapshot) PriceMinId() uint16 {
	return 1
}

func (*BookSnapshot) PriceMinSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) PriceMinInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.PriceMinSinceVersion()
}

func (*BookSnapshot) PriceMinDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) PriceMinMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) PriceMinMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshot) PriceMinMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshot) PriceMinNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshot) TickSizeId() uint16 {
	return 2
}

func (*BookSnapshot) TickSizeSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) TickSizeInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.TickSizeSinceVersion()
}

func (*BookSnapshot) TickSizeDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) TickSizeMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) TickSizeMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshot) TickSizeMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshot) TickSizeNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshot) NLevelsId() uint16 {
	return 3
}

func (*BookSnapshot) NLevelsSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) NLevelsInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.NLevelsSinceVersion()
}

func (*BookSnapshot) NLevelsDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) NLevelsMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) NLevelsMinValue() uint32 {
	return 0
}

func (*BookSnapshot) NLevelsMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshot) NLevelsNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshot) CapacityId() uint16 {
	return 4
}

func (*BookSnapshot) CapacitySinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) CapacityInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.CapacitySinceVersion()
}

func (*BookSnapshot) CapacityDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) CapacityMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) CapacityMinValue() uint32 {
	return 0
}

func (*BookSnapshot) CapacityMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshot) CapacityNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshot) HwmId() uint16 {
	return 5
}

func (*BookSnapshot) HwmSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) HwmInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.HwmSinceVersion()
}

func (*BookSnapshot) HwmDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) HwmMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) HwmMinValue() uint32 {
	return 0
}

func (*BookSnapshot) HwmMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshot) HwmNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshot) BestBidId() uint16 {
	return 6
}

func (*BookSnapshot) BestBidSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) BestBidInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.BestBidSinceVersion()
}

func (*BookSnapshot) BestBidDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) BestBidMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) BestBidMinValue() int32 {
	return math.MinInt32 + 1
}

func (*BookSnapshot) BestBidMaxValue() int32 {
	return math.MaxInt32
}

func (*BookSnapshot) BestBidNullValue() int32 {
	return math.MinInt32
}

func (*BookSnapshot) BestAskId() uint16 {
	return 7
}

func (*BookSnapshot) BestAskSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) BestAskInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.BestAskSinceVersion()
}

func (*BookSnapshot) BestAskDeprecated() uint16 {
	return 0
}

func (*BookSnapshot) BestAskMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshot) BestAskMinValue() int32 {
	return math.MinInt32 + 1
}

func (*BookSnapshot) BestAskMaxValue() int32 {
	return math.MaxInt32
}

func (*BookSnapshot) BestAskNullValue() int32 {
	return math.MinInt32
}

func (*BookSnapshotLevels) SideId() uint16 {
	return 11
}

func (*BookSnapshotLevels) SideSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) SideInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.SideSinceVersion()
}

func (*BookSnapshotLevels) SideDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) SideMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) LevelTickId() uint16 {
	return 12
}

func (*BookSnapshotLevels) LevelTickSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) LevelTickInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.LevelTickSinceVersion()
}

func (*BookSnapshotLevels) LevelTickDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) LevelTickMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) LevelTickMinValue() uint32 {
	return 0
}

func (*BookSnapshotLevels) LevelTickMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotLevels) LevelTickNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotLevels) QtyTotalId() uint16 {
	return 13
}

func (*BookSnapshotLevels) QtyTotalSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) QtyTotalInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.QtyTotalSinceVersion()
}

func (*BookSnapshotLevels) QtyTotalDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) QtyTotalMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) QtyTotalMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshotLevels) QtyTotalMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshotLevels) QtyTotalNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshotLevels) OrderCountId() uint16 {
	return 14
}

func (*BookSnapshotLevels) OrderCountSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) OrderCountInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.OrderCountSinceVersion()
}

func (*BookSnapshotLevels) OrderCountDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) OrderCountMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) OrderCountMinValue() uint32 {
	return 0
}

func (*BookSnapshotLevels) OrderCountMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotLevels) OrderCountNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotLevels) HeadId() uint16 {
	return 15
}

func (*BookSnapshotLevels) HeadSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) HeadInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.HeadSinceVersion()
}

func (*BookSnapshotLevels) HeadDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) HeadMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) HeadMinValue() uint32 {
	return 0
}

func (*BookSnapshotLevels) HeadMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotLevels) HeadNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotLevels) TailId() uint16 {
	return 16
}

func (*BookSnapshotLevels) TailSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotLevels) TailInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.TailSinceVersion()
}

func (*BookSnapshotLevels) TailDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) TailMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotLevels) TailMinValue() uint32 {
	return 0
}

func (*BookSnapshotLevels) TailMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotLevels) TailNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotOrders) SlotId() uint16 {
	return 21
}

func (*BookSnapshotOrders) SlotSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) SlotInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.SlotSinceVersion()
}

func (*BookSnapshotOrders) SlotDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) SlotMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) SlotMinValue() uint32 {
	return 0
}

func (*BookSnapshotOrders) SlotMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotOrders) SlotNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotOrders) OrderIdId() uint16 {
	return 22
}

func (*BookSnapshotOrders) OrderIdSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) OrderIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.OrderIdSinceVersion()
}

func (*BookSnapshotOrders) OrderIdDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) OrderIdMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) OrderIdMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshotOrders) OrderIdMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshotOrders) OrderIdNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshotOrders) PriceId() uint16 {
	return 23
}

func (*BookSnapshotOrders) PriceSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) PriceInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.PriceSinceVersion()
}

func (*BookSnapshotOrders) PriceDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) PriceMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) PriceMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshotOrders) PriceMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshotOrders) PriceNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshotOrders) QtyId() uint16 {
	return 24
}

func (*BookSnapshotOrders) QtySinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) QtyInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.QtySinceVersion()
}

func (*BookSnapshotOrders) QtyDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) QtyMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) QtyMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshotOrders) QtyMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshotOrders) QtyNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshotOrders) FilledId() uint16 {
	return 25
}

func (*BookSnapshotOrders) FilledSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) FilledInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.FilledSinceVersion()
}

func (*BookSnapshotOrders) FilledDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) FilledMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) FilledMinValue() int64 {
	return math.MinInt64 + 1
}

func (*BookSnapshotOrders) FilledMaxValue() int64 {
	return math.MaxInt64
}

func (*BookSnapshotOrders) FilledNullValue() int64 {
	return math.MinInt64
}

func (*BookSnapshotOrders) SideId() uint16 {
	return 26
}

func (*BookSnapshotOrders) SideSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) SideInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.SideSinceVersion()
}

func (*BookSnapshotOrders) SideDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) SideMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) NextSlotId() uint16 {
	return 27
}

func (*BookSnapshotOrders) NextSlotSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) NextSlotInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.NextSlotSinceVersion()
}

func (*BookSnapshotOrders) NextSlotDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) NextSlotMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) NextSlotMinValue() uint32 {
	return 0
}

func (*BookSnapshotOrders) NextSlotMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotOrders) NextSlotNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshotOrders) PrevId() uint16 {
	return 28
}

func (*BookSnapshotOrders) PrevSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshotOrders) PrevInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.PrevSinceVersion()
}

func (*BookSnapshotOrders) PrevDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) PrevMetaAttribute(meta int) string {
	switch meta {
	case 1:
		return ""
	case 2:
		return ""
	case 3:
		return ""
	case 4:
		return "required"
	}
	return ""
}

func (*BookSnapshotOrders) PrevMinValue() uint32 {
	return 0
}

func (*BookSnapshotOrders) PrevMaxValue() uint32 {
	return math.MaxUint32 - 1
}

func (*BookSnapshotOrders) PrevNullValue() uint32 {
	return math.MaxUint32
}

func (*BookSnapshot) LevelsId() uint16 {
	return 10
}

func (*BookSnapshot) LevelsSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) LevelsInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.LevelsSinceVersion()
}

func (*BookSnapshot) LevelsDeprecated() uint16 {
	return 0
}

func (*BookSnapshotLevels) SbeBlockLength() (blockLength uint) {
	return 25
}

func (*BookSnapshotLevels) SbeSchemaVersion() (schemaVersion uint16) {
	return 1
}

func (*BookSnapshot) OrdersId() uint16 {
	return 20
}

func (*BookSnapshot) OrdersSinceVersion() uint16 {
	return 0
}

func (b *BookSnapshot) OrdersInActingVersion(actingVersion uint16) bool {
	return actingVersion >= b.OrdersSinceVersion()
}

func (*BookSnapshot) OrdersDeprecated() uint16 {
	return 0
}

func (*BookSnapshotOrders) SbeBlockLength() (blockLength uint) {
	return 45
}

func (*BookSnapshotOrders) SbeSchemaVersion() (schemaVersion uint16) {
	return 1
}
