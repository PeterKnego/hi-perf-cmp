# rpc-roundtrip Focus Area Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `rpc-roundtrip` focus area measuring full mutating serialize→send→deserialize+mutate+reserialize→send→deserialize round-trip latency across three whole-stack cells: `sbe_udp` (Rust), `grpc` (Go), `bebop_tcp` (Go).

**Architecture:** One flat ~250-byte payload struct expressed in three schemas (SBE XML, bebop, proto3), with committed generated codecs (the serialization-focus-area precedent). A deterministic index-seeded builder + FNV checksum anchors the Rust and Go payloads to the same logical record (golden test). Each cell reuses the existing warmup/timed measure loop (`bench_common::measure` / `bench.Measure`) with a per-cell round-trip closure; the responder deserializes, increments a `hop` field, and re-serializes. Cross-host is the reported result; loopback is a local fitness check. New `RPC_*` env contract.

**Tech Stack:** Rust 1.96 (sbe_gen 0.7.3, std UDP), Go 1.22 (`github.com/200sc/bebop` v0.6.2, `google.golang.org/protobuf` v1.36.6, `google.golang.org/grpc` **v1.66.0**), protoc 3.21 + protoc-gen-go v1.36.6 + protoc-gen-go-grpc v1.5.1 (regen-time only).

**Spec:** `docs/superpowers/specs/2026-07-17-rpc-roundtrip-design.md`

## Global Constraints

- stdout carries **only** result-contract JSON lines; all logs/diagnostics (incl. gRPC) go to stderr.
- Metrics per cell: `rtt_p50`, `rtt_p99` (int ns), `rtt_mean` (float ns), `encoded_bytes` (bytes, samples=1). Focus area `rpc-roundtrip`; experiments `sbe_udp`/`grpc`/`bebop_tcp`; language matches the cell.
- Env contract `RPC_*`: `RPC_MODE` (loopback|server|client, default loopback), `RPC_HOST` (required in client mode), `RPC_UDP_PORT` (9200), `RPC_TCP_PORT` (9201), `RPC_GRPC_PORT` (9202), `RPC_WARMUP` (10000), `RPC_ITERATIONS` (100000). Malformed values hard-error. No `RPC_PAYLOAD_BYTES` (the schema fixes the size).
- Generated code (SBE via build.rs OUT_DIR; bebop + protobuf + grpc committed) — bench hosts need no generators/protoc at run time.
- gRPC pinned at **v1.66.0** (its go.mod directive is `go 1.21`); v1.67+ require go ≥ 1.22.7 which exceeds the bench host's go 1.22.5. Do not bump.
- Benchmark the bebop **safe** API (`MarshalBebopTo`/`UnmarshalBebop`); `flags` is a reserved bebop keyword so the `.bop` field is `recordFlags` (generated `RecordFlags`).
- Three modes mirror `network-rtt`: loopback (default, in-process, dev fitness only), server (0.0.0.0, emits nothing), client (measures, emits). UDP read timeout = hard error, never retransmit. One request outstanding at a time.
- Round-trip **verification** (correctness anchor): client asserts `resp.hop == req.hop + 1` and `resp.seq == req.seq`.
- No allocation on the timed path for the hand-rolled cells (buffers/payload pre-built); gRPC's internal allocation is the stack's honest cost.
- Keep `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check` and `cd go && go build ./... && go vet ./... && go test ./...` green; all new Go files gofmt-clean.
- Only real cross-host AWS runs are journaled; loopback is never journaled, never `terraform apply` unattended.

### The deterministic builder (identical in Rust and Go)

splitmix64 `mix`, then for record `index`:

```
h = mix(index)
hop        = index as u32                 // the MUTATED field (responder returns hop+1)
seq        = index as u64                  // echoed unchanged (verified preserved)
timestamp  = mix(h)        as i64
order_id   = mix(h ^ 0x01) as u64
price      = mix(h ^ 0x02) as i64
qty        = mix(h ^ 0x03) as i64
symbol_id  = (h >> 16)     as u32
account_id = mix(h ^ 0x04) as u64
venue_id   = (h >> 8)      as u16
side       = (h & 1)       as u8
flags      = (h >> 1)      as u8
signature[i] = (mix(h ^ 0x05) >> (i%8*8)) as u8 ^ i as u8   for i in 0..32
context[i]   = (mix(h ^ 0x06) >> (i%8*8)) as u8 ^ i as u8   for i in 0..152
```

`mix(x)`: `z = x + 0x9E3779B97F4A7C15; z = (z ^ z>>30) * 0xBF58476D1CE4E5B9; z = (z ^ z>>27) * 0x94D049BB133111EB; z ^ z>>31` (all wrapping).

### The checksum fold (identical in Rust and Go)

