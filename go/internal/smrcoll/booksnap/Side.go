// Generated SBE (Simple Binary Encoding) message codec

package booksnap

import (
	"fmt"
	"io"
	"reflect"
)

type SideEnum uint8
type SideValues struct {
	BID       SideEnum
	ASK       SideEnum
	NullValue SideEnum
}

var Side = SideValues{0, 1, 255}

func (s SideEnum) Encode(_m *SbeGoMarshaller, _w io.Writer) error {
	if err := _m.WriteUint8(_w, uint8(s)); err != nil {
		return err
	}
	return nil
}

func (s *SideEnum) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16) error {
	if err := _m.ReadUint8(_r, (*uint8)(s)); err != nil {
		return err
	}
	return nil
}

func (s SideEnum) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if actingVersion > schemaVersion {
		return nil
	}
	value := reflect.ValueOf(Side)
	for idx := 0; idx < value.NumField(); idx++ {
		if s == value.Field(idx).Interface() {
			return nil
		}
	}
	return fmt.Errorf("Range check failed on Side, unknown enumeration value %d", s)
}

func (*SideEnum) EncodedLength() int64 {
	return 1
}

func (*SideEnum) BIDSinceVersion() uint16 {
	return 0
}

func (s *SideEnum) BIDInActingVersion(actingVersion uint16) bool {
	return actingVersion >= s.BIDSinceVersion()
}

func (*SideEnum) BIDDeprecated() uint16 {
	return 0
}

func (*SideEnum) ASKSinceVersion() uint16 {
	return 0
}

func (s *SideEnum) ASKInActingVersion(actingVersion uint16) bool {
	return actingVersion >= s.ASKSinceVersion()
}

func (*SideEnum) ASKDeprecated() uint16 {
	return 0
}
