// Generated SBE (Simple Binary Encoding) message codec

package journalsbestruct

import (
	"fmt"
	"io"
	"io/ioutil"
	"math"
)

type JournalRecord struct {
	LeadershipTermId int64
	LogPosition      int64
	Timestamp        int64
	ClusterSessionId int64
	CorrelationId    int64
	LeaderMemberId   int32
	ServiceId        int32
	EventType        EventTypeEnum
	Flags            uint8
	Entries          []JournalRecordEntries
}
type JournalRecordEntries struct {
	EntryTermId    int64
	EntryIndex     int64
	EntryTimestamp int64
	CommandKey     int32
	Command        []uint8
}

func (j *JournalRecord) Encode(_m *SbeGoMarshaller, _w io.Writer, doRangeCheck bool) error {
	if doRangeCheck {
		if err := j.RangeCheck(j.SbeSchemaVersion(), j.SbeSchemaVersion()); err != nil {
			return err
		}
	}
	if err := _m.WriteInt64(_w, j.LeadershipTermId); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.LogPosition); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.Timestamp); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.ClusterSessionId); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.CorrelationId); err != nil {
		return err
	}
	if err := _m.WriteInt32(_w, j.LeaderMemberId); err != nil {
		return err
	}
	if err := _m.WriteInt32(_w, j.ServiceId); err != nil {
		return err
	}
	if err := j.EventType.Encode(_m, _w); err != nil {
		return err
	}
	if err := _m.WriteUint8(_w, j.Flags); err != nil {
		return err
	}
	var EntriesBlockLength uint16 = 28
	if err := _m.WriteUint16(_w, EntriesBlockLength); err != nil {
		return err
	}
	var EntriesNumInGroup uint16 = uint16(len(j.Entries))
	if err := _m.WriteUint16(_w, EntriesNumInGroup); err != nil {
		return err
	}
	for i := range j.Entries {
		if err := j.Entries[i].Encode(_m, _w); err != nil {
			return err
		}
	}
	return nil
}

func (j *JournalRecord) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16, blockLength uint16, doRangeCheck bool) error {
	if !j.LeadershipTermIdInActingVersion(actingVersion) {
		j.LeadershipTermId = j.LeadershipTermIdNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.LeadershipTermId); err != nil {
			return err
		}
	}
	if !j.LogPositionInActingVersion(actingVersion) {
		j.LogPosition = j.LogPositionNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.LogPosition); err != nil {
			return err
		}
	}
	if !j.TimestampInActingVersion(actingVersion) {
		j.Timestamp = j.TimestampNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.Timestamp); err != nil {
			return err
		}
	}
	if !j.ClusterSessionIdInActingVersion(actingVersion) {
		j.ClusterSessionId = j.ClusterSessionIdNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.ClusterSessionId); err != nil {
			return err
		}
	}
	if !j.CorrelationIdInActingVersion(actingVersion) {
		j.CorrelationId = j.CorrelationIdNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.CorrelationId); err != nil {
			return err
		}
	}
	if !j.LeaderMemberIdInActingVersion(actingVersion) {
		j.LeaderMemberId = j.LeaderMemberIdNullValue()
	} else {
		if err := _m.ReadInt32(_r, &j.LeaderMemberId); err != nil {
			return err
		}
	}
	if !j.ServiceIdInActingVersion(actingVersion) {
		j.ServiceId = j.ServiceIdNullValue()
	} else {
		if err := _m.ReadInt32(_r, &j.ServiceId); err != nil {
			return err
		}
	}
	if j.EventTypeInActingVersion(actingVersion) {
		if err := j.EventType.Decode(_m, _r, actingVersion); err != nil {
			return err
		}
	}
	if !j.FlagsInActingVersion(actingVersion) {
		j.Flags = j.FlagsNullValue()
	} else {
		if err := _m.ReadUint8(_r, &j.Flags); err != nil {
			return err
		}
	}
	if actingVersion > j.SbeSchemaVersion() && blockLength > j.SbeBlockLength() {
		io.CopyN(ioutil.Discard, _r, int64(blockLength-j.SbeBlockLength()))
	}

	if j.EntriesInActingVersion(actingVersion) {
		var EntriesBlockLength uint16
		if err := _m.ReadUint16(_r, &EntriesBlockLength); err != nil {
			return err
		}
		var EntriesNumInGroup uint16
		if err := _m.ReadUint16(_r, &EntriesNumInGroup); err != nil {
			return err
		}
		if cap(j.Entries) < int(EntriesNumInGroup) {
			j.Entries = make([]JournalRecordEntries, EntriesNumInGroup)
		}
		j.Entries = j.Entries[:EntriesNumInGroup]
		for i := range j.Entries {
			if err := j.Entries[i].Decode(_m, _r, actingVersion, uint(EntriesBlockLength)); err != nil {
				return err
			}
		}
	}
	if doRangeCheck {
		if err := j.RangeCheck(actingVersion, j.SbeSchemaVersion()); err != nil {
			return err
		}
	}
	return nil
}

