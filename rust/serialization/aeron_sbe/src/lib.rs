//! aeron_sbe (real-logic SBE tool → Rust) codec cell.

use journal::journal_record_codec::encoder::EntriesEncoder;
use journal::journal_record_codec::{JournalRecordDecoder, JournalRecordEncoder};
use journal::message_header_codec::MessageHeaderDecoder;
use journal::{Encoder, ReadBuf, WriteBuf};
use serialization_common::{Checksum, JournalRecord};

const HEADER_LEN: usize = 8;

/// Encode header + body into `buf`, returning total framed length.
///
/// `JournalRecordEncoder::header` writes the message header (block length,
/// template id, schema id, version) at the given offset using the *same*
/// underlying buffer as the body encoder that owns it as parent, so the body
/// is wrapped at `HEADER_LEN` and the header is written at `0` afterwards —
/// `header()`/`parent()` hand the body encoder back so field writes can
/// continue.
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    let enc = JournalRecordEncoder::default().wrap(WriteBuf::new(buf), HEADER_LEN);
    let mut header = enc.header(0);
    let enc = header.parent().expect("header parent");
    encode_fields(r, enc)
}

/// Encode only the SBE body starting at offset 0 (byte-identity comparison).
pub fn encode_body(r: &JournalRecord, buf: &mut [u8]) -> usize {
    let enc = JournalRecordEncoder::default().wrap(WriteBuf::new(buf), 0);
    encode_fields(r, enc)
}

fn encode_fields(r: &JournalRecord, mut enc: JournalRecordEncoder<'_>) -> usize {
    enc.leadership_term_id(r.leadership_term_id);
    enc.log_position(r.log_position);
    enc.timestamp(r.timestamp);
    enc.cluster_session_id(r.cluster_session_id);
    enc.correlation_id(r.correlation_id);
    enc.leader_member_id(r.leader_member_id);
    enc.service_id(r.service_id);
    enc.event_type(journal::event_type::EventType::from(r.event_type));
    enc.flags(r.flags);
    let mut group = enc.entries_encoder(r.entries.len() as u16, EntriesEncoder::default());
    for e in &r.entries {
        group.advance().expect("advance");
        group.entry_term_id(e.entry_term_id);
        group.entry_index(e.entry_index);
        group.entry_timestamp(e.entry_timestamp);
        group.command_key(e.command_key);
        group.command(&e.command);
    }
    group.get_limit()
}

/// Decode header + body and fold every field (full materialization).
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(bytes), 0);
    let dec = JournalRecordDecoder::default().header(header, 0);

    let mut c = Checksum::new();
    c.add_i64(dec.leadership_term_id());
    c.add_i64(dec.log_position());
    c.add_i64(dec.timestamp());
    c.add_i64(dec.cluster_session_id());
    c.add_i64(dec.correlation_id());
    c.add_i32(dec.leader_member_id());
    c.add_i32(dec.service_id());
    c.add_u8(u8::from(dec.event_type()));
    c.add_u8(dec.flags());
    let mut group = dec.entries_decoder();
    let count = group.count();
    for _ in 0..count {
        group.advance().expect("advance").expect("entry present");
        c.add_i64(group.entry_term_id());
        c.add_i64(group.entry_index());
        c.add_i64(group.entry_timestamp());
        c.add_i32(group.command_key());
        let coords = group.command_decoder();
        c.add_bytes(group.command_slice(coords));
    }
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serialization_common::{build_record, checksum_record};

    #[test]
    fn round_trip_checksum_matches() {
        let r = build_record(9, 4, 78);
        let mut buf = vec![0u8; 64 * 1024];
        let n = encode(&r, &mut buf);
        assert!(n > 400 && n < 700, "unexpected encoded size {n}");
        assert_eq!(decode_checksum(&buf[..n]), checksum_record(&r));
    }
}
