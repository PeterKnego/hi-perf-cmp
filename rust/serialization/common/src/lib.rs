//! Shared logical model for the `serialization` focus area: one ~500-byte SMR
//! journal record, a deterministic index-seeded builder, and a canonical
//! checksum every codec's decode must reproduce (the full-materialization proof).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub entry_term_id: i64,
    pub entry_index: i64,
    pub entry_timestamp: i64,
    pub command_key: i32,
    pub command: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub leadership_term_id: i64,
    pub log_position: i64,
    pub timestamp: i64,
    pub cluster_session_id: i64,
    pub correlation_id: i64,
    pub leader_member_id: i32,
    pub service_id: i32,
    pub event_type: u8,
    pub flags: u8,
    pub entries: Vec<Entry>,
}

/// Deterministic splitmix64 step — used only to spread field values from the
/// record index. Not cryptographic; chosen so a record is byte-reproducible
/// without any RNG state or wall-clock input.
#[inline]
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Build one journal record deterministically from `index`, with `entries`
/// group members each carrying a `cmd_bytes`-long command payload. Defaults of
/// `entries = 4`, `cmd_bytes = 78` encode to ~500 bytes.
pub fn build_record(index: u64, entries: usize, cmd_bytes: usize) -> JournalRecord {
    let h = mix(index);
    let mut group = Vec::with_capacity(entries);
    for k in 0..entries as u64 {
        let e = mix(h ^ k.wrapping_mul(0x0100_0000_01B3));
        let mut command = vec![0u8; cmd_bytes];
        for (i, b) in command.iter_mut().enumerate() {
            *b = (e >> (i % 8 * 8)) as u8 ^ i as u8;
        }
        group.push(Entry {
            entry_term_id: e as i64,
            entry_index: (index * entries as u64 + k) as i64,
            entry_timestamp: mix(e) as i64,
            command_key: (e >> 32) as i32,
            command,
        });
    }
    JournalRecord {
        leadership_term_id: h as i64,
        log_position: (index as i64) << 8,
        timestamp: mix(h) as i64,
        cluster_session_id: (h >> 16) as i64,
        correlation_id: mix(h ^ 0xABCD) as i64,
        leader_member_id: (h >> 8) as i32,
        service_id: (h >> 24) as i32,
        event_type: (h & 1) as u8, // 0 = APPEND, 1 = SNAPSHOT
        flags: (h >> 1) as u8,
        entries: group,
    }
}

/// Order-sensitive checksum accumulator. Every codec folds the decoded fields
/// in the same order; equal outputs prove identical materialization.
pub struct Checksum(u64);

impl Checksum {
    #[inline]
    pub fn new() -> Self {
        Checksum(0xcbf2_9ce4_8422_2325) // FNV-1a offset basis
    }
    #[inline]
    fn step(&mut self, v: u64) {
        self.0 = (self.0 ^ v).wrapping_mul(0x0000_0100_0000_01B3);
    }
    #[inline]
    pub fn add_i64(&mut self, v: i64) {
        self.step(v as u64);
    }
    #[inline]
    pub fn add_i32(&mut self, v: i32) {
        self.step(v as u32 as u64);
    }
    #[inline]
    pub fn add_u8(&mut self, v: u8) {
        self.step(v as u64);
    }
    #[inline]
    pub fn add_bytes(&mut self, b: &[u8]) {
        self.step(b.len() as u64);
        for &x in b {
            self.step(x as u64);
        }
    }
    #[inline]
    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Checksum {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical fold over a fully-owned record (the bincode path uses this after
/// decoding to an owned struct). SBE cells fold the same order manually.
pub fn checksum_record(r: &JournalRecord) -> u64 {
    let mut c = Checksum::new();
    c.add_i64(r.leadership_term_id);
    c.add_i64(r.log_position);
    c.add_i64(r.timestamp);
    c.add_i64(r.cluster_session_id);
    c.add_i64(r.correlation_id);
    c.add_i32(r.leader_member_id);
    c.add_i32(r.service_id);
    c.add_u8(r.event_type);
    c.add_u8(r.flags);
    for e in &r.entries {
        c.add_i64(e.entry_term_id);
        c.add_i64(e.entry_index);
        c.add_i64(e.entry_timestamp);
        c.add_i32(e.command_key);
        c.add_bytes(&e.command);
    }
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_record_is_deterministic() {
        let a = build_record(42, 4, 78);
        let b = build_record(42, 4, 78);
        assert_eq!(a, b);
        assert_eq!(a.entries.len(), 4);
        assert_eq!(a.entries[0].command.len(), 78);
    }

    #[test]
    fn build_record_varies_by_index() {
        assert_ne!(build_record(1, 4, 78), build_record(2, 4, 78));
    }

    #[test]
    fn checksum_matches_manual_fold() {
        let r = build_record(7, 2, 8);
        let mut c = Checksum::new();
        c.add_i64(r.leadership_term_id);
        c.add_i64(r.log_position);
        c.add_i64(r.timestamp);
        c.add_i64(r.cluster_session_id);
        c.add_i64(r.correlation_id);
        c.add_i32(r.leader_member_id);
        c.add_i32(r.service_id);
        c.add_u8(r.event_type);
        c.add_u8(r.flags);
        for e in &r.entries {
            c.add_i64(e.entry_term_id);
            c.add_i64(e.entry_index);
            c.add_i64(e.entry_timestamp);
            c.add_i32(e.command_key);
            c.add_bytes(&e.command);
        }
        assert_eq!(checksum_record(&r), c.finish());
    }
}
