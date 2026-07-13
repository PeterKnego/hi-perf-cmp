//! bincode (serde + bincode v2) codec cell — the ergonomic derive baseline.

use bincode::config::{Configuration, standard};
use serialization_common::{JournalRecord, checksum_record};

#[inline]
fn cfg() -> Configuration {
    standard()
}

/// Serialize into a reused caller buffer (zero-alloc encode path).
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    bincode::serde::encode_into_slice(r, buf, cfg()).expect("bincode encode")
}

/// Decode to an owned record, then fold via the canonical checksum. bincode
/// reaches full materialization by constructing the owned struct (Vecs and all).
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    let (r, _len): (JournalRecord, usize) =
        bincode::serde::decode_from_slice(bytes, cfg()).expect("bincode decode");
    checksum_record(&r)
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