func (j *JournalRecord) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if j.LeadershipTermIdInActingVersion(actingVersion) {
		if j.LeadershipTermId < j.LeadershipTermIdMinValue() || j.LeadershipTermId > j.LeadershipTermIdMaxValue() {
			return fmt.Errorf("Range check failed on j.LeadershipTermId (%v < %v > %v)", j.LeadershipTermIdMinValue(), j.LeadershipTermId, j.LeadershipTermIdMaxValue())
		}
	}
	if j.LogPositionInActingVersion(actingVersion) {
		if j.LogPosition < j.LogPositionMinValue() || j.LogPosition > j.LogPositionMaxValue() {
			return fmt.Errorf("Range check failed on j.LogPosition (%v < %v > %v)", j.LogPositionMinValue(), j.LogPosition, j.LogPositionMaxValue())
		}
	}
	if j.TimestampInActingVersion(actingVersion) {
		if j.Timestamp < j.TimestampMinValue() || j.Timestamp > j.TimestampMaxValue() {
			return fmt.Errorf("Range check failed on j.Timestamp (%v < %v > %v)", j.TimestampMinValue(), j.Timestamp, j.TimestampMaxValue())
		}
	}
	if j.ClusterSessionIdInActingVersion(actingVersion) {
		if j.ClusterSessionId < j.ClusterSessionIdMinValue() || j.ClusterSessionId > j.ClusterSessionIdMaxValue() {
			return fmt.Errorf("Range check failed on j.ClusterSessionId (%v < %v > %v)", j.ClusterSessionIdMinValue(), j.ClusterSessionId, j.ClusterSessionIdMaxValue())
		}
	}
	if j.CorrelationIdInActingVersion(actingVersion) {
		if j.CorrelationId < j.CorrelationIdMinValue() || j.CorrelationId > j.CorrelationIdMaxValue() {
			return fmt.Errorf("Range check failed on j.CorrelationId (%v < %v > %v)", j.CorrelationIdMinValue(), j.CorrelationId, j.CorrelationIdMaxValue())
		}
	}
	if j.LeaderMemberIdInActingVersion(actingVersion) {
		if j.LeaderMemberId < j.LeaderMemberIdMinValue() || j.LeaderMemberId > j.LeaderMemberIdMaxValue() {
			return fmt.Errorf("Range check failed on j.LeaderMemberId (%v < %v > %v)", j.LeaderMemberIdMinValue(), j.LeaderMemberId, j.LeaderMemberIdMaxValue())
		}
	}
	if j.ServiceIdInActingVersion(actingVersion) {
		if j.ServiceId < j.ServiceIdMinValue() || j.ServiceId > j.ServiceIdMaxValue() {
			return fmt.Errorf("Range check failed on j.ServiceId (%v < %v > %v)", j.ServiceIdMinValue(), j.ServiceId, j.ServiceIdMaxValue())
		}
	}
	if err := j.EventType.RangeCheck(actingVersion, schemaVersion); err != nil {
		return err
	}
	if j.FlagsInActingVersion(actingVersion) {
		if j.Flags < j.FlagsMinValue() || j.Flags > j.FlagsMaxValue() {
			return fmt.Errorf("Range check failed on j.Flags (%v < %v > %v)", j.FlagsMinValue(), j.Flags, j.FlagsMaxValue())
		}
	}
	for i := range j.Entries {
		if err := j.Entries[i].RangeCheck(actingVersion, schemaVersion); err != nil {
			return err
		}
	}
	return nil
}

func JournalRecordInit(j *JournalRecord) {
	return
}

