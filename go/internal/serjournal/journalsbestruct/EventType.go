// Generated SBE (Simple Binary Encoding) message codec

package journalsbestruct

import (
	"fmt"
	"io"
	"reflect"
)

type EventTypeEnum uint8
type EventTypeValues struct {
	APPEND    EventTypeEnum
	SNAPSHOT  EventTypeEnum
	NullValue EventTypeEnum
}

var EventType = EventTypeValues{0, 1, 255}

func (e EventTypeEnum) Encode(_m *SbeGoMarshaller, _w io.Writer) error {
	if err := _m.WriteUint8(_w, uint8(e)); err != nil {
		return err
	}
	return nil
}

func (e *EventTypeEnum) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16) error {
	if err := _m.ReadUint8(_r, (*uint8)(e)); err != nil {
		return err
	}
	return nil
}

func (e EventTypeEnum) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if actingVersion > schemaVersion {
		return nil
	}
	value := reflect.ValueOf(EventType)
	for idx := 0; idx < value.NumField(); idx++ {
		if e == value.Field(idx).Interface() {
			return nil
		}
	}
	return fmt.Errorf("Range check failed on EventType, unknown enumeration value %d", e)
}

func (*EventTypeEnum) EncodedLength() int64 {
	return 1
}

func (*EventTypeEnum) APPENDSinceVersion() uint16 {
	return 0
}

func (e *EventTypeEnum) APPENDInActingVersion(actingVersion uint16) bool {
	return actingVersion >= e.APPENDSinceVersion()
}

func (*EventTypeEnum) APPENDDeprecated() uint16 {
	return 0
}

func (*EventTypeEnum) SNAPSHOTSinceVersion() uint16 {
	return 0
}

func (e *EventTypeEnum) SNAPSHOTInActingVersion(actingVersion uint16) bool {
	return actingVersion >= e.SNAPSHOTSinceVersion()
}

func (*EventTypeEnum) SNAPSHOTDeprecated() uint16 {
	return 0
}
