//! sbe_gen (zerocopy SBE) codec cell.
//!
//! The generated modules are produced by build.rs into `OUT_DIR/sbe`. They
//! cross-reference via `super::types` / `super::message_header`, so they must
//! be declared as sibling submodules of one parent `mod sbe`. `types.rs` and
//! `message_header.rs` also carry a crate-root-style
//! `#![allow(dead_code, non_camel_case_types)]` inner attribute, which Rust
//! only accepts at the top of a *file*-module — not when spliced via
//! `include!` into an already-open `mod { ... }` body. build.rs therefore
//! declares each generated file as its own file-module via `#[path = ...]`
//! (baked with the absolute OUT_DIR path, since `#[path]` needs a string
//! literal, not `env!`/`concat!`) and writes that as a small shim,
//! `OUT_DIR/sbe_mod.rs`, which is pulled in with a single `include!` here.

include!(concat!(env!("OUT_DIR"), "/sbe_mod.rs"));

use sbe::journal_record::{self, JournalRecord as SbeRecord};
use sbe::types::EventType;
use serialization_common::{Checksum, JournalRecord};

/// Encode a full framed message (header + body) into `buf`.
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    let header = sbe::MessageHeader {
        block_length: zerocopy::byteorder::little_endian::U16::new(SbeRecord::BLOCK_LENGTH),
        template_id: zerocopy::byteorder::little_endian::U16::new(SbeRecord::TEMPLATE_ID),
        schema_id: zerocopy::byteorder::little_endian::U16::new(SbeRecord::SCHEMA_ID),
        version: zerocopy::byteorder::little_endian::U16::new(SbeRecord::SCHEMA_VERSION),
    };
    SbeRecord::encode_with_header_into(buf, header, |enc| {
        write_fields(r, enc);
        Ok(())
    })
    .expect("sbe_gen encode")
}

/// Encode only the SBE body (no header) — used by the byte-identity test, since
/// header framing depends on tool-specific schema-id attribute handling.
pub fn encode_body(r: &JournalRecord, buf: &mut [u8]) -> usize {
    SbeRecord::encode_body_into(buf, |enc| {
        write_fields(r, enc);
        Ok(())
    })
    .expect("sbe_gen encode body")
}

fn write_fields(r: &JournalRecord, enc: &mut journal_record::JournalRecordEncoder<'_>) {
    enc.leadership_term_id(r.leadership_term_id)
        .log_position(r.log_position)
        .timestamp(r.timestamp)
        .cluster_session_id(r.cluster_session_id)
        .correlation_id(r.correlation_id)
        .leader_member_id(r.leader_member_id)
        .service_id(r.service_id)
        .event_type(EventType(r.event_type))
        .flags(r.flags);
    enc.entries(|g| {
        for e in &r.entries {
            g.entry(|ee| {
                ee.entry_term_id(e.entry_term_id)
                    .entry_index(e.entry_index)
                    .entry_timestamp(e.entry_timestamp)
                    .command_key(e.command_key)
                    .cmd_qty(e.cmd_qty)
                    .cmd_price(e.cmd_price)
                    .cmd_flag(e.cmd_flag as u8);
                ee.cmd_text(e.cmd_text.as_bytes())?;
                Ok(())
            })?;
        }
        Ok(())
    })
    .expect("sbe_gen entries");
}

/// Decode the framed message and fold every field (full materialization).
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    // Skip the 8-byte SBE message header to reach the body.
    let body = &bytes[8..];
    let (rec, rest) = SbeRecord::parse_prefix(body).expect("sbe_gen header/body");
    let mut c = Checksum::new();
    c.add_i64(rec.leadership_term_id.get());
    c.add_i64(rec.log_position.get());
    c.add_i64(rec.timestamp.get());
    c.add_i64(rec.cluster_session_id.get());
    c.add_i64(rec.correlation_id.get());
    c.add_i32(rec.leader_member_id.get());
    c.add_i32(rec.service_id.get());
    c.add_u8(rec.event_type.0);
    c.add_u8(rec.flags);
    let group = journal_record::parse_entries(rest).expect("sbe_gen entries parse");
    for entry in group.iter() {
        c.add_i64(entry.entry_term_id().map(|v| v.get()).unwrap_or(0));
        c.add_i64(entry.entry_index().map(|v| v.get()).unwrap_or(0));
        c.add_i64(entry.entry_timestamp().map(|v| v.get()).unwrap_or(0));
        c.add_i32(entry.command_key().map(|v| v.get()).unwrap_or(0));
        c.add_i64(entry.cmd_qty().map(|v| v.get()).unwrap_or(0));
        c.add_f64(entry.cmd_price().map(|v| v.get()).unwrap_or(0.0));
        c.add_bool(entry.cmd_flag().copied().unwrap_or(0) != 0);
        // cmd_text is var-data bytes = the string's UTF-8; fold via add_str over the bytes.
        c.add_str(std::str::from_utf8(entry.cmd_text.bytes).unwrap_or(""));
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