func (j *JournalRecordEntries) Encode(_m *SbeGoMarshaller, _w io.Writer) error {
	if err := _m.WriteInt64(_w, j.EntryTermId); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.EntryIndex); err != nil {
		return err
	}
	if err := _m.WriteInt64(_w, j.EntryTimestamp); err != nil {
		return err
	}
	if err := _m.WriteInt32(_w, j.CommandKey); err != nil {
		return err
	}
	if err := _m.WriteUint32(_w, uint32(len(j.Command))); err != nil {
		return err
	}
	if err := _m.WriteBytes(_w, j.Command); err != nil {
		return err
	}
	return nil
}

func (j *JournalRecordEntries) Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion uint16, blockLength uint) error {
	if !j.EntryTermIdInActingVersion(actingVersion) {
		j.EntryTermId = j.EntryTermIdNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.EntryTermId); err != nil {
			return err
		}
	}
	if !j.EntryIndexInActingVersion(actingVersion) {
		j.EntryIndex = j.EntryIndexNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.EntryIndex); err != nil {
			return err
		}
	}
	if !j.EntryTimestampInActingVersion(actingVersion) {
		j.EntryTimestamp = j.EntryTimestampNullValue()
	} else {
		if err := _m.ReadInt64(_r, &j.EntryTimestamp); err != nil {
			return err
		}
	}
	if !j.CommandKeyInActingVersion(actingVersion) {
		j.CommandKey = j.CommandKeyNullValue()
	} else {
		if err := _m.ReadInt32(_r, &j.CommandKey); err != nil {
			return err
		}
	}
	if actingVersion > j.SbeSchemaVersion() && blockLength > j.SbeBlockLength() {
		io.CopyN(ioutil.Discard, _r, int64(blockLength-j.SbeBlockLength()))
	}

	if j.CommandInActingVersion(actingVersion) {
		var CommandLength uint32
		if err := _m.ReadUint32(_r, &CommandLength); err != nil {
			return err
		}
		if cap(j.Command) < int(CommandLength) {
			j.Command = make([]uint8, CommandLength)
		}
		j.Command = j.Command[:CommandLength]
		if err := _m.ReadBytes(_r, j.Command); err != nil {
			return err
		}
	}
	return nil
}

func (j *JournalRecordEntries) RangeCheck(actingVersion uint16, schemaVersion uint16) error {
	if j.EntryTermIdInActingVersion(actingVersion) {
		if j.EntryTermId < j.EntryTermIdMinValue() || j.EntryTermId > j.EntryTermIdMaxValue() {
			return fmt.Errorf("Range check failed on j.EntryTermId (%v < %v > %v)", j.EntryTermIdMinValue(), j.EntryTermId, j.EntryTermIdMaxValue())
		}
	}
	if j.EntryIndexInActingVersion(actingVersion) {
		if j.EntryIndex < j.EntryIndexMinValue() || j.EntryIndex > j.EntryIndexMaxValue() {
			return fmt.Errorf("Range check failed on j.EntryIndex (%v < %v > %v)", j.EntryIndexMinValue(), j.EntryIndex, j.EntryIndexMaxValue())
		}
	}
	if j.EntryTimestampInActingVersion(actingVersion) {
		if j.EntryTimestamp < j.EntryTimestampMinValue() || j.EntryTimestamp > j.EntryTimestampMaxValue() {
			return fmt.Errorf("Range check failed on j.EntryTimestamp (%v < %v > %v)", j.EntryTimestampMinValue(), j.EntryTimestamp, j.EntryTimestampMaxValue())
		}
	}
	if j.CommandKeyInActingVersion(actingVersion) {
		if j.CommandKey < j.CommandKeyMinValue() || j.CommandKey > j.CommandKeyMaxValue() {
			return fmt.Errorf("Range check failed on j.CommandKey (%v < %v > %v)", j.CommandKeyMinValue(), j.CommandKey, j.CommandKeyMaxValue())
		}
	}
	return nil
}

func JournalRecordEntriesInit(j *JournalRecordEntries) {
	return
}

func (*JournalRecord) SbeBlockLength() (blockLength uint16) {
	return 50
}

func (*JournalRecord) SbeTemplateId() (templateId uint16) {
	return 1
}

func (*JournalRecord) SbeSchemaId() (schemaId uint16) {
	return 7
}

func (*JournalRecord) SbeSchemaVersion() (schemaVersion uint16) {
	return 1
}

func (*JournalRecord) SbeSemanticType() (semanticType []byte) {
	return []byte("")
}

