//! Cross-codec conformance: all three codecs materialize the same record to the
//! same checksum, and the two SBE codecs produce byte-identical SBE bodies.

use serialization_common::{build_record, checksum_record};

fn scratch() -> Vec<u8> {
    vec![0u8; 64 * 1024]
}

#[test]
fn all_codecs_agree_on_checksum() {
    for i in 0..64u64 {
        let r = build_record(i, 4, 78);
        let want = checksum_record(&r);

        let mut b = scratch();
        let n = serialization_bincode::encode(&r, &mut b);
        assert_eq!(
            serialization_bincode::decode_checksum(&b[..n]),
            want,
            "bincode i={i}"
        );

        let mut s = scratch();
        let n = serialization_sbe_gen::encode(&r, &mut s);
        assert_eq!(
            serialization_sbe_gen::decode_checksum(&s[..n]),
            want,
            "sbe_gen i={i}"
        );

        let mut a = scratch();
        let n = serialization_aeron_sbe::encode(&r, &mut a);
        assert_eq!(
            serialization_aeron_sbe::decode_checksum(&a[..n]),
            want,
            "aeron_sbe i={i}"
        );
    }
}

#[test]
fn sbe_bodies_are_byte_identical() {
    // Both SBE toolchains implement the same wire spec, so the encoded BODY
    // (fixed block + group + var-data, excluding the header frame) must match
    // byte-for-byte for the same record.
    for i in 0..64u64 {
        let r = build_record(i, 4, 78);
        let mut a = scratch();
        let mut b = scratch();
        let na = serialization_sbe_gen::encode_body(&r, &mut a);
        let nb = serialization_aeron_sbe::encode_body(&r, &mut b);
        assert_eq!(&a[..na], &b[..nb], "SBE body mismatch at i={i}");
    }
}
