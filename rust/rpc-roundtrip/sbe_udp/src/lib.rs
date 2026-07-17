//! sbe_gen (zerocopy SBE) codec for the rpc-roundtrip sbe_udp cell.

include!(concat!(env!("OUT_DIR"), "/sbe_mod.rs"));

use rpc_roundtrip_common::Payload;
use sbe::rpc_payload::RpcPayload as SbeRpc;

/// Wire offset of the `hop` field: 8-byte message header + block offset 0.
pub const HOP_OFFSET: usize = 8;
/// Full framed encoded size (header + 244-byte block).
pub const ENCODED_LEN: usize = 252;

/// Encode a full framed message (header + body) into `buf`, return byte count.
pub fn encode(p: &Payload, buf: &mut [u8]) -> usize {
    let header = sbe::MessageHeader {
        block_length: zerocopy::byteorder::little_endian::U16::new(SbeRpc::BLOCK_LENGTH),
        template_id: zerocopy::byteorder::little_endian::U16::new(SbeRpc::TEMPLATE_ID),
        schema_id: zerocopy::byteorder::little_endian::U16::new(SbeRpc::SCHEMA_ID),
        version: zerocopy::byteorder::little_endian::U16::new(SbeRpc::SCHEMA_VERSION),
    };
    SbeRpc::encode_with_header_into(buf, header, |enc| {
        enc.hop(p.hop)
            .seq(p.seq)
            .timestamp(p.timestamp)
            .order_id(p.order_id)
            .price(p.price)
            .qty(p.qty)
            .symbol_id(p.symbol_id)
            .account_id(p.account_id)
            .venue_id(p.venue_id)
            .side(p.side)
            .flags(p.flags)
            .signature(p.signature)
            .context(p.context);
        Ok(())
    })
    .expect("sbe encode")
}

/// Read `hop` from a framed message (deserialize).
pub fn read_hop(bytes: &[u8]) -> u32 {
    let (rec, _) = SbeRpc::parse_prefix(&bytes[8..]).expect("sbe parse");
    rec.hop.get()
}

/// Read `seq` from a framed message.
pub fn read_seq(bytes: &[u8]) -> u64 {
    let (rec, _) = SbeRpc::parse_prefix(&bytes[8..]).expect("sbe parse");
    rec.seq.get()
}

/// Responder step: deserialize `hop`, then re-serialize `hop + 1` in place
/// (SBE fixed-layout mutate — the codec's zero-copy advantage). `buf` holds a
/// framed message; the hop field at `HOP_OFFSET` is overwritten little-endian.
pub fn mutate_hop_in_place(buf: &mut [u8]) {
    let hop = read_hop(buf);
    buf[HOP_OFFSET..HOP_OFFSET + 4].copy_from_slice(&(hop + 1).to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_roundtrip_common::build;

    #[test]
    fn encode_mutate_roundtrip() {
        let p = build(0);
        let mut buf = vec![0u8; 1024];
        let n = encode(&p, &mut buf);
        assert_eq!(n, ENCODED_LEN);
        assert_eq!(read_hop(&buf[..n]), p.hop);
        mutate_hop_in_place(&mut buf[..n]);
        assert_eq!(read_hop(&buf[..n]), p.hop + 1);
        assert_eq!(read_seq(&buf[..n]), p.seq);
    }
}