func (*JournalRecord) SbeSemanticVersion() (semanticVersion string) {
	return ""
}

func (*JournalRecord) LeadershipTermIdId() uint16 {
	return 1
}

func (*JournalRecord) LeadershipTermIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) LeadershipTermIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.LeadershipTermIdSinceVersion()
}

func (*JournalRecord) LeadershipTermIdDeprecated() uint16 {
	return 0
}

func (*JournalRecord) LeadershipTermIdMetaAttribute(meta int) string {
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

func (*JournalRecord) LeadershipTermIdMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecord) LeadershipTermIdMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecord) LeadershipTermIdNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecord) LogPositionId() uint16 {
	return 2
}

func (*JournalRecord) LogPositionSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) LogPositionInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.LogPositionSinceVersion()
}

func (*JournalRecord) LogPositionDeprecated() uint16 {
	return 0
}

func (*JournalRecord) LogPositionMetaAttribute(meta int) string {
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

func (*JournalRecord) LogPositionMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecord) LogPositionMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecord) LogPositionNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecord) TimestampId() uint16 {
	return 3
}

func (*JournalRecord) TimestampSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) TimestampInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.TimestampSinceVersion()
}

func (*JournalRecord) TimestampDeprecated() uint16 {
	return 0
}

func (*JournalRecord) TimestampMetaAttribute(meta int) string {
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

func (*JournalRecord) TimestampMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecord) TimestampMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecord) TimestampNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecord) ClusterSessionIdId() uint16 {
	return 4
}

func (*JournalRecord) ClusterSessionIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) ClusterSessionIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.ClusterSessionIdSinceVersion()
}

func (*JournalRecord) ClusterSessionIdDeprecated() uint16 {
	return 0
}

func (*JournalRecord) ClusterSessionIdMetaAttribute(meta int) string {
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

func (*JournalRecord) ClusterSessionIdMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecord) ClusterSessionIdMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecord) ClusterSessionIdNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecord) CorrelationIdId() uint16 {
	return 5
}

func (*JournalRecord) CorrelationIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) CorrelationIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.CorrelationIdSinceVersion()
}

func (*JournalRecord) CorrelationIdDeprecated() uint16 {
	return 0
}

func (*JournalRecord) CorrelationIdMetaAttribute(meta int) string {
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

func (*JournalRecord) CorrelationIdMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecord) CorrelationIdMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecord) CorrelationIdNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecord) LeaderMemberIdId() uint16 {
	return 6
}

func (*JournalRecord) LeaderMemberIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) LeaderMemberIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.LeaderMemberIdSinceVersion()
}

func (*JournalRecord) LeaderMemberIdDeprecated() uint16 {
	return 0
}

func (*JournalRecord) LeaderMemberIdMetaAttribute(meta int) string {
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

func (*JournalRecord) LeaderMemberIdMinValue() int32 {
	return math.MinInt32 + 1
}

func (*JournalRecord) LeaderMemberIdMaxValue() int32 {
	return math.MaxInt32
}

func (*JournalRecord) LeaderMemberIdNullValue() int32 {
	return math.MinInt32
}

func (*JournalRecord) ServiceIdId() uint16 {
	return 7
}

func (*JournalRecord) ServiceIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) ServiceIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.ServiceIdSinceVersion()
}

func (*JournalRecord) ServiceIdDeprecated() uint16 {
	return 0
}

func (*JournalRecord) ServiceIdMetaAttribute(meta int) string {
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

func (*JournalRecord) ServiceIdMinValue() int32 {
	return math.MinInt32 + 1
}

func (*JournalRecord) ServiceIdMaxValue() int32 {
	return math.MaxInt32
}

func (*JournalRecord) ServiceIdNullValue() int32 {
	return math.MinInt32
}

func (*JournalRecord) EventTypeId() uint16 {
	return 8
}

func (*JournalRecord) EventTypeSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) EventTypeInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.EventTypeSinceVersion()
}

func (*JournalRecord) EventTypeDeprecated() uint16 {
	return 0
}

func (*JournalRecord) EventTypeMetaAttribute(meta int) string {
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

func (*JournalRecord) FlagsId() uint16 {
	return 9
}

func (*JournalRecord) FlagsSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) FlagsInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.FlagsSinceVersion()
}

