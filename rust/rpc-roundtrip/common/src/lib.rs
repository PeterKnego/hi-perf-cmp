//! Shared logical model for the `rpc-roundtrip` focus area: one flat ~250-byte
//! payload, a deterministic index-seeded builder, a canonical checksum (the
//! cross-language fairness anchor), and the `RPC_*` env config.

use bench_common::config::Mode;
use std::env;

/// The flat request/response payload (~250 bytes encoded). `hop` is the mutated
/// field (responder returns `hop + 1`); `seq` is echoed unchanged (verified).
#[derive(Clone, Debug, PartialEq)]
pub struct Payload {
    pub hop: u32,
    pub seq: u64,
    pub timestamp: i64,
    pub order_id: u64,
    pub price: i64,
    pub qty: i64,
    pub symbol_id: u32,
    pub account_id: u64,
    pub venue_id: u16,
    pub side: u8,
    pub flags: u8,
    pub signature: [u8; 32],
    pub context: [u8; 152],
}

#[inline]
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Build payload `index` deterministically (no RNG, no wall clock).
pub fn build(index: u64) -> Payload {
    let h = mix(index);
    let mut signature = [0u8; 32];
    let s = mix(h ^ 0x05);
    for (i, b) in signature.iter_mut().enumerate() {
        *b = (s >> (i % 8 * 8)) as u8 ^ i as u8;
    }
    let mut context = [0u8; 152];
    let c = mix(h ^ 0x06);
    for (i, b) in context.iter_mut().enumerate() {
        *b = (c >> (i % 8 * 8)) as u8 ^ i as u8;
    }
    Payload {
        hop: index as u32,
        seq: index,
        timestamp: mix(h) as i64,
        order_id: mix(h ^ 0x01),
        price: mix(h ^ 0x02) as i64,
        qty: mix(h ^ 0x03) as i64,
        symbol_id: (h >> 16) as u32,
        account_id: mix(h ^ 0x04),
        venue_id: (h >> 8) as u16,
        side: (h & 1) as u8,
        flags: (h >> 1) as u8,
        signature,
        context,
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Canonical order-sensitive fold over every field. Both languages reproduce
/// this exactly; the golden test anchors them to identical logical payloads.
pub fn checksum(p: &Payload) -> u64 {
    let mut a = FNV_OFFSET;
    let mut step = |v: u64| a = (a ^ v).wrapping_mul(FNV_PRIME);
    step(p.hop as u64);
    step(p.seq);
    step(p.timestamp as u64);
    step(p.order_id);
    step(p.price as u64);
    step(p.qty as u64);
    step(p.symbol_id as u64);
    step(p.account_id);
    step(p.venue_id as u64);
    step(p.side as u64);
    step(p.flags as u64);
    step(p.signature.len() as u64);
    for &x in &p.signature {
        step(x as u64);
    }
    step(p.context.len() as u64);
    for &x in &p.context {
        step(x as u64);
    }
    a
}

/// Parsed `RPC_*` env contract.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub mode: Mode,
    pub host: Option<String>,
    pub udp_port: u16,
    pub tcp_port: u16,
    pub grpc_port: u16,
    pub warmup: usize,
    pub iterations: usize,
}

impl RpcConfig {
    pub fn from_env() -> Result<RpcConfig, String> {
        let mode = parse_mode("RPC_MODE")?;
        let host = match env::var("RPC_HOST") {
            Ok(raw) => {
                let t = raw.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            Err(_) => None,
        };
        let udp_port = parse_port("RPC_UDP_PORT", 9200)?;
        let tcp_port = parse_port("RPC_TCP_PORT", 9201)?;
        let grpc_port = parse_port("RPC_GRPC_PORT", 9202)?;
        let warmup = parse_positive("RPC_WARMUP", 10_000)?;
        let iterations = parse_positive("RPC_ITERATIONS", 100_000)?;
        if mode == Mode::Client && host.is_none() {
            return Err("RPC_HOST: required in client mode (set RPC_HOST=<responder>)".to_string());
        }
        Ok(RpcConfig {
            mode,
            host,
            udp_port,
            tcp_port,
            grpc_port,
            warmup,
            iterations,
        })
    }

    pub fn require_host(&self) -> Result<&str, String> {
        self.host
            .as_deref()
            .ok_or_else(|| "RPC_HOST: required in client mode".to_string())
    }
}

fn parse_mode(name: &str) -> Result<Mode, String> {
    match env::var(name) {
        Err(_) => Ok(Mode::Loopback),
        Ok(v) => match v.trim() {
            "" | "loopback" => Ok(Mode::Loopback),
            "server" => Ok(Mode::Server),
            "client" => Ok(Mode::Client),
            other => Err(format!(
                "{name}: unknown mode {other:?} (want loopback|server|client)"
            )),
        },
    }
}

fn parse_port(name: &str, default: u16) -> Result<u16, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => {
            let p = v
                .trim()
                .parse::<u16>()
                .map_err(|_| format!("{name}: expected a u16 port, got {v:?}"))?;
            if p == 0 {
                Err(format!("{name}: must be a non-zero port, got 0"))
            } else {
                Ok(p)
            }
        }
    }
}

fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => {
            let n = v
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("{name}: expected a positive integer, got {v:?}"))?;
            if n == 0 {
                Err(format!("{name}: must be > 0"))
            } else {
                Ok(n)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_checksums() {
        // Generated from this builder on 2026-07-17.
        assert_eq!(checksum(&build(0)), 0x5169_4f16_fd78_29b6);
        assert_eq!(checksum(&build(1)), 0x42bd_19ed_5deb_1079);
        assert_eq!(checksum(&build(42)), 0x2a89_2040_2906_b171);
        assert_eq!(checksum(&build(99999)), 0x97ca_10ed_0ba9_17b7);
    }

    #[test]
    fn build_is_deterministic_and_varies() {
        assert_eq!(build(7), build(7));
        assert_ne!(build(1), build(2));
        assert_eq!(build(3).signature.len(), 32);
        assert_eq!(build(3).context.len(), 152);
    }
}