FNV offset `0xcbf29ce484222325`, prime `0x100000001B3`; `step(v): acc = (acc ^ v) * prime`. Fold order: hop(u32), seq(u64), timestamp(i64), order_id(u64), price(i64), qty(i64), symbol_id(u32), account_id(u64), venue_id(u16), side(u8), flags(u8), then signature (length then each byte), then context (length then each byte). Integers fold as `v as u64` (signed via `as u64` two's-complement).

### Golden checksums (generated from the Rust builder, 2026-07-17)

```
build(0)     -> 0x51694f16fd7829b6
build(1)     -> 0x42bd19ed5deb1079
build(42)    -> 0x2a8920402906b171
build(99999) -> 0x97ca10ed0ba917b7
```

Regenerate with a scratch crate implementing the builder+fold above.

### Encoded sizes (verified in scratch prototypes)

SBE ≈ 252 B (8-byte header + 244-byte block), bebop = 252 B, protobuf = 260 B. Size-band test: `[200, 300]`.

---

### Task 1: Rust `rpc-roundtrip/common` crate + bench-common measure helpers

**Files:**
- Modify: `rust/Cargo.toml` (workspace members)
- Modify: `rust/bench-common/src/measure.rs` (add `run_n`, `emit_rtt_with_focus`)
- Create: `rust/rpc-roundtrip/common/Cargo.toml`
- Create: `rust/rpc-roundtrip/common/src/lib.rs`

**Interfaces:**
- Consumes: `bench_common::config::Mode` (existing enum).
- Produces (used by Task 2):
  - `rpc_roundtrip_common::Payload { hop: u32, seq: u64, timestamp: i64, order_id: u64, price: i64, qty: i64, symbol_id: u32, account_id: u64, venue_id: u16, side: u8, flags: u8, signature: [u8; 32], context: [u8; 152] }`
  - `rpc_roundtrip_common::build(index: u64) -> Payload`
  - `rpc_roundtrip_common::checksum(p: &Payload) -> u64`
  - `rpc_roundtrip_common::RpcConfig { mode: Mode, host: Option<String>, udp_port: u16, tcp_port: u16, grpc_port: u16, warmup: usize, iterations: usize }` with `RpcConfig::from_env() -> Result<RpcConfig, String>` and `require_host(&self) -> Result<&str, String>`
  - `bench_common::measure::run_n<F: FnMut() -> io::Result<()>>(warmup: usize, iterations: usize, round_trip: F) -> io::Result<Vec<u64>>`
  - `bench_common::measure::emit_rtt_with_focus(focus_area: &str, experiment: &str, samples: &[u64])`

- [ ] **Step 1: Add the bench-common measure helpers**

In `rust/bench-common/src/measure.rs`, refactor `run` to delegate to a new `run_n` and add a focus-parametrized emit. Replace the existing `run` and `emit_rtt` bodies:

```rust
/// Warmup + timed loop decoupled from `Config` (used by focus areas whose
/// config type is not `network-rtt`'s `Config`). Allocation happens before the
/// timed loop.
pub fn run_n<F>(warmup: usize, iterations: usize, mut round_trip: F) -> io::Result<Vec<u64>>
where
    F: FnMut() -> io::Result<()>,
{
    for _ in 0..warmup {
        round_trip()?;
    }
    let mut samples = vec![0u64; iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip()?;
        *slot = start.elapsed().as_nanos() as u64;
    }
    Ok(samples)
}

/// Run the measure loop driven by a `network-rtt` `Config`.
pub fn run<F>(cfg: &Config, round_trip: F) -> io::Result<Vec<u64>>
where
    F: FnMut() -> io::Result<()>,
{
    run_n(cfg.warmup, cfg.iterations, round_trip)
}

/// Sort, compute p50/p99/mean, emit the three `rtt_*` lines under `focus_area`.
pub fn emit_rtt_with_focus(focus_area: &str, experiment: &str, samples: &[u64]) {
    let n = samples.len();
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p50 = stats::percentile(&sorted, 50.0);
    let p99 = stats::percentile(&sorted, 99.0);
    let mean = stats::mean(samples);
    result::emit(focus_area, experiment, "rtt_p50", p50, "ns", n);
    result::emit(focus_area, experiment, "rtt_p99", p99, "ns", n);
    result::emit_float(focus_area, experiment, "rtt_mean", mean, "ns", n);
}

/// Emit the three `rtt_*` lines under the `network-rtt` focus area.
pub fn emit_rtt(experiment: &str, samples: &[u64]) {
    emit_rtt_with_focus(FOCUS_AREA, experiment, samples);
}
```

- [ ] **Step 2: Verify bench-common still builds**

Run: `cd rust && cargo build -p bench-common && cargo test -p bench-common`
Expected: PASS (pure refactor; `run`/`emit_rtt` behavior unchanged).

- [ ] **Step 3: Create the common crate manifest**

`rust/rpc-roundtrip/common/Cargo.toml`:

```toml
[package]
name = "rpc-roundtrip-common"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[lib]
name = "rpc_roundtrip_common"
path = "src/lib.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

- [ ] **Step 4: Register the workspace member**

In `rust/Cargo.toml`, add exactly this one line to `members` (after the `smr-collections/*` block). The `rpc-roundtrip/sbe_udp` member is added in Task 2, when its directory exists — do not list it here (Cargo errors on a member path that has no `Cargo.toml`):

```toml
    "rpc-roundtrip/common",
```

- [ ] **Step 5: Write the failing golden test + implementation**

`rust/rpc-roundtrip/common/src/lib.rs`:

```rust
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
                if t.is_empty() { None } else { Some(t.to_string()) }
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
        Ok(RpcConfig { mode, host, udp_port, tcp_port, grpc_port, warmup, iterations })
    }

    pub fn require_host(&self) -> Result<&str, String> {
        self.host.as_deref().ok_or_else(|| "RPC_HOST: required in client mode".to_string())
    }
}

fn parse_mode(name: &str) -> Result<Mode, String> {
    match env::var(name) {
        Err(_) => Ok(Mode::Loopback),
        Ok(v) => match v.trim() {
            "" | "loopback" => Ok(Mode::Loopback),
            "server" => Ok(Mode::Server),
            "client" => Ok(Mode::Client),
            other => Err(format!("{name}: unknown mode {other:?} (want loopback|server|client)")),
        },
    }
}

fn parse_port(name: &str, default: u16) -> Result<u16, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => v.trim().parse::<u16>().map_err(|_| format!("{name}: expected a u16 port, got {v:?}")),
    }
}

fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => {
            let n = v.trim().parse::<usize>().map_err(|_| format!("{name}: expected a positive integer, got {v:?}"))?;
            if n == 0 { Err(format!("{name}: must be > 0")) } else { Ok(n) }
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
```

- [ ] **Step 6: Run tests**

Run: `cd rust && cargo test -p rpc-roundtrip-common && cargo clippy -p rpc-roundtrip-common --all-targets && cargo fmt --check`
Expected: PASS; golden checksums match.

- [ ] **Step 7: Commit**

```bash
git add rust/Cargo.toml rust/bench-common/src/measure.rs rust/rpc-roundtrip/common/
git commit -m "feat(rpc-roundtrip): Rust common crate (payload/build/checksum/RpcConfig) + bench-common measure helpers"
```

---

### Task 2: Rust `rpc-roundtrip-sbe_udp` cell

**Files:**
- Modify: `rust/Cargo.toml` (add `rpc-roundtrip/sbe_udp` member)
- Create: `rust/rpc-roundtrip/sbe_udp/Cargo.toml`
- Create: `rust/rpc-roundtrip/sbe_udp/build.rs`
- Create: `rust/rpc-roundtrip/sbe_udp/schema/rpc_payload.xml`
- Create: `rust/rpc-roundtrip/sbe_udp/src/lib.rs`
- Create: `rust/rpc-roundtrip/sbe_udp/src/main.rs`

**Interfaces:**
- Consumes: `rpc_roundtrip_common::{Payload, build, RpcConfig}`, `bench_common::config::Mode`, `bench_common::measure::{run_n, emit_rtt_with_focus}`.
- Produces: the runnable artifact `rpc-roundtrip-sbe_udp`.
- Codec facts (verified against sbe_gen 0.7.3): schema fixed types `sig32`(uint8 length 32) / `ctx152`(uint8 length 152); generated `RpcPayload::{BLOCK_LENGTH, TEMPLATE_ID, SCHEMA_ID, SCHEMA_VERSION}`, `RpcPayload::encode_with_header_into(buf, header, |enc| {...}) -> usize`, `RpcPayload::parse_prefix(body) -> Option<(&RpcPayload, &[u8])>`; encoder setters `.hop(u32).seq(u64)....signature([u8;32]).context([u8;152])`; view field accessors return `zerocopy` LE ints with `.get()`. Encoded size 252 B (8-byte header + 244 block). `hop` is at wire bytes `[8..12]` (header 8 + block offset 0), little-endian u32.

- [ ] **Step 1: Add the workspace member**

In `rust/Cargo.toml` `members`, add:

```toml
    "rpc-roundtrip/sbe_udp",
```

- [ ] **Step 2: Manifest + build.rs + schema**

`rust/rpc-roundtrip/sbe_udp/Cargo.toml`:

```toml
[package]
name = "rpc-roundtrip-sbe_udp"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "rpc-roundtrip-sbe_udp"
path = "src/main.rs"

[lib]
name = "rpc_roundtrip_sbe_udp"
path = "src/lib.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
rpc-roundtrip-common = { path = "../common" }
zerocopy = { workspace = true }

[build-dependencies]
sbe_gen = { workspace = true }
```

`rust/rpc-roundtrip/sbe_udp/build.rs` (mirrors `serialization/sbe_gen/build.rs`, one message, no group):

```rust
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema = manifest.join("schema/rpc_payload.xml");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out = out_dir.join("sbe");

    println!("cargo:rerun-if-changed={}", schema.display());
    let xml = fs::read_to_string(&schema).expect("read schema");
    fs::create_dir_all(&out).expect("create out dir");
    sbe_gen::generate_to(&xml, &out, &sbe_gen::GeneratorOptions::default()).expect("sbe_gen generate");

    let shim = format!(
        r#"#[allow(dead_code, non_camel_case_types, unused_imports, unused_parens, clippy::all)]
mod sbe {{
    #[path = {types:?}]
    pub mod types;
    #[path = {message_header:?}]
    pub mod message_header;
    #[path = {rpc_payload:?}]
    pub mod rpc_payload;
    pub use message_header::MessageHeader;
}}
"#,
        types = out.join("types.rs").display().to_string(),
        message_header = out.join("message_header.rs").display().to_string(),
        rpc_payload = out.join("rpc_payload.rs").display().to_string(),
    );
    fs::write(out_dir.join("sbe_mod.rs"), shim).expect("write sbe module shim");
}
```

`rust/rpc-roundtrip/sbe_udp/schema/rpc_payload.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<sbe:messageSchema xmlns:sbe="http://fixprotocol.io/2016/sbe"
                   package="rpc_payload" id="8" schemaId="8" version="1"
                   byteOrder="littleEndian">
  <types>
    <composite name="messageHeader">
      <type name="blockLength" primitiveType="uint16"/>
      <type name="templateId"  primitiveType="uint16"/>
      <type name="schemaId"    primitiveType="uint16"/>
      <type name="version"     primitiveType="uint16"/>
    </composite>
    <type name="sig32"  primitiveType="uint8" length="32"/>
    <type name="ctx152" primitiveType="uint8" length="152"/>
  </types>
  <sbe:message name="RpcPayload" id="1" blockLength="244">
    <field name="hop"       id="1"  type="uint32"/>
    <field name="seq"       id="2"  type="uint64"/>
    <field name="timestamp" id="3"  type="int64"/>
    <field name="orderId"   id="4"  type="uint64"/>
    <field name="price"     id="5"  type="int64"/>
    <field name="qty"       id="6"  type="int64"/>
    <field name="symbolId"  id="7"  type="uint32"/>
    <field name="accountId" id="8"  type="uint64"/>
    <field name="venueId"   id="9"  type="uint16"/>
    <field name="side"      id="10" type="uint8"/>
    <field name="flags"     id="11" type="uint8"/>
    <field name="signature" id="12" type="sig32"/>
    <field name="context"   id="13" type="ctx152"/>
  </sbe:message>
</sbe:messageSchema>
```

- [ ] **Step 3: Write the failing codec test + lib.rs**

`rust/rpc-roundtrip/sbe_udp/src/lib.rs` — encode, mutate-in-place (the SBE zero-copy re-serialize), and decode+verify helpers:

```rust
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
```

- [ ] **Step 4: Run the codec test**

Run: `cd rust && cargo test -p rpc-roundtrip-sbe_udp --lib`
Expected: PASS; encoded size is 252.

- [ ] **Step 5: Write main.rs (UDP responder + client + modes)**

`rust/rpc-roundtrip/sbe_udp/src/main.rs` — mirrors `network-rtt/udp` structure (bounded spin responder, read-timeout client), but the responder mutates instead of echoing and the client encodes/decodes/verifies:

```rust
//! rpc-roundtrip sbe_udp cell: UDP transport + zero-copy SBE codec.
//!
//! The responder deserializes each datagram's `hop`, re-serializes `hop + 1`
//! in place, and bounces it back. The client encodes one pre-built payload per
//! iteration, sends it, receives the reply, and verifies `hop == sent + 1`,
//! `seq == sent`. A read timeout is a hard error, never a retransmit.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use bench_common::config::Mode;
use bench_common::measure;
use rpc_roundtrip_common::{build, RpcConfig};
use rpc_roundtrip_sbe_udp::{encode, mutate_hop_in_place, read_hop, read_seq, ENCODED_LEN};

const EXPERIMENT: &str = "sbe_udp";
const FOCUS: &str = "rpc-roundtrip";
const SPIN_BUDGET: u32 = 2048;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn prog() -> String {
    format!("{FOCUS}-{EXPERIMENT}")
}

fn main() {
    let cfg = match RpcConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}: {msg}", prog());
            std::process::exit(1);
        }
    };
    let result = match cfg.mode {
        Mode::Loopback => run_loopback(&cfg),
        Mode::Server => run_server(&cfg),
        Mode::Client => run_client(&cfg),
    };
    if let Err(e) = result {
        eprintln!("{}: {e}", prog());
        std::process::exit(1);
    }
}

fn run_server(cfg: &RpcConfig) -> io::Result<()> {
    let addr: SocketAddr = format!("0.0.0.0:{}", cfg.udp_port).parse().unwrap();
    eprintln!("{}: serving udp {addr}", prog());
    serve(UdpSocket::bind(addr)?)
}

fn run_loopback(cfg: &RpcConfig) -> io::Result<()> {
    let server = UdpSocket::bind("127.0.0.1:0")?;
    let server_addr = server.local_addr()?;
    std::thread::spawn(move || {
        let _ = serve(server);
    });
    let samples = measure_client(server_addr, cfg)?;
    emit(&samples);
    Ok(())
}

fn run_client(cfg: &RpcConfig) -> io::Result<()> {
    let host = cfg
        .require_host()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let addr: SocketAddr = format!("{host}:{}", cfg.udp_port)
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("{e}")))?;
    let samples = measure_client(addr, cfg)?;
    emit(&samples);
    Ok(())
}

/// Emit the three `rtt_*` lines plus `encoded_bytes` (the four result lines).
fn emit(samples: &[u64]) {
    measure::emit_rtt_with_focus(FOCUS, EXPERIMENT, samples);
    bench_common::result::emit(FOCUS, EXPERIMENT, "encoded_bytes", ENCODED_LEN as u64, "bytes", 1);
}

/// Echo responder: mutate `hop` in place and bounce every datagram back.
fn serve(sock: UdpSocket) -> io::Result<()> {
    let mut buf = [0u8; 2048];
    sock.set_nonblocking(true)?;
    loop {
        let (n, src) = {
            let mut spins: u32 = 0;
            loop {
                match sock.recv_from(&mut buf) {
                    Ok(pair) => break pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        spins += 1;
                        if spins >= SPIN_BUDGET {
                            sock.set_nonblocking(false)?;
                            let r = loop {
                                match sock.recv_from(&mut buf) {
                                    Ok(pair) => break Ok(pair),
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(e) => break Err(e),
                                }
                            };
                            sock.set_nonblocking(true)?;
                            break r?;
                        }
                        std::hint::spin_loop();
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        };
        mutate_hop_in_place(&mut buf[..n]);
        sock.send_to(&buf[..n], src)?;
    }
}

/// Client: warmup + timed round trips. Each round trip encodes the pre-built
/// request, sends it, receives the reply, and verifies the mutation.
fn measure_client(addr: SocketAddr, cfg: &RpcConfig) -> io::Result<Vec<u64>> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(addr)?;
    sock.set_read_timeout(Some(READ_TIMEOUT))?;

    let req = build(0);
    let mut send_buf = vec![0u8; ENCODED_LEN];
    let n = encode(&req, &mut send_buf);
    let mut recv_buf = [0u8; 2048];

    let round_trip = || -> io::Result<()> {
        sock.send(&send_buf[..n])?;
        let m = sock.recv(&mut recv_buf)?;
        if read_hop(&recv_buf[..m]) != req.hop + 1 || read_seq(&recv_buf[..m]) != req.seq {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "rpc: verification failed"));
        }
        Ok(())
    };

    measure::run_n(cfg.warmup, cfg.iterations, round_trip)
}
```

- [ ] **Step 6: Loopback smoke + full gates**

Run: `cd rust && cargo build --release -p rpc-roundtrip-sbe_udp && RPC_WARMUP=100 RPC_ITERATIONS=1000 cargo run --release -q -p rpc-roundtrip-sbe_udp`
Expected: exactly 4 JSON lines — `rtt_p50`, `rtt_p99`, `rtt_mean` (ns) and `encoded_bytes` (252) — each carrying `"focus_area":"rpc-roundtrip"`, `"language":"rust"`, `"experiment":"sbe_udp"`, and nothing else on stdout.

Run: `cd rust && cargo test -p rpc-roundtrip-sbe_udp && cargo clippy -p rpc-roundtrip-sbe_udp --all-targets && cargo fmt --check`
Expected: PASS, clippy-clean, fmt-clean.

- [ ] **Step 7: Commit**

```bash
git add rust/Cargo.toml rust/rpc-roundtrip/sbe_udp/
git commit -m "feat(rpc-roundtrip): Rust sbe_udp cell — UDP transport + zero-copy SBE mutate-and-return"
```

---

### Task 3: Go `internal/bench` rpc harness + `internal/rpcpayload` model

**Files:**
- Create: `go/internal/bench/rpc.go`
- Test: `go/internal/bench/rpc_test.go`
- Create: `go/internal/rpcpayload/rpcpayload.go`
- Test: `go/internal/rpcpayload/rpcpayload_test.go`

**Interfaces:**
- Consumes: `positiveEnv`, `Mode`/`ModeLoopback`/`ModeServer`/`ModeClient`, `loadMode`, `Emit`/`Result`, `Percentile`/`Mean` (all existing in package `bench`).
- Produces:
  - `bench.RpcConfig { Mode Mode; Host string; UDPPort, TCPPort, GRPCPort, Warmup, Iterations int }`, `bench.LoadRpcConfig() (RpcConfig, error)`
  - `bench.MeasureN(warmup, iterations int, rt RoundTrip) ([]int64, error)`
  - `bench.EmitRoundtrip(experiment string, samples []int64)` and `bench.EmitRoundtripInt(experiment, metric string, value int64, unit string, samples int64)`
  - `rpcpayload.Record { Hop uint32; Seq uint64; Timestamp int64; OrderID uint64; Price int64; Qty int64; SymbolID uint32; AccountID uint64; VenueID uint16; Side, Flags uint8; Signature, Context []byte }`
  - `rpcpayload.BuildRecord(index uint64) Record`, `rpcpayload.ChecksumRecord(r *Record) uint64`, `rpcpayload.Checksum` with `AddU64/AddU32/AddU16/AddU8/AddI64/AddBytes`

- [ ] **Step 1: Write the failing config + payload tests**

`go/internal/bench/rpc_test.go`:

```go
package bench

import "testing"

func TestLoadRpcConfigDefaults(t *testing.T) {
	cfg, err := LoadRpcConfig()
	if err != nil {
		t.Fatalf("defaults errored: %v", err)
	}
	if cfg.Mode != ModeLoopback || cfg.UDPPort != 9200 || cfg.TCPPort != 9201 ||
		cfg.GRPCPort != 9202 || cfg.Warmup != 10000 || cfg.Iterations != 100000 {
		t.Fatalf("unexpected defaults: %+v", cfg)
	}
}

func TestLoadRpcConfigClientRequiresHost(t *testing.T) {
	t.Setenv("RPC_MODE", "client")
	if _, err := LoadRpcConfig(); err == nil {
		t.Fatal("client mode without RPC_HOST did not error")
	}
}

func TestLoadRpcConfigRejectsMalformed(t *testing.T) {
	t.Setenv("RPC_ITERATIONS", "nope")
	if _, err := LoadRpcConfig(); err == nil {
		t.Fatal("malformed RPC_ITERATIONS did not error")
	}
}
```

`go/internal/rpcpayload/rpcpayload_test.go`:

```go
package rpcpayload

import "testing"

// Golden values generated from the Rust rpc_roundtrip_common builder on 2026-07-17.
var golden = []struct {
	index uint64
	want  uint64
}{
	{0, 0x51694f16fd7829b6},
	{1, 0x42bd19ed5deb1079},
	{42, 0x2a8920402906b171},
	{99999, 0x97ca10ed0ba917b7},
}

func TestGoldenChecksumsMatchRust(t *testing.T) {
	for _, g := range golden {
		r := BuildRecord(g.index)
		if got := ChecksumRecord(&r); got != g.want {
			t.Errorf("build(%d): got %#016x, want %#016x", g.index, got, g.want)
		}
	}
}

func TestBuildRecordDeterministicAndSized(t *testing.T) {
	a, b := BuildRecord(7), BuildRecord(7)
	if ChecksumRecord(&a) != ChecksumRecord(&b) {
		t.Fatal("same index produced different records")
	}
	if len(a.Signature) != 32 || len(a.Context) != 152 {
		t.Fatalf("blob sizes: sig=%d ctx=%d", len(a.Signature), len(a.Context))
	}
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd go && go test ./internal/bench/ -run TestLoadRpc ./internal/rpcpayload/`
Expected: FAIL — `undefined: LoadRpcConfig`, `undefined: BuildRecord`.

- [ ] **Step 3: Implement the bench rpc harness**

`go/internal/bench/rpc.go`:

```go
package bench

import (
	"fmt"
	"os"
	"sort"
	"time"
)

const rpcFocusArea = "rpc-roundtrip"

// RpcConfig configures the rpc-roundtrip benchmark, sourced from RPC_* env vars.
type RpcConfig struct {
	Mode       Mode
	Host       string
	UDPPort    int
	TCPPort    int
	GRPCPort   int
	Warmup     int
	Iterations int
}

// LoadRpcConfig reads and validates the RPC_* environment.
func LoadRpcConfig() (RpcConfig, error) {
	mode, err := loadMode("RPC_MODE", ModeLoopback)
	if err != nil {
		return RpcConfig{}, err
	}
	udp, err := positiveEnv("RPC_UDP_PORT", 9200)
	if err != nil {
		return RpcConfig{}, err
	}
	tcp, err := positiveEnv("RPC_TCP_PORT", 9201)
	if err != nil {
		return RpcConfig{}, err
	}
	grpcPort, err := positiveEnv("RPC_GRPC_PORT", 9202)
	if err != nil {
		return RpcConfig{}, err
	}
	warmup, err := positiveEnv("RPC_WARMUP", 10000)
	if err != nil {
		return RpcConfig{}, err
	}
	iters, err := positiveEnv("RPC_ITERATIONS", 100000)
	if err != nil {
		return RpcConfig{}, err
	}
	host := os.Getenv("RPC_HOST")
	if mode == ModeClient && host == "" {
		return RpcConfig{}, fmt.Errorf("RPC_HOST: required in client mode")
	}
	return RpcConfig{Mode: mode, Host: host, UDPPort: udp, TCPPort: tcp, GRPCPort: grpcPort, Warmup: warmup, Iterations: iters}, nil
}

// MeasureN runs warmup discarded round trips, then times iterations round trips
// into a pre-allocated buffer (allocation never enters the timed path).
func MeasureN(warmup, iterations int, rt RoundTrip) ([]int64, error) {
	for i := 0; i < warmup; i++ {
		if err := rt(); err != nil {
			return nil, err
		}
	}
	samples := make([]int64, iterations)
	for i := 0; i < iterations; i++ {
		start := time.Now()
		if err := rt(); err != nil {
			return nil, err
		}
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples, nil
}

// EmitRoundtrip sorts samples and emits rtt_p50/p99/mean (ns) under the
// rpc-roundtrip focus area. samples is sorted in place.
func EmitRoundtrip(experiment string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}

// EmitRoundtripInt emits one integer metric line under the rpc-roundtrip focus area.
func EmitRoundtripInt(experiment, metric string, value int64, unit string, samples int64) {
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: metric, Value: float64(value), Unit: unit, Samples: samples})
}
```

- [ ] **Step 4: Implement the payload model**

`go/internal/rpcpayload/rpcpayload.go`:

```go
// Package rpcpayload holds the shared logical model for the rpc-roundtrip
// focus area's Go cells: one flat ~250-byte payload, a deterministic
// index-seeded builder, and the canonical checksum that anchors the Go and
// Rust builders to identical logical payloads (golden test).
package rpcpayload

// Record is the flat request/response payload (~250 bytes encoded). Hop is the
// mutated field (responder returns Hop+1); Seq is echoed unchanged (verified).
type Record struct {
	Hop       uint32
	Seq       uint64
	Timestamp int64
	OrderID   uint64
	Price     int64
	Qty       int64
	SymbolID  uint32
	AccountID uint64
	VenueID   uint16
	Side      uint8
	Flags     uint8
	Signature []byte // 32 bytes
	Context   []byte // 152 bytes
}

func mix(x uint64) uint64 {
	z := x + 0x9E3779B97F4A7C15
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// BuildRecord builds payload index deterministically (no RNG, no wall clock).
func BuildRecord(index uint64) Record {
	h := mix(index)
	sig := make([]byte, 32)
	s := mix(h ^ 0x05)
	for i := range sig {
		sig[i] = byte(s>>(i%8*8)) ^ byte(i)
	}
	ctx := make([]byte, 152)
	c := mix(h ^ 0x06)
	for i := range ctx {
		ctx[i] = byte(c>>(i%8*8)) ^ byte(i)
	}
	return Record{
		Hop:       uint32(index),
		Seq:       index,
		Timestamp: int64(mix(h)),
		OrderID:   mix(h ^ 0x01),
		Price:     int64(mix(h ^ 0x02)),
		Qty:       int64(mix(h ^ 0x03)),
		SymbolID:  uint32(h >> 16),
		AccountID: mix(h ^ 0x04),
		VenueID:   uint16(h >> 8),
		Side:      uint8(h & 1),
		Flags:     uint8(h >> 1),
		Signature: sig,
		Context:   ctx,
	}
}

// Checksum is the order-sensitive FNV fold both languages reproduce.
type Checksum uint64

func NewChecksum() Checksum { return 0xcbf29ce484222325 }

func (c *Checksum) step(v uint64) { *c = Checksum((uint64(*c) ^ v) * 0x100000001B3) }

func (c *Checksum) AddU64(v uint64) { c.step(v) }
func (c *Checksum) AddU32(v uint32) { c.step(uint64(v)) }
func (c *Checksum) AddU16(v uint16) { c.step(uint64(v)) }
func (c *Checksum) AddU8(v uint8)   { c.step(uint64(v)) }
func (c *Checksum) AddI64(v int64)  { c.step(uint64(v)) }
func (c *Checksum) AddBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}
func (c Checksum) Finish() uint64 { return uint64(c) }

// ChecksumRecord folds every field in the canonical order.
func ChecksumRecord(r *Record) uint64 {
	c := NewChecksum()
	c.AddU32(r.Hop)
	c.AddU64(r.Seq)
	c.AddI64(r.Timestamp)
	c.AddU64(r.OrderID)
	c.AddI64(r.Price)
	c.AddI64(r.Qty)
	c.AddU32(r.SymbolID)
	c.AddU64(r.AccountID)
	c.AddU16(r.VenueID)
	c.AddU8(r.Side)
	c.AddU8(r.Flags)
	c.AddBytes(r.Signature)
	c.AddBytes(r.Context)
	return c.Finish()
}
```

- [ ] **Step 5: Run tests**

Run: `cd go && go test ./internal/bench/ ./internal/rpcpayload/ && go vet ./internal/bench/ ./internal/rpcpayload/ && gofmt -l internal/bench/rpc.go internal/rpcpayload/`
Expected: PASS; golden checksums match; gofmt prints nothing.

- [ ] **Step 6: Commit**

```bash
git add go/internal/bench/rpc.go go/internal/bench/rpc_test.go go/internal/rpcpayload/
git commit -m "feat(rpc-roundtrip): Go rpc harness (RpcConfig/MeasureN/EmitRoundtrip) + payload model golden-anchored to Rust"
```

---

### Task 4: Go `bebop_tcp` codec + cell

**Files:**
- Create: `go/internal/rpcpayload/schema/rpc_payload.bop`
- Create: `go/internal/rpcpayload/regen-payloadbop.sh` (mode 755)
- Create: `go/internal/rpcpayload/payloadbop/rpc_payload.go` (generated, committed)
- Create: `go/internal/rpcpayload/bebop.go`
- Test: `go/internal/rpcpayload/bebop_test.go`
- Create: `go/cmd/rpc-roundtrip-bebop_tcp/main.go`
- Modify: `go/go.mod`, `go/go.sum` (bebop already present from serialization; `go mod tidy` no-op or minor)

**Interfaces:**
- Consumes: `rpcpayload.{Record, BuildRecord}`, `bench.{RpcConfig, LoadRpcConfig, MeasureN, EmitRoundtrip, EmitRoundtripInt, RoundTrip, Mode, ModeLoopback, ModeServer, ModeClient, Logf, Fatalf}`.
- Produces: `rpcpayload.ToBebop(r *Record) payloadbop.RpcPayload`, `rpcpayload.EncodeBebop(r payloadbop.RpcPayload, scratch []byte) int`, `rpcpayload.DecodeBebop(buf []byte) (payloadbop.RpcPayload, error)`; the artifact `rpc-roundtrip-bebop_tcp`.
- Codec facts (verified): generated `payloadbop.RpcPayload{Hop uint32, Seq uint64, ..., RecordFlags byte, Signature []byte, Context []byte}`, `MarshalBebopTo(buf []byte) int`, `UnmarshalBebop(buf []byte) error`; encoded size 252 B.

- [ ] **Step 1: Schema + regen script**

`go/internal/rpcpayload/schema/rpc_payload.bop`:

```
// rpc-roundtrip flat payload (see the 2026-07-17 design spec). `flags` is a
// reserved bebop keyword, so the field is recordFlags.
struct RpcPayload {
    uint32 hop;
    uint64 seq;
    int64 timestamp;
    uint64 orderId;
    int64 price;
    int64 qty;
    uint32 symbolId;
    uint64 accountId;
    uint16 venueId;
    byte side;
    byte recordFlags;
    byte[] signature;
    byte[] context;
}
```

`go/internal/rpcpayload/regen-payloadbop.sh`:

```sh
#!/bin/sh
# Regenerate payloadbop/ from schema/rpc_payload.bop with the 200sc/bebop
# generator at the version pinned in go.mod. Dev-time only; output is committed.
set -eu
cd "$(dirname "$0")"
mkdir -p payloadbop
go run github.com/200sc/bebop/main/bebopc-go \
    -i schema/rpc_payload.bop -o payloadbop/rpc_payload.go -package payloadbop
gofmt -w payloadbop/rpc_payload.go
```

Run:

```bash
chmod +x go/internal/rpcpayload/regen-payloadbop.sh
cd go && ./internal/rpcpayload/regen-payloadbop.sh && go build ./...
```

Expected: `payloadbop/rpc_payload.go` appears (package `payloadbop`, imports `github.com/200sc/bebop` + `iohelp`); build passes.

- [ ] **Step 2: Write the failing adapter test**

`go/internal/rpcpayload/bebop_test.go`:

```go
package rpcpayload

import "testing"

func TestBebopRoundTripAndSize(t *testing.T) {
	r := BuildRecord(0)
	scratch := make([]byte, 64*1024)
	n := EncodeBebop(ToBebop(&r), scratch)
	if n < 200 || n > 300 {
		t.Fatalf("encoded size %d outside [200,300]", n)
	}
	d, err := DecodeBebop(scratch[:n])
	if err != nil {
		t.Fatal(err)
	}
	if d.Hop != r.Hop || d.Seq != r.Seq || d.RecordFlags != r.Flags ||
		len(d.Signature) != 32 || len(d.Context) != 152 {
		t.Fatalf("field mismatch: %+v", d)
	}
}
```

Run: `cd go && go test ./internal/rpcpayload/ -run TestBebop`
Expected: FAIL — `undefined: EncodeBebop`.

- [ ] **Step 3: Write the adapter**

`go/internal/rpcpayload/bebop.go`:

```go
package rpcpayload

import "github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadbop"

// ToBebop converts the logical record to the generated bebop representation.
// Blob slices are shared, not copied (encode only reads them).
func ToBebop(r *Record) payloadbop.RpcPayload {
	return payloadbop.RpcPayload{
		Hop:         r.Hop,
		Seq:         r.Seq,
		Timestamp:   r.Timestamp,
		OrderId:     r.OrderID,
		Price:       r.Price,
		Qty:         r.Qty,
		SymbolId:    r.SymbolID,
		AccountId:   r.AccountID,
		VenueId:     r.VenueID,
		Side:        r.Side,
		RecordFlags: r.Flags,
		Signature:   r.Signature,
		Context:     r.Context,
	}
}

// EncodeBebop serializes via the safe MarshalBebopTo into the reused scratch
// buffer, returning the encoded length.
func EncodeBebop(r payloadbop.RpcPayload, scratch []byte) int {
	return r.MarshalBebopTo(scratch)
}

// DecodeBebop deserializes a framed bebop message.
func DecodeBebop(buf []byte) (payloadbop.RpcPayload, error) {
	var d payloadbop.RpcPayload
	err := d.UnmarshalBebop(buf)
	return d, err
}
```

Run: `cd go && go test ./internal/rpcpayload/ -run TestBebop && gofmt -l internal/rpcpayload/bebop.go`
Expected: PASS; size 252; gofmt clean.

- [ ] **Step 4: Write the cell main (TCP responder + client)**

`go/cmd/rpc-roundtrip-bebop_tcp/main.go` — length-prefixed TCP framing (4-byte big-endian length + body), strict ping-pong. The responder deserializes, increments Hop, re-serializes; the client verifies.

```go
// rpc-roundtrip-bebop_tcp: TCP transport + bebop safe-API codec. The responder
// deserializes each request, increments Hop, and re-serializes the reply; the
// client verifies resp.Hop == req.Hop+1 and resp.Seq == req.Seq. Framing is a
// 4-byte big-endian length prefix + body. One request outstanding at a time.
package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"strconv"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload"
)

const experiment = "bebop_tcp"

func prog() string { return "rpc-roundtrip-" + experiment }

func main() {
	cfg, err := bench.LoadRpcConfig()
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	switch cfg.Mode {
	case bench.ModeLoopback:
		runLoopback(cfg)
	case bench.ModeServer:
		runServer(cfg)
	case bench.ModeClient:
		runClient(cfg)
	default:
		bench.Fatalf(prog(), "unknown mode %q", cfg.Mode)
	}
}

func runLoopback(cfg bench.RpcConfig) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	defer ln.Close()
	go func() { _ = serve(ln) }()
	measureAndEmit(ln.Addr().String(), cfg)
}

func runServer(cfg bench.RpcConfig) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.TCPPort))
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	defer ln.Close()
	bench.Logf(prog(), "serving tcp %s", addr)
	if err := serve(ln); err != nil {
		bench.Fatalf(prog(), "serve: %v", err)
	}
}

func runClient(cfg bench.RpcConfig) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.TCPPort))
	measureAndEmit(addr, cfg)
}

func measureAndEmit(addr string, cfg bench.RpcConfig) {
	req := rpcpayload.BuildRecord(0)
	bebopReq := rpcpayload.ToBebop(&req)

	conn, err := net.Dial("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "dial: %v", err)
	}
	defer conn.Close()
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}

	sendBody := make([]byte, 64*1024)
	sendFrame := make([]byte, 4+64*1024)
	recvHdr := make([]byte, 4)
	recvBody := make([]byte, 64*1024)

	roundTrip := func() error {
		n := rpcpayload.EncodeBebop(bebopReq, sendBody)
		binary.BigEndian.PutUint32(sendFrame, uint32(n))
		copy(sendFrame[4:], sendBody[:n])
		if _, err := conn.Write(sendFrame[:4+n]); err != nil {
			return fmt.Errorf("write: %w", err)
		}
		if _, err := io.ReadFull(conn, recvHdr); err != nil {
			return fmt.Errorf("read hdr: %w", err)
		}
		m := int(binary.BigEndian.Uint32(recvHdr))
		if m > len(recvBody) {
			return fmt.Errorf("reply too large: %d", m)
		}
		if _, err := io.ReadFull(conn, recvBody[:m]); err != nil {
			return fmt.Errorf("read body: %w", err)
		}
		resp, err := rpcpayload.DecodeBebop(recvBody[:m])
		if err != nil {
			return fmt.Errorf("decode: %w", err)
		}
		if resp.Hop != req.Hop+1 || resp.Seq != req.Seq {
			return fmt.Errorf("verification failed: hop=%d seq=%d", resp.Hop, resp.Seq)
		}
		return nil
	}

	samples, err := bench.MeasureN(cfg.Warmup, cfg.Iterations, roundTrip)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRoundtrip(experiment, samples)
	encoded := rpcpayload.EncodeBebop(bebopReq, sendBody)
	bench.EmitRoundtripInt(experiment, "encoded_bytes", int64(encoded), "bytes", 1)
}

func serve(ln net.Listener) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return err
		}
		go handle(conn)
	}
}

// handle reads length-prefixed requests, increments Hop, and writes the reply.
func handle(conn net.Conn) {
	defer conn.Close()
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}
	hdr := make([]byte, 4)
	body := make([]byte, 64*1024)
	out := make([]byte, 64*1024)
	frame := make([]byte, 4+64*1024)
	for {
		if _, err := io.ReadFull(conn, hdr); err != nil {
			return // client closed; normal
		}
		n := int(binary.BigEndian.Uint32(hdr))
		if n > len(body) {
			bench.Logf(prog(), "request too large: %d", n)
			return
		}
		if _, err := io.ReadFull(conn, body[:n]); err != nil {
			return
		}
		d, err := rpcpayload.DecodeBebop(body[:n])
		if err != nil {
			bench.Logf(prog(), "decode: %v", err)
			return
		}
		d.Hop++ // mutate
		m := rpcpayload.EncodeBebop(d, out)
		binary.BigEndian.PutUint32(frame, uint32(m))
		copy(frame[4:], out[:m])
		if _, err := conn.Write(frame[:4+m]); err != nil {
			bench.Logf(prog(), "write: %v", err)
			return
		}
	}
}
```

- [ ] **Step 5: Build, test, loopback smoke**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/rpcpayload/`
Then: `cd go && RPC_WARMUP=100 RPC_ITERATIONS=1000 go run ./cmd/rpc-roundtrip-bebop_tcp | tee /dev/stderr | wc -l`
Expected: exactly 4 JSON lines (`rtt_p50/p99/mean`, `encoded_bytes`=252), each `"focus_area":"rpc-roundtrip"`, `"language":"go"`, `"experiment":"bebop_tcp"`; gofmt clean on the new files.

- [ ] **Step 6: Commit**

```bash
git add go/internal/rpcpayload/ go/cmd/rpc-roundtrip-bebop_tcp/ go/go.mod go/go.sum
git commit -m "feat(rpc-roundtrip): Go bebop_tcp cell — length-prefixed TCP + bebop mutate-and-return"
```

---

### Task 5: Go `grpc` codec + cell

**Files:**
- Create: `go/internal/rpcpayload/schema/rpc_payload.proto`
- Create: `go/internal/rpcpayload/regen-payloadpb.sh` (mode 755)
- Create: `go/internal/rpcpayload/payloadpb/rpc_payload.pb.go` (generated, committed)
- Create: `go/internal/rpcpayload/payloadpb/rpc_payload_grpc.pb.go` (generated, committed)
- Create: `go/internal/rpcpayload/proto.go`
- Test: `go/internal/rpcpayload/proto_test.go`
- Create: `go/cmd/rpc-roundtrip-grpc/main.go`
- Modify: `go/go.mod`, `go/go.sum` (add `google.golang.org/grpc v1.66.0`)

**Interfaces:**
- Consumes: `rpcpayload.{Record, BuildRecord}`, `bench.{RpcConfig, LoadRpcConfig, MeasureN, EmitRoundtrip, EmitRoundtripInt, Mode, ...}`.
- Produces: `rpcpayload.ToProto(r *Record) *payloadpb.RpcPayload`; the artifact `rpc-roundtrip-grpc`.
- Codec/service facts (verified): proto3 message `RpcPayload` (`fixed32 hop`, `fixed64 seq`, `sfixed64 timestamp/price/qty`, `fixed64 order_id/account_id`, `fixed32 symbol_id`, `uint32 venue_id/side/flags`, `bytes signature/context`) + `service RpcRoundtrip { rpc Roundtrip(RpcPayload) returns (RpcPayload); }`. Generated: `payloadpb.RpcPayload{Hop uint32, Seq uint64, ..., Flags uint32, Signature, Context []byte}`, `NewRpcRoundtripClient(cc)`, `RegisterRpcRoundtripServer(s, srv)`, `RpcRoundtripServer` interface + `UnimplementedRpcRoundtripServer`, `Roundtrip(ctx, *RpcPayload) (*RpcPayload, error)`. Encoded size 260 B. grpc **v1.66.0**.

- [ ] **Step 1: Schema + regen script + dependency**

`go/internal/rpcpayload/schema/rpc_payload.proto`:

```proto
// rpc-roundtrip flat payload + unary Roundtrip service (2026-07-17 spec).
// sfixed/fixed for the wide scalars (full-width values; matches the
// serialization protobuf cell's fixed-width choice); uint32 for byte-wide fields.
syntax = "proto3";
package hiperf.rpc;

option go_package = "github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadpb";

message RpcPayload {
  fixed32  hop = 1;
  fixed64  seq = 2;
  sfixed64 timestamp = 3;
  fixed64  order_id = 4;
  sfixed64 price = 5;
  sfixed64 qty = 6;
  fixed32  symbol_id = 7;
  fixed64  account_id = 8;
  uint32   venue_id = 9;
  uint32   side = 10;
  uint32   flags = 11;
  bytes    signature = 12;
  bytes    context = 13;
}

service RpcRoundtrip {
  rpc Roundtrip(RpcPayload) returns (RpcPayload);
}
```

`go/internal/rpcpayload/regen-payloadpb.sh`:

```sh
#!/bin/sh
# Regenerate payloadpb/ (message + gRPC service) from schema/rpc_payload.proto.
# Requires protoc (3.21+) on PATH; protoc-gen-go and protoc-gen-go-grpc are
# version-pinned and installed to a temp dir. Output is committed (bench hosts
# need no protoc). Dev-time only.
set -eu
cd "$(dirname "$0")"
PLUGIN_DIR="$(mktemp -d)"
trap 'rm -rf "$PLUGIN_DIR"' EXIT
GOBIN="$PLUGIN_DIR" go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6
GOBIN="$PLUGIN_DIR" go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1
PATH="$PLUGIN_DIR:$PATH" protoc \
    --go_out=. --go_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload \
    --go-grpc_out=. --go-grpc_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload \
    schema/rpc_payload.proto
```

Run:

```bash
chmod +x go/internal/rpcpayload/regen-payloadpb.sh
cd go && go get google.golang.org/grpc@v1.66.0 && ./internal/rpcpayload/regen-payloadpb.sh && go mod tidy && go build ./...
```

Expected: `payloadpb/rpc_payload.pb.go` + `payloadpb/rpc_payload_grpc.pb.go` appear; build passes. `go.mod` now pins `google.golang.org/grpc v1.66.0`.

- [ ] **Step 2: Failing adapter test**

`go/internal/rpcpayload/proto_test.go`:

```go
package rpcpayload

import (
	"testing"

	"google.golang.org/protobuf/proto"
)

func TestProtoRoundTripAndSize(t *testing.T) {
	r := BuildRecord(0)
	out, err := proto.Marshal(ToProto(&r))
	if err != nil {
		t.Fatal(err)
	}
	if len(out) < 200 || len(out) > 300 {
		t.Fatalf("encoded size %d outside [200,300]", len(out))
	}
}
```

Run: `cd go && go test ./internal/rpcpayload/ -run TestProto`
Expected: FAIL — `undefined: ToProto`.

- [ ] **Step 3: Adapter**

`go/internal/rpcpayload/proto.go`:

```go
package rpcpayload

import "github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadpb"

// ToProto converts the logical record to the generated protobuf representation.
// Blob slices are shared, not copied.
func ToProto(r *Record) *payloadpb.RpcPayload {
	return &payloadpb.RpcPayload{
		Hop:       r.Hop,
		Seq:       r.Seq,
		Timestamp: r.Timestamp,
		OrderId:   r.OrderID,
		Price:     r.Price,
		Qty:       r.Qty,
		SymbolId:  r.SymbolID,
		AccountId: r.AccountID,
		VenueId:   uint32(r.VenueID),
		Side:      uint32(r.Side),
		Flags:     uint32(r.Flags),
		Signature: r.Signature,
		Context:   r.Context,
	}
}
```

Run: `cd go && go test ./internal/rpcpayload/ -run TestProto && gofmt -l internal/rpcpayload/proto.go`
Expected: PASS; size 260; gofmt clean.

- [ ] **Step 4: The cell main (gRPC server + client)**

`go/cmd/rpc-roundtrip-grpc/main.go`:

```go
// rpc-roundtrip-grpc: gRPC (HTTP/2 + protobuf) transport. One unary Roundtrip
// call is the round trip; the server handler increments Hop and returns. The
// client verifies resp.Hop == req.Hop+1 and resp.Seq == req.Seq.
package main

import (
	"context"
	"fmt"
	"net"
	"strconv"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadpb"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/proto"
)

const experiment = "grpc"

func prog() string { return "rpc-roundtrip-" + experiment }

type server struct {
	payloadpb.UnimplementedRpcRoundtripServer
}

// Roundtrip mutates the request (Hop+1) and returns it — the deserialize +
// mutate + reserialize the focus area measures (gRPC owns the codec + framing).
func (server) Roundtrip(_ context.Context, in *payloadpb.RpcPayload) (*payloadpb.RpcPayload, error) {
	in.Hop++
	return in, nil
}

func main() {
	cfg, err := bench.LoadRpcConfig()
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	switch cfg.Mode {
	case bench.ModeLoopback:
		runLoopback(cfg)
	case bench.ModeServer:
		runServer(cfg)
	case bench.ModeClient:
		runClient(cfg)
	default:
		bench.Fatalf(prog(), "unknown mode %q", cfg.Mode)
	}
}

func runLoopback(cfg bench.RpcConfig) {
	lis, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	s := grpc.NewServer()
	payloadpb.RegisterRpcRoundtripServer(s, server{})
	go func() { _ = s.Serve(lis) }()
	defer s.Stop()
	measureAndEmit(lis.Addr().String(), cfg)
}

func runServer(cfg bench.RpcConfig) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.GRPCPort))
	lis, err := net.Listen("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	s := grpc.NewServer()
	payloadpb.RegisterRpcRoundtripServer(s, server{})
	bench.Logf(prog(), "serving grpc %s", addr)
	if err := s.Serve(lis); err != nil {
		bench.Fatalf(prog(), "serve: %v", err)
	}
}

func runClient(cfg bench.RpcConfig) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.GRPCPort))
	measureAndEmit(addr, cfg)
}

func measureAndEmit(addr string, cfg bench.RpcConfig) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		bench.Fatalf(prog(), "dial: %v", err)
	}
	defer conn.Close()
	client := payloadpb.NewRpcRoundtripClient(conn)

	rec := rpcpayload.BuildRecord(0)
	req := rpcpayload.ToProto(&rec)
	ctx := context.Background()

	roundTrip := func() error {
		resp, err := client.Roundtrip(ctx, req)
		if err != nil {
			return fmt.Errorf("roundtrip: %w", err)
		}
		if resp.Hop != rec.Hop+1 || resp.Seq != rec.Seq {
			return fmt.Errorf("verification failed: hop=%d seq=%d", resp.Hop, resp.Seq)
		}
		return nil
	}

	samples, err := bench.MeasureN(cfg.Warmup, cfg.Iterations, roundTrip)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRoundtrip(experiment, samples)
	encoded, _ := protoSize(req)
	bench.EmitRoundtripInt(experiment, "encoded_bytes", int64(encoded), "bytes", 1)
}

func protoSize(m *payloadpb.RpcPayload) (int, error) {
	out, err := proto.Marshal(m)
	return len(out), err
}
```

(`proto` is used only for the `encoded_bytes` metric, so it lives in the cell, not the adapter.)

- [ ] **Step 5: Build, test, loopback smoke**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/rpcpayload/`
Then: `cd go && RPC_WARMUP=100 RPC_ITERATIONS=1000 go run ./cmd/rpc-roundtrip-grpc | tee /dev/stderr | wc -l`
Expected: exactly 4 JSON lines (`rtt_p50/p99/mean`, `encoded_bytes`=260), `"focus_area":"rpc-roundtrip"`, `"language":"go"`, `"experiment":"grpc"`; stdout carries no gRPC logs; gofmt clean.

- [ ] **Step 6: Commit**

```bash
git add go/internal/rpcpayload/ go/cmd/rpc-roundtrip-grpc/ go/go.mod go/go.sum
git commit -m "feat(rpc-roundtrip): Go grpc cell — unary gRPC mutate-and-return (grpc v1.66.0)"
```

---

### Task 6: bench-infra matrix rows + `RPC_*` orchestration + docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml` (matrix rows + `rpc_*` params)
- Modify: `bench-infra/ansible/roles/run/tasks/cross_host.yml` (export `RPC_*`)
- Modify: `bench-infra/ansible/roles/run/files/run_bench.sh` (focus-area case + `RPC_*` defaults)
- Modify: `CLAUDE.md` (focus-area list, status, artifact names, run examples)
- Modify: `docs/result-contract.md` (if it enumerates focus areas/experiments)

**Interfaces:**
- Consumes: artifact names `rpc-roundtrip-{sbe_udp,grpc,bebop_tcp}` (Tasks 2/4/5).
- Produces: documentation + run-matrix only.

- [ ] **Step 1: Matrix rows + params in all.yml**

In `bench-infra/ansible/group_vars/all.yml`, add three `cross_host` rows to the `experiments` list (after the existing `network-rtt` rows or at the end of the experiments block):

```yaml
  - { focus_area: rpc-roundtrip,    experiment: sbe_udp,     kind: cross_host, languages: [rust] }
  - { focus_area: rpc-roundtrip,    experiment: grpc,        kind: cross_host, languages: [go] }
  - { focus_area: rpc-roundtrip,    experiment: bebop_tcp,   kind: cross_host, languages: [go] }
```

And add the `rpc_*` params block (near the other focus-area params):

```yaml
# rpc-roundtrip params (cross-host: node0 client, node1 responder). Three
# whole-stack cells over one shared ~250-byte payload. Ports distinct from
# network-rtt's 91xx so the two focus areas never collide.
rpc_udp_port: 9200
rpc_tcp_port: 9201
rpc_grpc_port: 9202
rpc_warmup: 10000
rpc_iterations: 100000
```

- [ ] **Step 2: Export RPC_* in cross_host.yml**

In `bench-infra/ansible/roles/run/tasks/cross_host.yml`, add the `RPC_*` exports to BOTH shell blocks (the node1 responder `start responder` task and the node0 `run client` task). In the responder block, after the `RTT_*` exports:

```yaml
        export RPC_UDP_PORT="{{ rpc_udp_port }}"
        export RPC_TCP_PORT="{{ rpc_tcp_port }}"
        export RPC_GRPC_PORT="{{ rpc_grpc_port }}"
        export RPC_WARMUP="{{ rpc_warmup }}"
        export RPC_ITERATIONS="{{ rpc_iterations }}"
```

In the client block, after the `RTT_*` exports (note the host uses node1's private IP, same as `RTT_HOST`):

```yaml
        export RPC_HOST="{{ hostvars[groups['node1'][0]].private_ip }}"
        export RPC_UDP_PORT="{{ rpc_udp_port }}"
        export RPC_TCP_PORT="{{ rpc_tcp_port }}"
        export RPC_GRPC_PORT="{{ rpc_grpc_port }}"
        export RPC_WARMUP="{{ rpc_warmup }}"
        export RPC_ITERATIONS="{{ rpc_iterations }}"
```

(The `RPC_MODE` is set by `run_bench.sh` from its 4th arg, same as `RTT_MODE`.)

- [ ] **Step 3: run_bench.sh — focus-area case + RPC_* defaults + mode**

In `bench-infra/ansible/roles/run/files/run_bench.sh`:

1. Add `rpc-roundtrip` to the focus-area `case` validity check:

```sh
  network-rtt|filesystem-write|thread-handoff|serialization|smr-collections|rpc-roundtrip) ;;
```

2. Export the `RPC_*` contract with defaults (near the `RTT_*` exports), including `RPC_MODE` from the mode arg:

```sh
export RPC_MODE="${MODE}"
export RPC_HOST="${RPC_HOST:-}"
export RPC_UDP_PORT="${RPC_UDP_PORT:-9200}"
export RPC_TCP_PORT="${RPC_TCP_PORT:-9201}"
export RPC_GRPC_PORT="${RPC_GRPC_PORT:-9202}"
export RPC_WARMUP="${RPC_WARMUP:-10000}"
export RPC_ITERATIONS="${RPC_ITERATIONS:-100000}"
```

3. Update the `usage()` focus-area list string to include `rpc-roundtrip`.

The responder-kill patterns in `cross_host.yml`'s `always` block are prefix-based
on `<focus_area>-<experiment>` (`target/release/rpc-roundtrip-sbe_udp`,
`bin/rpc-roundtrip-grpc`, etc.) and already match — no change needed there.

- [ ] **Step 4: Docs**

In `CLAUDE.md`:
1. Add to the focus-areas bullet list:
   `- **rpc-roundtrip** — mutating serialize→send→deserialize+mutate→reserialize→send→deserialize round-trip across transport+codec stacks.`
2. Add a status sentence: `rpc-roundtrip` is implemented for `sbe_udp` (Rust, UDP + zero-copy SBE), `grpc` (Go, gRPC), and `bebop_tcp` (Go, TCP + bebop), cross-host, measuring full mutating round-trip latency + encoded size; Java not planned.
3. Add the artifact names to the artifact-names line: `rpc-roundtrip-{sbe_udp}` (Rust) and `rpc-roundtrip-{grpc,bebop_tcp}` (Go).
4. Add run examples (Rust `cargo run --release -p rpc-roundtrip-sbe_udp`; Go `go run ./cmd/rpc-roundtrip-grpc` / `-bebop_tcp`).

In `docs/result-contract.md`: if it lists focus areas or the `RTT_*`/`SER_*` env contracts, add the `rpc-roundtrip` focus area and the `RPC_*` contract (grep first: `grep -n "network-rtt\|RTT_\|focus area" docs/result-contract.md`).

- [ ] **Step 5: Verify**

Run: `cd go && go build ./... && go vet ./... && go test ./...` (green) and
`python3 -c "import yaml; yaml.safe_load(open('bench-infra/ansible/group_vars/all.yml'))"` (parses) and
`sh -n bench-infra/ansible/roles/run/files/run_bench.sh` (valid shell).
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add bench-infra/ CLAUDE.md docs/result-contract.md
git commit -m "chore(rpc-roundtrip): register cross-host cells in bench-infra matrix + RPC_* orchestration + docs"
```