func (*JournalRecord) FlagsDeprecated() uint16 {
	return 0
}

func (*JournalRecord) FlagsMetaAttribute(meta int) string {
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

func (*JournalRecord) FlagsMinValue() uint8 {
	return 0
}

func (*JournalRecord) FlagsMaxValue() uint8 {
	return math.MaxUint8 - 1
}

func (*JournalRecord) FlagsNullValue() uint8 {
	return math.MaxUint8
}

func (*JournalRecordEntries) EntryTermIdId() uint16 {
	return 11
}

func (*JournalRecordEntries) EntryTermIdSinceVersion() uint16 {
	return 0
}

func (j *JournalRecordEntries) EntryTermIdInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.EntryTermIdSinceVersion()
}

func (*JournalRecordEntries) EntryTermIdDeprecated() uint16 {
	return 0
}

func (*JournalRecordEntries) EntryTermIdMetaAttribute(meta int) string {
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

func (*JournalRecordEntries) EntryTermIdMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecordEntries) EntryTermIdMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecordEntries) EntryTermIdNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecordEntries) EntryIndexId() uint16 {
	return 12
}

func (*JournalRecordEntries) EntryIndexSinceVersion() uint16 {
	return 0
}

func (j *JournalRecordEntries) EntryIndexInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.EntryIndexSinceVersion()
}

func (*JournalRecordEntries) EntryIndexDeprecated() uint16 {
	return 0
}

func (*JournalRecordEntries) EntryIndexMetaAttribute(meta int) string {
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

func (*JournalRecordEntries) EntryIndexMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecordEntries) EntryIndexMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecordEntries) EntryIndexNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecordEntries) EntryTimestampId() uint16 {
	return 13
}

func (*JournalRecordEntries) EntryTimestampSinceVersion() uint16 {
	return 0
}

func (j *JournalRecordEntries) EntryTimestampInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.EntryTimestampSinceVersion()
}

func (*JournalRecordEntries) EntryTimestampDeprecated() uint16 {
	return 0
}

func (*JournalRecordEntries) EntryTimestampMetaAttribute(meta int) string {
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

func (*JournalRecordEntries) EntryTimestampMinValue() int64 {
	return math.MinInt64 + 1
}

func (*JournalRecordEntries) EntryTimestampMaxValue() int64 {
	return math.MaxInt64
}

func (*JournalRecordEntries) EntryTimestampNullValue() int64 {
	return math.MinInt64
}

func (*JournalRecordEntries) CommandKeyId() uint16 {
	return 14
}

func (*JournalRecordEntries) CommandKeySinceVersion() uint16 {
	return 0
}

func (j *JournalRecordEntries) CommandKeyInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.CommandKeySinceVersion()
}

func (*JournalRecordEntries) CommandKeyDeprecated() uint16 {
	return 0
}

func (*JournalRecordEntries) CommandKeyMetaAttribute(meta int) string {
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

func (*JournalRecordEntries) CommandKeyMinValue() int32 {
	return math.MinInt32 + 1
}

func (*JournalRecordEntries) CommandKeyMaxValue() int32 {
	return math.MaxInt32
}

func (*JournalRecordEntries) CommandKeyNullValue() int32 {
	return math.MinInt32
}

func (*JournalRecordEntries) CommandMetaAttribute(meta int) string {
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

func (*JournalRecordEntries) CommandSinceVersion() uint16 {
	return 0
}

func (j *JournalRecordEntries) CommandInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.CommandSinceVersion()
}

func (*JournalRecordEntries) CommandDeprecated() uint16 {
	return 0
}

func (JournalRecordEntries) CommandCharacterEncoding() string {
	return "null"
}

func (JournalRecordEntries) CommandHeaderLength() uint64 {
	return 4
}

func (*JournalRecord) EntriesId() uint16 {
	return 10
}

func (*JournalRecord) EntriesSinceVersion() uint16 {
	return 0
}

func (j *JournalRecord) EntriesInActingVersion(actingVersion uint16) bool {
	return actingVersion >= j.EntriesSinceVersion()
}

func (*JournalRecord) EntriesDeprecated() uint16 {
	return 0
}

func (*JournalRecordEntries) SbeBlockLength() (blockLength uint) {
	return 28
}

func (*JournalRecordEntries) SbeSchemaVersion() (schemaVersion uint16) {
	return 1
}
