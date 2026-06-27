# thread-handoff Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the `thread-handoff` stub into a real focus area in Rust, Go, and Java — four artifacts (`spin`, `condvar`, `channel`, `ring`) measuring in-process thread-to-thread handoff cost via two-thread ping-pong.

**Architecture:** The in-process mirror of `network-rtt`. A *timer* thread ping-pongs a token with a *responder* thread; the timer records round-trip latency. Each language's shared bench library gains a `handoff` module (config + warmup/timed loop + emission); each artifact is a thin `main` that builds its transport, spawns the responder, and drives the loop. `spin`/`condvar`/`channel` emit `handoff_rtt_{p50,p99,mean}` (ns); `ring` is a pipelined SPSC ring emitting `handoff_throughput` (ops_per_sec).

**Tech Stack:** Rust 1.96 (edition 2024, Cargo workspace, std-only), Go 1.22 (single module, std-only), Java 21 (Gradle 8.10.2 via wrapper, JUnit 5, std-only).

## Global Constraints

- **stdout is results-only** — every diagnostic/log/error goes to **stderr**; result lines follow `docs/result-contract.md`.
- **Token is a constant non-zero `u64`/`uint64`/`long` (value `1`)** — handoff cost is synchronization, not payload; value `1` doubles as the "slot full" sentinel for `spin` (`0` = empty). No payload-size config.
- **`TH_` env contract:** `TH_WARMUP` (default `10000`), `TH_ITERATIONS` (default `100000`), `TH_RING_CAP` (default `1024`). All four artifacts parse all three (uniform config); only `ring` consumes `TH_RING_CAP`. Invalid/non-positive → stderr message + non-zero exit. No `TH_DIR`.
- **Std-only, no new dependencies** in any language. No CPU pinning. Requires a **≥2-core host** (documented); Go uses default `GOMAXPROCS` (= `NumCPU`).
- **Responder thread count = `TH_WARMUP + TH_ITERATIONS`** (it services every warmup + measured round trip). The timer drives exactly that many `round_trip` calls via the shared measure loop.
- **`Stats` is reused unchanged** in every language (the percentile/mean shared with `network-rtt`/`filesystem-write`).
- **Latency reported as round-trip** (`handoff_rtt_*`), no `/2`. `samples = TH_ITERATIONS` for every line.
- Keep Rust **clippy- and rustfmt-clean** (`cargo clippy --all-targets`, `cargo fmt --check`); keep Go `go vet`-clean; keep `./gradlew build` green.

---

## Task 1 (Rust): `handoff` module in `bench-common`

**Files:**
- Create: `rust/bench-common/src/handoff.rs`
- Modify: `rust/bench-common/src/lib.rs` (register the module)

**Interfaces:**
- Consumes: `crate::result::{emit, emit_float}` (signature `(focus_area, experiment, metric, value, unit, samples)`); `crate::stats::{percentile, mean}`.
- Produces:
  - `bench_common::handoff::FOCUS_AREA: &str` = `"thread-handoff"`
  - `HandoffConfig { warmup: usize, iterations: usize, ring_cap: usize }` + `HandoffConfig::from_env() -> Result<HandoffConfig, String>`
  - `measure<F: FnMut()>(cfg: &HandoffConfig, round_trip: F) -> Vec<u64>`
  - `emit_handoff(experiment: &str, samples: &[u64])`
  - `emit_handoff_throughput(experiment: &str, ops_per_sec: f64, samples: usize)`

- [ ] **Step 1: Register the module** in `rust/bench-common/src/lib.rs` — add after the `pub mod fswrite;` line:

```rust
pub mod handoff;
```

- [ ] **Step 2: Write the failing test** — create `rust/bench-common/src/handoff.rs` with the full module below (it includes its own unit test at the bottom):

```rust
//! thread-handoff focus area: `TH_*` config, the warmup + timed round-trip
//! loop, and result emission (three `handoff_rtt_*` latency lines, or one
//! `handoff_throughput` line). Reuses `stats`. Std-only.

use std::env;
use std::time::Instant;

use crate::result;
use crate::stats;

/// Focus area for every thread-handoff experiment.
pub const FOCUS_AREA: &str = "thread-handoff";

/// Parsed, validated thread-handoff configuration (`TH_*`).
#[derive(Debug, Clone)]
pub struct HandoffConfig {
    /// Discarded warmup round-trips / handoffs.
    pub warmup: usize,
    /// Measured round-trips / handoffs (== sample count).
    pub iterations: usize,
    /// Ring capacity (the `ring` experiment only).
    pub ring_cap: usize,
}

impl HandoffConfig {
    /// Read configuration from `TH_*`, applying defaults and validating.
    pub fn from_env() -> Result<HandoffConfig, String> {
        Ok(HandoffConfig {
            warmup: parse_positive("TH_WARMUP", 10_000)?,
            iterations: parse_positive("TH_ITERATIONS", 100_000)?,
            ring_cap: parse_positive("TH_RING_CAP", 1024)?,
        })
    }
}

fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let value: usize = raw.trim().parse().map_err(|_| {
                format!("{name}: invalid value {raw:?} (expected a positive integer)")
            })?;
            if value == 0 {
                return Err(format!("{name}: must be positive, got 0"));
            }
            Ok(value)
        }
    }
}

/// Run `cfg.warmup` discarded round trips, then time `cfg.iterations` round
/// trips into a pre-allocated buffer, returning one elapsed-ns sample each.
/// `round_trip` performs exactly one ping-pong (infallible — in-process).
pub fn measure<F>(cfg: &HandoffConfig, mut round_trip: F) -> Vec<u64>
where
    F: FnMut(),
{
    for _ in 0..cfg.warmup {
        round_trip();
    }
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip();
        *slot = start.elapsed().as_nanos() as u64;
    }
    samples
}

/// Sort the round-trip samples and emit the three `handoff_rtt_*` lines.
pub fn emit_handoff(experiment: &str, samples: &[u64]) {
    let n = samples.len();
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    result::emit(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_p50",
        stats::percentile(&sorted, 50.0),
        "ns",
        n,
    );
    result::emit(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_p99",
        stats::percentile(&sorted, 99.0),
        "ns",
        n,
    );
    result::emit_float(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_mean",
        stats::mean(samples),
        "ns",
        n,
    );
}

/// Emit the single `handoff_throughput` line (the `ring` experiment).
pub fn emit_handoff_throughput(experiment: &str, ops_per_sec: f64, samples: usize) {
    result::emit_float(
        FOCUS_AREA,
        experiment,
        "handoff_throughput",
        ops_per_sec,
        "ops_per_sec",
        samples,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_runs_warmup_plus_iterations_and_returns_iterations_samples() {
        let cfg = HandoffConfig {
            warmup: 3,
            iterations: 5,
            ring_cap: 16,
        };
        let mut calls = 0usize;
        let samples = measure(&cfg, || calls += 1);
        assert_eq!(samples.len(), 5, "one sample per measured iteration");
        assert_eq!(calls, 8, "warmup (3) + iterations (5) round-trip calls");
    }
}
```

- [ ] **Step 3: Run the test to verify it passes** (it is deterministic):

Run: `cd rust && cargo test -p bench-common handoff`
Expected: PASS (`measure_runs_warmup_plus_iterations_and_returns_iterations_samples`).

- [ ] **Step 4: Verify the workspace still builds clippy/fmt-clean:**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diff.

- [ ] **Step 5: Commit**

```bash
git add rust/bench-common/src/handoff.rs rust/bench-common/src/lib.rs
git commit -m "rust(thread-handoff): add bench-common handoff module (config, measure, emit)"
```

---

## Task 2 (Rust): `thread-handoff-spin` artifact (replaces the stub)

**Files:**
- Delete: `rust/thread-handoff/Cargo.toml`, `rust/thread-handoff/src/main.rs`
- Create: `rust/thread-handoff/spin/Cargo.toml`, `rust/thread-handoff/spin/src/main.rs`
- Modify: `rust/Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `bench_common::handoff::{HandoffConfig, measure, emit_handoff}`.
- Produces: binary `thread-handoff-spin`.

- [ ] **Step 1: Remove the old single-crate stub**

```bash
git rm rust/thread-handoff/Cargo.toml rust/thread-handoff/src/main.rs
```

- [ ] **Step 2: Update workspace members** in `rust/Cargo.toml` — replace the line `    "thread-handoff",` with:

```toml
    "thread-handoff/spin",
```

- [ ] **Step 3: Create the crate manifest** `rust/thread-handoff/spin/Cargo.toml`:

```toml
[package]
name = "thread-handoff-spin"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "thread-handoff-spin"
path = "src/main.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

- [ ] **Step 4: Create** `rust/thread-handoff/spin/src/main.rs`:

```rust
//! thread-handoff **spin** experiment (Rust): single-slot atomic handoff,
//! busy-wait. Lowest latency, burns a core. Emits three `handoff_rtt_*` lines.
//!
//! Two single-slot mailboxes carry a non-zero token; `0` means empty.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "spin";

struct Slots {
    req: AtomicU64,  // timer -> responder (0 = empty)
    resp: AtomicU64, // responder -> timer (0 = empty)
}

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    let slots = Arc::new(Slots {
        req: AtomicU64::new(0),
        resp: AtomicU64::new(0),
    });

    let responder = {
        let slots = Arc::clone(&slots);
        thread::spawn(move || {
            for _ in 0..total {
                while slots.req.load(Ordering::Acquire) == 0 {
                    std::hint::spin_loop();
                }
                slots.req.store(0, Ordering::Relaxed);
                slots.resp.store(1, Ordering::Release);
            }
        })
    };

    let samples = handoff::measure(&cfg, || {
        slots.req.store(1, Ordering::Release);
        while slots.resp.load(Ordering::Acquire) == 0 {
            std::hint::spin_loop();
        }
        slots.resp.store(0, Ordering::Relaxed);
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
```

- [ ] **Step 5: Build and smoke-run**

Run: `cd rust && cargo build --release -p thread-handoff-spin && TH_WARMUP=100 TH_ITERATIONS=2000 cargo run --release -p thread-handoff-spin`
Expected: exactly three stdout lines, e.g.
`{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":<int>,"unit":"ns","samples":2000}` plus `handoff_rtt_p99` and `handoff_rtt_mean`. `p99 >= p50`.

- [ ] **Step 6: clippy/fmt clean**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diff.

- [ ] **Step 7: Commit**

```bash
git add rust/Cargo.toml rust/thread-handoff/spin/
git commit -m "rust(thread-handoff): spin experiment (atomic single-slot busy-wait)"
```

---

## Task 3 (Rust): `thread-handoff-condvar` artifact

**Files:**
- Create: `rust/thread-handoff/condvar/Cargo.toml`, `rust/thread-handoff/condvar/src/main.rs`
- Modify: `rust/Cargo.toml` (add member)

**Interfaces:**
- Consumes: `bench_common::handoff::{HandoffConfig, measure, emit_handoff}`.
- Produces: binary `thread-handoff-condvar`.

- [ ] **Step 1: Add workspace member** in `rust/Cargo.toml` — after `    "thread-handoff/spin",` add:

```toml
    "thread-handoff/condvar",
```

- [ ] **Step 2: Create** `rust/thread-handoff/condvar/Cargo.toml` (same as spin with name `thread-handoff-condvar`):

```toml
[package]
name = "thread-handoff-condvar"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "thread-handoff-condvar"
path = "src/main.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

- [ ] **Step 3: Create** `rust/thread-handoff/condvar/src/main.rs`:

```rust
//! thread-handoff **condvar** experiment (Rust): mutex + condition-variable
//! rendezvous. Isolates the park/unpark + signal cost. Three `handoff_rtt_*`.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "condvar";

/// A one-slot mutex+condvar mailbox carrying a single token.
struct Mailbox {
    slot: Mutex<Option<u64>>,
    cv: Condvar,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox {
            slot: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    fn send(&self, v: u64) {
        let mut g = self.slot.lock().unwrap();
        *g = Some(v);
        drop(g);
        self.cv.notify_one();
    }

    fn recv(&self) -> u64 {
        let mut g = self.slot.lock().unwrap();
        loop {
            if let Some(v) = g.take() {
                return v;
            }
            g = self.cv.wait(g).unwrap();
        }
    }
}

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    let req = Arc::new(Mailbox::new());
    let resp = Arc::new(Mailbox::new());

    let responder = {
        let (req, resp) = (Arc::clone(&req), Arc::clone(&resp));
        thread::spawn(move || {
            for _ in 0..total {
                let v = req.recv();
                resp.send(v);
            }
        })
    };

    let samples = handoff::measure(&cfg, || {
        req.send(1);
        let _ = resp.recv();
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
```

- [ ] **Step 4: Build and smoke-run**

Run: `cd rust && TH_WARMUP=100 TH_ITERATIONS=2000 cargo run --release -p thread-handoff-condvar`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 5: clippy/fmt clean**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diff.

- [ ] **Step 6: Commit**

```bash
git add rust/Cargo.toml rust/thread-handoff/condvar/
git commit -m "rust(thread-handoff): condvar experiment (mutex+condvar rendezvous)"
```

---

## Task 4 (Rust): `thread-handoff-channel` artifact

**Files:**
- Create: `rust/thread-handoff/channel/Cargo.toml`, `rust/thread-handoff/channel/src/main.rs`
- Modify: `rust/Cargo.toml` (add member)

**Interfaces:**
- Consumes: `bench_common::handoff::{HandoffConfig, measure, emit_handoff}`.
- Produces: binary `thread-handoff-channel`.

- [ ] **Step 1: Add workspace member** in `rust/Cargo.toml` — after `    "thread-handoff/condvar",` add:

```toml
    "thread-handoff/channel",
```

- [ ] **Step 2: Create** `rust/thread-handoff/channel/Cargo.toml` (name `thread-handoff-channel`):

```toml
[package]
name = "thread-handoff-channel"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "thread-handoff-channel"
path = "src/main.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

- [ ] **Step 3: Create** `rust/thread-handoff/channel/src/main.rs`:

```rust
//! thread-handoff **channel** experiment (Rust): a std rendezvous
//! `sync_channel(0)` in each direction — the idiomatic blocking-queue handoff.
//! Three `handoff_rtt_*` lines.

use std::sync::mpsc;
use std::thread;

use bench_common::handoff::{self, HandoffConfig};

const EXPERIMENT: &str = "channel";

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    // Rendezvous (capacity 0): send blocks until the receiver takes the value.
    let (req_tx, req_rx) = mpsc::sync_channel::<u64>(0);
    let (resp_tx, resp_rx) = mpsc::sync_channel::<u64>(0);

    let responder = thread::spawn(move || {
        for _ in 0..total {
            let v = match req_rx.recv() {
                Ok(v) => v,
                Err(_) => return,
            };
            if resp_tx.send(v).is_err() {
                return;
            }
        }
    });

    let samples = handoff::measure(&cfg, || {
        req_tx.send(1).expect("responder gone");
        let _ = resp_rx.recv().expect("responder gone");
    });

    if responder.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: responder thread panicked");
        std::process::exit(1);
    }

    handoff::emit_handoff(EXPERIMENT, &samples);
}
```

- [ ] **Step 4: Build and smoke-run**

Run: `cd rust && TH_WARMUP=100 TH_ITERATIONS=2000 cargo run --release -p thread-handoff-channel`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 5: clippy/fmt clean**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diff.

- [ ] **Step 6: Commit**

```bash
git add rust/Cargo.toml rust/thread-handoff/channel/
git commit -m "rust(thread-handoff): channel experiment (std sync_channel rendezvous)"
```

---

## Task 5 (Rust): `thread-handoff-ring` artifact (SPSC + throughput)

**Files:**
- Create: `rust/thread-handoff/ring/Cargo.toml`, `rust/thread-handoff/ring/src/spsc.rs`, `rust/thread-handoff/ring/src/main.rs`
- Modify: `rust/Cargo.toml` (add member)

**Interfaces:**
- Consumes: `bench_common::handoff::{HandoffConfig, emit_handoff_throughput}`.
- Produces: binary `thread-handoff-ring`; `spsc::Spsc` with `new(cap) -> Spsc`, `push(&self, u64)`, `pop(&self) -> u64`, `consumed(&self) -> usize`.

- [ ] **Step 1: Add workspace member** in `rust/Cargo.toml` — after `    "thread-handoff/channel",` add:

```toml
    "thread-handoff/ring",
```

- [ ] **Step 2: Create** `rust/thread-handoff/ring/Cargo.toml` (name `thread-handoff-ring`):

```toml
[package]
name = "thread-handoff-ring"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "thread-handoff-ring"
path = "src/main.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

- [ ] **Step 3: Write the failing SPSC test** — create `rust/thread-handoff/ring/src/spsc.rs` with the struct and its test:

```rust
//! A bounded single-producer single-consumer ring of `u64` tokens with
//! busy-wait (no parking). `head`/`tail` are monotonic counters; `head` doubles
//! as the consumed-count. Safe: each slot is an `AtomicU64`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct Spsc {
    buf: Box<[AtomicU64]>,
    cap: usize,
    head: AtomicUsize, // total popped (consumer writes)
    tail: AtomicUsize, // total pushed (producer writes)
}

impl Spsc {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "ring capacity must be positive");
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Spsc {
            buf,
            cap,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Producer: push one token, busy-waiting while the ring is full.
    pub fn push(&self, v: u64) {
        let tail = self.tail.load(Ordering::Relaxed);
        while tail - self.head.load(Ordering::Acquire) == self.cap {
            std::hint::spin_loop();
        }
        self.buf[tail % self.cap].store(v, Ordering::Relaxed);
        self.tail.store(tail + 1, Ordering::Release);
    }

    /// Consumer: pop one token, busy-waiting while the ring is empty.
    pub fn pop(&self) -> u64 {
        let head = self.head.load(Ordering::Relaxed);
        while head == self.tail.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        let v = self.buf[head % self.cap].load(Ordering::Relaxed);
        self.head.store(head + 1, Ordering::Release);
        v
    }

    /// Total tokens popped so far (consumer progress).
    pub fn consumed(&self) -> usize {
        self.head.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn spsc_preserves_order_and_count() {
        let n = 100_000usize;
        let ring = Arc::new(Spsc::new(64));
        let consumer = {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                let mut got = Vec::with_capacity(n);
                for _ in 0..n {
                    got.push(ring.pop());
                }
                got
            })
        };
        for i in 0..n {
            ring.push(i as u64);
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "token {i} out of order");
        }
        assert_eq!(ring.consumed(), n);
    }
}
```

- [ ] **Step 4: Run the SPSC test**

Run: `cd rust && cargo test -p thread-handoff-ring`
Expected: PASS (`spsc_preserves_order_and_count`).

- [ ] **Step 5: Create** `rust/thread-handoff/ring/src/main.rs`:

```rust
//! thread-handoff **ring** experiment (Rust): bounded SPSC ring, busy-wait,
//! pipelined depth N. Emits one `handoff_throughput` line.

mod spsc;

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use bench_common::handoff::{self, HandoffConfig};
use spsc::Spsc;

const EXPERIMENT: &str = "ring";

fn main() {
    let cfg = match HandoffConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("thread-handoff-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let total = cfg.warmup + cfg.iterations;

    let ring = Arc::new(Spsc::new(cfg.ring_cap));

    let consumer = {
        let ring = Arc::clone(&ring);
        thread::spawn(move || {
            for _ in 0..total {
                let _ = ring.pop();
            }
        })
    };

    // Warmup pushes, then a drain barrier so timing excludes warmup.
    for _ in 0..cfg.warmup {
        ring.push(1);
    }
    while ring.consumed() < cfg.warmup {
        std::hint::spin_loop();
    }

    let t_start = Instant::now();
    for _ in 0..cfg.iterations {
        ring.push(1);
    }
    while ring.consumed() < total {
        std::hint::spin_loop();
    }
    let elapsed = t_start.elapsed();

    if consumer.join().is_err() {
        eprintln!("thread-handoff-{EXPERIMENT}: consumer thread panicked");
        std::process::exit(1);
    }

    let throughput = cfg.iterations as f64 / elapsed.as_secs_f64();
    handoff::emit_handoff_throughput(EXPERIMENT, throughput, cfg.iterations);
}
```

- [ ] **Step 6: Build and smoke-run**

Run: `cd rust && TH_WARMUP=1000 TH_ITERATIONS=200000 cargo run --release -p thread-handoff-ring`
Expected: one line, `metric":"handoff_throughput"`, `unit":"ops_per_sec"`, `value` > 0, `samples:200000`.

- [ ] **Step 7: clippy/fmt clean**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diff.

- [ ] **Step 8: Commit**

```bash
git add rust/Cargo.toml rust/thread-handoff/ring/
git commit -m "rust(thread-handoff): ring experiment (SPSC busy-wait, pipelined throughput)"
```

---

## Task 6 (Rust): full-workspace verification

**Files:** none (verification only).

- [ ] **Step 1: Full build + test + lint**

Run: `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check`
Expected: all pass; binaries `thread-handoff-{spin,condvar,channel,ring}` built.

- [ ] **Step 2: Smoke all four and eyeball the ladder**

Run:
```bash
cd rust
for e in spin condvar channel; do TH_WARMUP=500 TH_ITERATIONS=20000 cargo run -q --release -p thread-handoff-$e; done
TH_WARMUP=1000 TH_ITERATIONS=500000 cargo run -q --release -p thread-handoff-ring
```
Expected: 3+3+3 latency lines then 1 throughput line. Sanity: `spin` `handoff_rtt_p50` ≤ `condvar` ≤ `channel` (scheduler noise allowed; gross inversions are a red flag), every `p99 >= p50`, ring throughput > 0.

- [ ] **Step 3: Commit** (only if Step 1/2 required any fix; otherwise skip)

```bash
git commit -am "rust(thread-handoff): verification fixups"
```

---

## Task 7 (Go): `handoff` config + measure + emit in `internal/bench`

**Files:**
- Create: `go/internal/bench/handoff.go`, `go/internal/bench/handoff_test.go`

**Interfaces:**
- Consumes: `positiveEnv` (config.go), `Percentile`/`Mean` (stats.go), `Emit`/`Result` (result.go) — all package `bench`.
- Produces:
  - `THFocusArea = "thread-handoff"`
  - `HandoffConfig { Warmup, Iterations, RingCap int }` + `LoadHandoffConfig() (HandoffConfig, error)`
  - `HandoffRoundTrip = func()`
  - `MeasureHandoff(cfg HandoffConfig, rt HandoffRoundTrip) []int64`
  - `EmitHandoff(experiment string, samples []int64)`
  - `EmitHandoffThroughput(experiment string, opsPerSec float64, samples int64)`

- [ ] **Step 1: Create** `go/internal/bench/handoff.go`:

```go
package bench

import (
	"sort"
	"time"
)

// THFocusArea is the focus area for every thread-handoff experiment.
const THFocusArea = "thread-handoff"

// HandoffConfig holds the thread-handoff parameters from the TH_* env vars.
type HandoffConfig struct {
	Warmup     int
	Iterations int
	RingCap    int
}

// LoadHandoffConfig reads TH_WARMUP, TH_ITERATIONS and TH_RING_CAP, applying
// defaults. Invalid or non-positive values yield an error.
func LoadHandoffConfig() (HandoffConfig, error) {
	warmup, err := positiveEnv("TH_WARMUP", 10000)
	if err != nil {
		return HandoffConfig{}, err
	}
	iterations, err := positiveEnv("TH_ITERATIONS", 100000)
	if err != nil {
		return HandoffConfig{}, err
	}
	ringCap, err := positiveEnv("TH_RING_CAP", 1024)
	if err != nil {
		return HandoffConfig{}, err
	}
	return HandoffConfig{Warmup: warmup, Iterations: iterations, RingCap: ringCap}, nil
}

// HandoffRoundTrip performs exactly one ping-pong handoff (send a token, wait
// for its echo). Infallible — it runs entirely in-process.
type HandoffRoundTrip func()

// MeasureHandoff runs cfg.Warmup discarded round trips, then cfg.Iterations
// timed round trips into a pre-allocated buffer (ns). Mirrors Measure but for
// the infallible in-process handoff.
func MeasureHandoff(cfg HandoffConfig, rt HandoffRoundTrip) []int64 {
	for i := 0; i < cfg.Warmup; i++ {
		rt()
	}
	samples := make([]int64, cfg.Iterations)
	for i := 0; i < cfg.Iterations; i++ {
		start := time.Now()
		rt()
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples
}

// EmitHandoff sorts samples and emits the handoff_rtt_p50/p99/mean lines (ns).
func EmitHandoff(experiment string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}

// EmitHandoffThroughput emits the single handoff_throughput line (ops/sec).
func EmitHandoffThroughput(experiment string, opsPerSec float64, samples int64) {
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_throughput", Value: opsPerSec, Unit: "ops_per_sec", Samples: samples})
}
```

- [ ] **Step 2: Write the failing test** — create `go/internal/bench/handoff_test.go`:

```go
package bench

import "testing"

func TestMeasureHandoffSampleCountAndCalls(t *testing.T) {
	cfg := HandoffConfig{Warmup: 3, Iterations: 5, RingCap: 16}
	calls := 0
	samples := MeasureHandoff(cfg, func() { calls++ })
	if len(samples) != 5 {
		t.Fatalf("want 5 samples, got %d", len(samples))
	}
	if calls != 8 {
		t.Fatalf("want 8 calls (warmup+iterations), got %d", calls)
	}
}
```

- [ ] **Step 3: Run the test**

Run: `cd go && go test ./internal/bench/ -run TestMeasureHandoff -v`
Expected: PASS.

- [ ] **Step 4: Vet**

Run: `cd go && go vet ./...`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add go/internal/bench/handoff.go go/internal/bench/handoff_test.go
git commit -m "go(thread-handoff): add bench handoff config, measure, emit"
```

---

## Task 8 (Go): `thread-handoff-spin` command (replaces the stub)

**Files:**
- Delete: `go/cmd/thread-handoff/main.go` (and the now-empty `go/cmd/thread-handoff/` dir)
- Create: `go/cmd/thread-handoff-spin/main.go`

**Interfaces:**
- Consumes: `bench.{LoadHandoffConfig, MeasureHandoff, EmitHandoff, Fatalf}`.
- Produces: command `thread-handoff-spin`.

- [ ] **Step 1: Remove the stub command**

```bash
git rm go/cmd/thread-handoff/main.go
```

- [ ] **Step 2: Create** `go/cmd/thread-handoff-spin/main.go`:

```go
// thread-handoff-spin (Go): single-slot atomic handoff, busy-wait. Emits three
// handoff_rtt_* lines. See the thread-handoff design spec.
package main

import (
	"sync/atomic"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "spin"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	var req, resp atomic.Uint64 // 0 == empty; token is a non-zero 1

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			for req.Load() == 0 {
			}
			req.Store(0)
			resp.Store(1)
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req.Store(1)
		for resp.Load() == 0 {
		}
		resp.Store(0)
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
```

- [ ] **Step 3: Build and smoke-run**

Run: `cd go && go build ./... && TH_WARMUP=100 TH_ITERATIONS=2000 go run ./cmd/thread-handoff-spin`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 4: Vet**

Run: `cd go && go vet ./...`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add go/cmd/thread-handoff-spin/
git commit -m "go(thread-handoff): spin experiment (atomic single-slot busy-wait)"
```

---

## Task 9 (Go): `thread-handoff-condvar` command

**Files:**
- Create: `go/cmd/thread-handoff-condvar/main.go`

**Interfaces:**
- Consumes: `bench.{LoadHandoffConfig, MeasureHandoff, EmitHandoff, Fatalf}`.
- Produces: command `thread-handoff-condvar`.

- [ ] **Step 1: Create** `go/cmd/thread-handoff-condvar/main.go`:

```go
// thread-handoff-condvar (Go): sync.Cond rendezvous. Isolates park/unpark +
// signal cost. Emits three handoff_rtt_* lines.
package main

import (
	"sync"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "condvar"

// mailbox is a one-slot rendezvous carrying a single token.
type mailbox struct {
	mu   sync.Mutex
	cond *sync.Cond
	val  uint64
	full bool
}

func newMailbox() *mailbox {
	m := &mailbox{}
	m.cond = sync.NewCond(&m.mu)
	return m
}

func (m *mailbox) send(v uint64) {
	m.mu.Lock()
	m.val = v
	m.full = true
	m.mu.Unlock()
	m.cond.Signal()
}

func (m *mailbox) recv() uint64 {
	m.mu.Lock()
	for !m.full {
		m.cond.Wait()
	}
	v := m.val
	m.full = false
	m.mu.Unlock()
	return v
}

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	req, resp := newMailbox(), newMailbox()

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			resp.send(req.recv())
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req.send(1)
		_ = resp.recv()
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
```

- [ ] **Step 2: Build and smoke-run**

Run: `cd go && TH_WARMUP=100 TH_ITERATIONS=2000 go run ./cmd/thread-handoff-condvar`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 3: Vet**

Run: `cd go && go vet ./...`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add go/cmd/thread-handoff-condvar/
git commit -m "go(thread-handoff): condvar experiment (sync.Cond rendezvous)"
```

---

## Task 10 (Go): `thread-handoff-channel` command

**Files:**
- Create: `go/cmd/thread-handoff-channel/main.go`

**Interfaces:**
- Consumes: `bench.{LoadHandoffConfig, MeasureHandoff, EmitHandoff, Fatalf}`.
- Produces: command `thread-handoff-channel`.

- [ ] **Step 1: Create** `go/cmd/thread-handoff-channel/main.go`:

```go
// thread-handoff-channel (Go): unbuffered chan rendezvous in each direction —
// the idiomatic blocking-queue handoff. Emits three handoff_rtt_* lines.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

const experiment = "channel"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	req := make(chan uint64)  // unbuffered == rendezvous
	resp := make(chan uint64)

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			v := <-req
			resp <- v
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req <- 1
		<-resp
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
```

- [ ] **Step 2: Build and smoke-run**

Run: `cd go && TH_WARMUP=100 TH_ITERATIONS=2000 go run ./cmd/thread-handoff-channel`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 3: Vet**

Run: `cd go && go vet ./...`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add go/cmd/thread-handoff-channel/
git commit -m "go(thread-handoff): channel experiment (unbuffered chan rendezvous)"
```

---

## Task 11 (Go): `thread-handoff-ring` command (SPSC + throughput)

**Files:**
- Create: `go/cmd/thread-handoff-ring/spsc.go`, `go/cmd/thread-handoff-ring/spsc_test.go`, `go/cmd/thread-handoff-ring/main.go`

**Interfaces:**
- Consumes: `bench.{LoadHandoffConfig, EmitHandoffThroughput, Fatalf}`.
- Produces: command `thread-handoff-ring`; `spsc` with `newSPSC(int) *spsc`, `push(uint64)`, `pop() uint64`, `consumed() uint64`.

- [ ] **Step 1: Write the SPSC** — create `go/cmd/thread-handoff-ring/spsc.go`:

```go
package main

import "sync/atomic"

// spsc is a bounded single-producer single-consumer ring of uint64 tokens with
// busy-wait (no parking). head/tail are monotonic; head doubles as the consumed
// count. Atomic Load/Store establish the happens-before for the plain buf slots.
type spsc struct {
	buf  []uint64
	cap  uint64
	head atomic.Uint64 // total popped (consumer)
	tail atomic.Uint64 // total pushed (producer)
}

func newSPSC(capacity int) *spsc {
	return &spsc{buf: make([]uint64, capacity), cap: uint64(capacity)}
}

func (s *spsc) push(v uint64) {
	tail := s.tail.Load()
	for tail-s.head.Load() == s.cap {
	}
	s.buf[tail%s.cap] = v
	s.tail.Store(tail + 1)
}

func (s *spsc) pop() uint64 {
	head := s.head.Load()
	for head == s.tail.Load() {
	}
	v := s.buf[head%s.cap]
	s.head.Store(head + 1)
	return v
}

func (s *spsc) consumed() uint64 {
	return s.head.Load()
}
```

- [ ] **Step 2: Write the failing test** — create `go/cmd/thread-handoff-ring/spsc_test.go`:

```go
package main

import (
	"sync"
	"testing"
)

func TestSPSCPreservesOrderAndCount(t *testing.T) {
	const n = 100000
	ring := newSPSC(64)
	var got []uint64
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		got = make([]uint64, 0, n)
		for i := 0; i < n; i++ {
			got = append(got, ring.pop())
		}
	}()
	for i := 0; i < n; i++ {
		ring.push(uint64(i))
	}
	wg.Wait()
	if len(got) != n {
		t.Fatalf("want %d tokens, got %d", n, len(got))
	}
	for i := 0; i < n; i++ {
		if got[i] != uint64(i) {
			t.Fatalf("token %d: want %d, got %d", i, i, got[i])
		}
	}
	if ring.consumed() != n {
		t.Fatalf("want consumed %d, got %d", n, ring.consumed())
	}
}
```

- [ ] **Step 3: Run the test**

Run: `cd go && go test ./cmd/thread-handoff-ring/ -run TestSPSC -v`
Expected: PASS.

- [ ] **Step 4: Create** `go/cmd/thread-handoff-ring/main.go`:

```go
// thread-handoff-ring (Go): bounded SPSC ring, busy-wait, pipelined depth N.
// Emits one handoff_throughput line.
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "ring"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := uint64(cfg.Warmup + cfg.Iterations)

	ring := newSPSC(cfg.RingCap)

	done := make(chan struct{})
	go func() {
		for i := uint64(0); i < total; i++ {
			ring.pop()
		}
		close(done)
	}()

	// Warmup pushes, then a drain barrier so timing excludes warmup.
	for i := 0; i < cfg.Warmup; i++ {
		ring.push(1)
	}
	for ring.consumed() < uint64(cfg.Warmup) {
	}

	start := time.Now()
	for i := 0; i < cfg.Iterations; i++ {
		ring.push(1)
	}
	for ring.consumed() < total {
	}
	elapsed := time.Since(start)

	<-done
	throughput := float64(cfg.Iterations) / elapsed.Seconds()
	bench.EmitHandoffThroughput(experiment, throughput, int64(cfg.Iterations))
}
```

- [ ] **Step 5: Build and smoke-run**

Run: `cd go && go build ./... && TH_WARMUP=1000 TH_ITERATIONS=200000 go run ./cmd/thread-handoff-ring`
Expected: one `handoff_throughput` line, `unit":"ops_per_sec"`, `value` > 0, `samples:200000`.

- [ ] **Step 6: Vet**

Run: `cd go && go vet ./...`
Expected: no output.

- [ ] **Step 7: Commit**

```bash
git add go/cmd/thread-handoff-ring/
git commit -m "go(thread-handoff): ring experiment (SPSC busy-wait, pipelined throughput)"
```

---

## Task 12 (Go): full-module verification

**Files:** none (verification only).

- [ ] **Step 1: Build + vet + test**

Run: `cd go && go build ./... && go vet ./... && go test ./...`
Expected: all pass.

- [ ] **Step 2: Smoke all four and eyeball the ladder**

Run:
```bash
cd go
for e in spin condvar channel; do TH_WARMUP=500 TH_ITERATIONS=20000 go run ./cmd/thread-handoff-$e; done
TH_WARMUP=1000 TH_ITERATIONS=500000 go run ./cmd/thread-handoff-ring
```
Expected: 3+3+3 latency lines then 1 throughput line; `spin` p50 ≤ `condvar` ≤ `channel` (noise allowed), every `p99 >= p50`, ring throughput > 0.

- [ ] **Step 3: Commit** (only if Step 1/2 required a fix)

```bash
git commit -am "go(thread-handoff): verification fixups"
```

---

## Task 13 (Java): `HandoffConfig` + `Handoff` in `:common`

**Files:**
- Create: `java/common/src/main/java/net/knego/hiperf/common/HandoffConfig.java`
- Create: `java/common/src/main/java/net/knego/hiperf/common/Handoff.java`
- Create: `java/common/src/test/java/net/knego/hiperf/common/HandoffTest.java`

**Interfaces:**
- Consumes: `Env.readPositiveInt(name, def)`; `Stats.percentile`/`Stats.mean`; `Result` record + `emit()`.
- Produces:
  - `HandoffConfig(int warmup, int iterations, int ringCap)` record + `HandoffConfig.fromEnv()`
  - `Handoff.FOCUS_AREA = "thread-handoff"`; `Handoff.RoundTrip` (`void run()`); `Handoff.measure(HandoffConfig, RoundTrip) -> long[]`; `Handoff.emit(String, long[])`; `Handoff.emitThroughput(String, double, long)`.

- [ ] **Step 1: Create** `java/common/src/main/java/net/knego/hiperf/common/HandoffConfig.java`:

```java
package net.knego.hiperf.common;

/** thread-handoff configuration from the {@code TH_*} env vars; positive integers. */
public record HandoffConfig(int warmup, int iterations, int ringCap) {

    public static HandoffConfig fromEnv() {
        return new HandoffConfig(
                Env.readPositiveInt("TH_WARMUP", 10000),
                Env.readPositiveInt("TH_ITERATIONS", 100000),
                Env.readPositiveInt("TH_RING_CAP", 1024));
    }
}
```

- [ ] **Step 2: Create** `java/common/src/main/java/net/knego/hiperf/common/Handoff.java`:

```java
package net.knego.hiperf.common;

import java.util.Arrays;

/**
 * Shared thread-handoff driver: the warmup + timed round-trip loop and result
 * emission. Mirrors {@link Measure} but for the infallible in-process handoff,
 * and adds the throughput emitter for the ring experiment.
 */
public final class Handoff {

    /** Focus area shared by all thread-handoff experiments. */
    public static final String FOCUS_AREA = "thread-handoff";

    private Handoff() {}

    /** A single ping-pong handoff round trip (infallible, in-process). */
    @FunctionalInterface
    public interface RoundTrip {
        void run();
    }

    /**
     * Run {@code cfg.warmup()} discarded round trips, then time
     * {@code cfg.iterations()} round trips into a pre-allocated array (ns).
     */
    public static long[] measure(HandoffConfig cfg, RoundTrip roundTrip) {
        for (int i = 0; i < cfg.warmup(); i++) {
            roundTrip.run();
        }
        long[] samples = new long[cfg.iterations()];
        for (int i = 0; i < cfg.iterations(); i++) {
            long start = System.nanoTime();
            roundTrip.run();
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    /** Sort and emit handoff_rtt_p50/p99/mean (ns) for the experiment. */
    public static void emit(String experiment, long[] samples) {
        Arrays.sort(samples);
        long p50 = Stats.percentile(samples, 50);
        long p99 = Stats.percentile(samples, 99);
        double mean = Stats.mean(samples);
        long n = samples.length;
        new Result(FOCUS_AREA, experiment, "handoff_rtt_p50", p50, "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, "handoff_rtt_p99", p99, "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, "handoff_rtt_mean", mean, "ns", n, "").emit();
    }

    /** Emit the single handoff_throughput (ops_per_sec) line. */
    public static void emitThroughput(String experiment, double opsPerSec, long samples) {
        new Result(FOCUS_AREA, experiment, "handoff_throughput", opsPerSec, "ops_per_sec", samples, "").emit();
    }
}
```

- [ ] **Step 3: Write the failing test** — create `java/common/src/test/java/net/knego/hiperf/common/HandoffTest.java`:

```java
package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class HandoffTest {

    @Test
    void measureRunsWarmupPlusIterationsAndReturnsIterationsSamples() {
        HandoffConfig cfg = new HandoffConfig(3, 5, 16);
        AtomicInteger calls = new AtomicInteger();
        long[] samples = Handoff.measure(cfg, calls::incrementAndGet);
        assertEquals(5, samples.length, "one sample per measured iteration");
        assertEquals(8, calls.get(), "warmup (3) + iterations (5) calls");
    }
}
```

- [ ] **Step 4: Run the test**

Run: `cd java && ./gradlew :common:test --tests 'net.knego.hiperf.common.HandoffTest'`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add java/common/src/main/java/net/knego/hiperf/common/HandoffConfig.java \
        java/common/src/main/java/net/knego/hiperf/common/Handoff.java \
        java/common/src/test/java/net/knego/hiperf/common/HandoffTest.java
git commit -m "java(thread-handoff): add common HandoffConfig + Handoff driver"
```

---

## Task 14 (Java): `:thread-handoff-spin` subproject (replaces the stub)

**Files:**
- Delete: `java/thread-handoff/` (the whole stub subproject: `build.gradle.kts`, `src/`, `thread-handoff.iml`)
- Create: `java/thread-handoff-spin/build.gradle.kts`
- Create: `java/thread-handoff-spin/src/main/java/net/knego/hiperf/threadhandoff/spin/Main.java`
- Modify: `java/settings.gradle.kts`

**Interfaces:**
- Consumes: `net.knego.hiperf.common.{Handoff, HandoffConfig}`.
- Produces: runnable subproject `:thread-handoff-spin`, mainClass `net.knego.hiperf.threadhandoff.spin.Main`.

- [ ] **Step 1: Remove the stub subproject**

```bash
git rm -r java/thread-handoff
```

- [ ] **Step 2: Update** `java/settings.gradle.kts` — replace the `"thread-handoff",` line with the four subprojects:

```kotlin
    "thread-handoff-spin",
    "thread-handoff-condvar",
    "thread-handoff-channel",
    "thread-handoff-ring",
```

- [ ] **Step 3: Create** `java/thread-handoff-spin/build.gradle.kts`:

```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.spin.Main")
}
```

- [ ] **Step 4: Create** `java/thread-handoff-spin/src/main/java/net/knego/hiperf/threadhandoff/spin/Main.java`:

```java
package net.knego.hiperf.threadhandoff.spin;

import java.util.concurrent.atomic.AtomicLong;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / spin (Java): single-slot atomic handoff, busy-wait. Lowest
 * latency, burns a core. Emits three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "spin";

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            AtomicLong req = new AtomicLong(0);  // 0 == empty; token is non-zero 1
            AtomicLong resp = new AtomicLong(0);

            Thread responder = new Thread(() -> {
                for (int i = 0; i < total; i++) {
                    while (req.get() == 0) {
                        Thread.onSpinWait();
                    }
                    req.set(0);
                    resp.set(1);
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
                req.set(1);
                while (resp.get() == 0) {
                    Thread.onSpinWait();
                }
                resp.set(0);
            });

            responder.join();
            Handoff.emit(EXPERIMENT, samples);
        } catch (IllegalArgumentException e) {
            System.err.println("thread-handoff-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            System.err.println("thread-handoff-" + EXPERIMENT + ": interrupted: " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 5: Build and smoke-run**

Run: `cd java && TH_WARMUP=100 TH_ITERATIONS=2000 ./gradlew :thread-handoff-spin:run -q`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 6: Commit**

```bash
git add java/settings.gradle.kts java/thread-handoff-spin/
git commit -m "java(thread-handoff): spin experiment (AtomicLong single-slot busy-wait)"
```

---

## Task 15 (Java): `:thread-handoff-condvar` subproject

**Files:**
- Create: `java/thread-handoff-condvar/build.gradle.kts`
- Create: `java/thread-handoff-condvar/src/main/java/net/knego/hiperf/threadhandoff/condvar/Main.java`

**Interfaces:**
- Consumes: `net.knego.hiperf.common.{Handoff, HandoffConfig}`.
- Produces: runnable subproject `:thread-handoff-condvar`.

(`:thread-handoff-condvar` is already registered in `settings.gradle.kts` from Task 14.)

- [ ] **Step 1: Create** `java/thread-handoff-condvar/build.gradle.kts`:

```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.condvar.Main")
}
```

- [ ] **Step 2: Create** `java/thread-handoff-condvar/src/main/java/net/knego/hiperf/threadhandoff/condvar/Main.java`:

```java
package net.knego.hiperf.threadhandoff.condvar;

import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / condvar (Java): a monitor (synchronized + wait/notify)
 * rendezvous. Isolates park/unpark + signal cost. Three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "condvar";

    /** One-slot monitor mailbox carrying a single long token. */
    static final class Mailbox {
        private long value;
        private boolean full;

        synchronized void send(long v) {
            value = v;
            full = true;
            notify();
        }

        synchronized long recv() {
            while (!full) {
                try {
                    wait();
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new RuntimeException("interrupted while waiting for handoff", e);
                }
            }
            full = false;
            return value;
        }
    }

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            Mailbox req = new Mailbox();
            Mailbox resp = new Mailbox();

            Thread responder = new Thread(() -> {
                for (int i = 0; i < total; i++) {
                    resp.send(req.recv());
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
                req.send(1);
                resp.recv();
            });

            responder.join();
            Handoff.emit(EXPERIMENT, samples);
        } catch (IllegalArgumentException e) {
            System.err.println("thread-handoff-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            System.err.println("thread-handoff-" + EXPERIMENT + ": interrupted: " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 3: Build and smoke-run**

Run: `cd java && TH_WARMUP=100 TH_ITERATIONS=2000 ./gradlew :thread-handoff-condvar:run -q`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 4: Commit**

```bash
git add java/thread-handoff-condvar/
git commit -m "java(thread-handoff): condvar experiment (monitor wait/notify rendezvous)"
```

---

## Task 16 (Java): `:thread-handoff-channel` subproject

**Files:**
- Create: `java/thread-handoff-channel/build.gradle.kts`
- Create: `java/thread-handoff-channel/src/main/java/net/knego/hiperf/threadhandoff/channel/Main.java`

**Interfaces:**
- Consumes: `net.knego.hiperf.common.{Handoff, HandoffConfig}`.
- Produces: runnable subproject `:thread-handoff-channel`.

- [ ] **Step 1: Create** `java/thread-handoff-channel/build.gradle.kts`:

```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.channel.Main")
}
```

- [ ] **Step 2: Create** `java/thread-handoff-channel/src/main/java/net/knego/hiperf/threadhandoff/channel/Main.java`:

```java
package net.knego.hiperf.threadhandoff.channel;

import java.util.concurrent.SynchronousQueue;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / channel (Java): a {@link SynchronousQueue} rendezvous in each
 * direction — the idiomatic blocking-queue handoff. The token is a constant
 * cached {@link Long} (a reused box), so there is no per-handoff allocation.
 * Three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "channel";

    /** Constant cached box (Long.valueOf caches small values) — reused, never re-boxed. */
    private static final Long TOKEN = 1L;

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            SynchronousQueue<Long> req = new SynchronousQueue<>();
            SynchronousQueue<Long> resp = new SynchronousQueue<>();

            Thread responder = new Thread(() -> {
                try {
                    for (int i = 0; i < total; i++) {
                        resp.put(req.take());
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
                try {
                    req.put(TOKEN);
                    resp.take();
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new RuntimeException("interrupted during handoff", e);
                }
            });

            responder.join();
            Handoff.emit(EXPERIMENT, samples);
        } catch (IllegalArgumentException e) {
            System.err.println("thread-handoff-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            System.err.println("thread-handoff-" + EXPERIMENT + ": interrupted: " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 3: Build and smoke-run**

Run: `cd java && TH_WARMUP=100 TH_ITERATIONS=2000 ./gradlew :thread-handoff-channel:run -q`
Expected: three `handoff_rtt_*` lines, `samples:2000`, `p99 >= p50`.

- [ ] **Step 4: Commit**

```bash
git add java/thread-handoff-channel/
git commit -m "java(thread-handoff): channel experiment (SynchronousQueue rendezvous)"
```

---

## Task 17 (Java): `:thread-handoff-ring` subproject (SPSC + throughput)

**Files:**
- Create: `java/thread-handoff-ring/build.gradle.kts`
- Create: `java/thread-handoff-ring/src/main/java/net/knego/hiperf/threadhandoff/ring/Spsc.java`
- Create: `java/thread-handoff-ring/src/main/java/net/knego/hiperf/threadhandoff/ring/Main.java`
- Create: `java/thread-handoff-ring/src/test/java/net/knego/hiperf/threadhandoff/ring/SpscTest.java`

**Interfaces:**
- Consumes: `net.knego.hiperf.common.{Handoff, HandoffConfig}`.
- Produces: runnable subproject `:thread-handoff-ring`; `Spsc` with `Spsc(int cap)`, `push(long)`, `pop() -> long`, `consumed() -> long`.

- [ ] **Step 1: Create** `java/thread-handoff-ring/build.gradle.kts`:

```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.ring.Main")
}
```

- [ ] **Step 2: Write the SPSC** — create `java/thread-handoff-ring/src/main/java/net/knego/hiperf/threadhandoff/ring/Spsc.java`:

```java
package net.knego.hiperf.threadhandoff.ring;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Bounded single-producer single-consumer ring of {@code long} tokens with
 * busy-wait (no parking). {@code head}/{@code tail} are monotonic; {@code head}
 * doubles as the consumed count. The {@link AtomicLong} head/tail (release/
 * acquire via volatile get/set) publish the plain-array slot writes.
 */
public final class Spsc {

    private final long[] buf;
    private final int cap;
    private final AtomicLong head = new AtomicLong(0); // total popped (consumer)
    private final AtomicLong tail = new AtomicLong(0); // total pushed (producer)

    public Spsc(int cap) {
        if (cap <= 0) {
            throw new IllegalArgumentException("ring capacity must be positive");
        }
        this.cap = cap;
        this.buf = new long[cap];
    }

    /** Producer: push one token, busy-waiting while the ring is full. */
    public void push(long v) {
        long t = tail.get();
        while (t - head.get() == cap) {
            Thread.onSpinWait();
        }
        buf[(int) (t % cap)] = v;
        tail.set(t + 1);
    }

    /** Consumer: pop one token, busy-waiting while the ring is empty. */
    public long pop() {
        long h = head.get();
        while (h == tail.get()) {
            Thread.onSpinWait();
        }
        long v = buf[(int) (h % cap)];
        head.set(h + 1);
        return v;
    }

    /** Total tokens popped so far (consumer progress). */
    public long consumed() {
        return head.get();
    }
}
```

- [ ] **Step 3: Write the failing test** — create `java/thread-handoff-ring/src/test/java/net/knego/hiperf/threadhandoff/ring/SpscTest.java`:

```java
package net.knego.hiperf.threadhandoff.ring;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class SpscTest {

    @Test
    void preservesOrderAndCount() throws InterruptedException {
        final int n = 100_000;
        Spsc ring = new Spsc(64);
        long[] got = new long[n];
        Thread consumer = new Thread(() -> {
            for (int i = 0; i < n; i++) {
                got[i] = ring.pop();
            }
        });
        consumer.start();
        for (int i = 0; i < n; i++) {
            ring.push(i);
        }
        consumer.join();
        for (int i = 0; i < n; i++) {
            assertEquals(i, got[i], "token " + i + " out of order");
        }
        assertEquals(n, ring.consumed());
    }
}
```

- [ ] **Step 4: Run the test**

Run: `cd java && ./gradlew :thread-handoff-ring:test`
Expected: PASS (`SpscTest.preservesOrderAndCount`).

- [ ] **Step 5: Create** `java/thread-handoff-ring/src/main/java/net/knego/hiperf/threadhandoff/ring/Main.java`:

```java
package net.knego.hiperf.threadhandoff.ring;

import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / ring (Java): bounded SPSC ring, busy-wait, pipelined depth N.
 * Emits one handoff_throughput line.
 */
public final class Main {

    private static final String EXPERIMENT = "ring";

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            long total = (long) cfg.warmup() + cfg.iterations();

            Spsc ring = new Spsc(cfg.ringCap());

            Thread consumer = new Thread(() -> {
                for (long i = 0; i < total; i++) {
                    ring.pop();
                }
            }, "consumer");
            consumer.start();

            // Warmup pushes, then a drain barrier so timing excludes warmup.
            for (int i = 0; i < cfg.warmup(); i++) {
                ring.push(1);
            }
            while (ring.consumed() < cfg.warmup()) {
                Thread.onSpinWait();
            }

            long startNanos = System.nanoTime();
            for (int i = 0; i < cfg.iterations(); i++) {
                ring.push(1);
            }
            while (ring.consumed() < total) {
                Thread.onSpinWait();
            }
            long elapsedNanos = System.nanoTime() - startNanos;

            consumer.join();

            double throughput = cfg.iterations() / (elapsedNanos / 1_000_000_000.0);
            Handoff.emitThroughput(EXPERIMENT, throughput, cfg.iterations());
        } catch (IllegalArgumentException e) {
            System.err.println("thread-handoff-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            System.err.println("thread-handoff-" + EXPERIMENT + ": interrupted: " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 6: Build and smoke-run**

Run: `cd java && TH_WARMUP=1000 TH_ITERATIONS=200000 ./gradlew :thread-handoff-ring:run -q`
Expected: one `handoff_throughput` line, `unit":"ops_per_sec"`, `value` > 0, `samples:200000`.

- [ ] **Step 7: Commit**

```bash
git add java/thread-handoff-ring/
git commit -m "java(thread-handoff): ring experiment (SPSC busy-wait, pipelined throughput)"
```

---

## Task 18 (Java): full-build verification

**Files:** none (verification only).

- [ ] **Step 1: Full build (runs all tests)**

Run: `cd java && ./gradlew build`
Expected: BUILD SUCCESSFUL; `StatsTest`, `DurableAppendTest`, `HandoffTest`, `SpscTest` all pass.

- [ ] **Step 2: Smoke all four and eyeball the ladder**

Run:
```bash
cd java
for e in spin condvar channel; do TH_WARMUP=500 TH_ITERATIONS=20000 ./gradlew :thread-handoff-$e:run -q; done
TH_WARMUP=1000 TH_ITERATIONS=500000 ./gradlew :thread-handoff-ring:run -q
```
Expected: 3+3+3 latency lines then 1 throughput line; `spin` p50 ≤ `condvar` ≤ `channel` (noise allowed), every `p99 >= p50`, ring throughput > 0.

- [ ] **Step 3: Commit** (only if Step 1/2 required a fix)

```bash
git commit -am "java(thread-handoff): verification fixups"
```

---

## Task 19 (infra): bench-infra matrix + `TH_*` params

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml`
- Modify: `bench-infra/ansible/roles/run/tasks/local.yml`
- Modify: `bench-infra/ansible/roles/build/tasks/main.yml` (artifact-verification list)

**Interfaces:**
- Consumes: the run role's per-experiment env-export + `run_bench.sh` invocation.
- Produces: four `thread-handoff` matrix rows; `TH_*` exported for local experiments; build-verification list naming the four new binaries.

- [ ] **Step 1: Replace the placeholder matrix row** in `bench-infra/ansible/group_vars/all.yml` — replace the line
  `  - { focus_area: thread-handoff,   experiment: placeholder, kind: local }`
  with:

```yaml
  - { focus_area: thread-handoff,   experiment: spin,        kind: local }
  - { focus_area: thread-handoff,   experiment: condvar,     kind: local }
  - { focus_area: thread-handoff,   experiment: channel,     kind: local }
  - { focus_area: thread-handoff,   experiment: ring,        kind: local }
```

- [ ] **Step 2: Add the `th_*` param block** in `bench-infra/ansible/group_vars/all.yml` — immediately after the `fsw_batch: 32` line, add:

```yaml

# thread-handoff params (single-host, node0). Identical across languages.
th_warmup: 10000
th_iterations: 100000
th_ring_cap: 1024
```

- [ ] **Step 3: Update the stale top-of-file comment** in `bench-infra/ansible/group_vars/all.yml` — replace the two comment lines
  `# local runs on node0 only. thread-handoff is still a stub with a single`
  `# 'placeholder' experiment until real experiments are defined.`
  with:

```yaml
# local runs on node0 only. thread-handoff is single-host: spin/condvar/channel
# measure round-trip handoff latency, ring measures pipelined SPSC throughput.
```

- [ ] **Step 4: Export `TH_*`** in `bench-infra/ansible/roles/run/tasks/local.yml` — after the `export FSW_BATCH="{{ fsw_batch }}"` line (line 22), add:

```yaml
    export TH_WARMUP="{{ th_warmup }}"
    export TH_ITERATIONS="{{ th_iterations }}"
    export TH_RING_CAP="{{ th_ring_cap }}"
```

- [ ] **Step 5: Update the build-role artifact-verification list** in `bench-infra/ansible/roles/build/tasks/main.yml` — replace exactly:

```
    # One artifact per experiment (<focus_area>-<experiment>) plus the
    # single-artifact stub focus areas. Each name maps 1:1 to a rust release bin,
    # a go bin, and a java subproject build dir.
    for art in network-rtt-tcp network-rtt-udp network-rtt-quic \
               filesystem-write-fsync filesystem-write-fdatasync \
               filesystem-write-prealloc filesystem-write-batch \
               thread-handoff; do
```

  with:

```
    # One artifact per experiment (<focus_area>-<experiment>). Each name maps 1:1
    # to a rust release bin, a go bin, and a java subproject build dir.
    for art in network-rtt-tcp network-rtt-udp network-rtt-quic \
               filesystem-write-fsync filesystem-write-fdatasync \
               filesystem-write-prealloc filesystem-write-batch \
               thread-handoff-spin thread-handoff-condvar \
               thread-handoff-channel thread-handoff-ring; do
```

- [ ] **Step 6: Verify YAML parses** (no remote run — this is free and offline):

Run: `cd bench-infra && python3 -c "import yaml; yaml.safe_load(open('ansible/group_vars/all.yml')); yaml.safe_load(open('ansible/roles/build/tasks/main.yml')); yaml.safe_load(open('ansible/roles/run/tasks/local.yml')); print('OK')"`
Expected: `OK`.

- [ ] **Step 7: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml \
        bench-infra/ansible/roles/run/tasks/local.yml \
        bench-infra/ansible/roles/build/tasks/main.yml
git commit -m "bench-infra: add thread-handoff matrix rows + TH_* params"
```

---

## Task 20 (docs): update result-contract + CLAUDE.md

**Files:**
- Modify: `docs/result-contract.md`
- Modify: `CLAUDE.md`

**Interfaces:** none (documentation). All edits below are exact find/replace.

- [ ] **Step 1: Update `docs/result-contract.md` "Current state"** — replace exactly:

```
`thread-handoff` remains a **stub** that emits a single placeholder line
(`experiment: "placeholder"`, `metric: "placeholder"`, `notes: "stub"`).
```

  with:

```
`thread-handoff` is implemented for the `spin`, `condvar`, `channel`, and `ring`
experiments (each a runnable artifact named `thread-handoff-<experiment>`):
`spin`/`condvar`/`channel` emit `handoff_rtt_{p50,p99,mean}` (ns), `ring` emits
`handoff_throughput` (ops_per_sec). `shared-memory-ipc` is not yet scaffolded.
```

- [ ] **Step 2: Update `CLAUDE.md` Status** — replace exactly:

```
(single-host, local NVMe). `thread-handoff` is a stub that emits a placeholder line; `shared-memory-ipc`
is not yet scaffolded.
```

  with:

```
(single-host, local NVMe). `thread-handoff` is implemented for the `spin`, `condvar`, `channel`, and
`ring` experiments (single-host). `shared-memory-ipc` is not yet scaffolded.
```

- [ ] **Step 3: Update `CLAUDE.md` "Layout is language-first"** — replace exactly:

```
bench library; a stub focus area (currently `thread-handoff`) has a single artifact named just `<focus_area>`. This keeps each toolchain
```

  with:

```
bench library; a stub focus area would have a single artifact named just `<focus_area>` (none at present). This keeps each toolchain
```

- [ ] **Step 4: Update `CLAUDE.md` Artifact names line** — replace exactly:

```
Artifact names: `network-rtt-{tcp,udp,quic}`, `filesystem-write-{fsync,fdatasync,prealloc,batch}`, `thread-handoff`.
```

  with:

```
Artifact names: `network-rtt-{tcp,udp,quic}`, `filesystem-write-{fsync,fdatasync,prealloc,batch}`, `thread-handoff-{spin,condvar,channel,ring}`.
```

- [ ] **Step 5: Fix the three stale "+ stubs" build-block comments in `CLAUDE.md`** (all focus areas are now real). Apply three exact replacements:

  Replace `# Rust — Cargo workspace: bench-common + network-rtt/{tcp,udp,quic} + stubs`
  with `# Rust — Cargo workspace: bench-common + network-rtt + filesystem-write + thread-handoff experiments`

  Replace `# Go — single module: internal/bench + cmd/network-rtt-{tcp,udp,quic} + stubs`
  with `# Go — single module: internal/bench + cmd/network-rtt-* + filesystem-write-* + thread-handoff-*`

  Replace `# Java — single Gradle build: :common + :network-rtt-{tcp,udp,quic} + stubs, JDK 21 toolchain`
  with `# Java — single Gradle build: :common + :network-rtt-* + :filesystem-write-* + :thread-handoff-*, JDK 21 toolchain`

- [ ] **Step 6: Sanity-check no stale "thread-handoff ... stub/placeholder" remains**

Run: `cd /home/claude/ultima/hi-perf-cmp && grep -rniE 'thread-handoff' CLAUDE.md docs/result-contract.md | grep -iE 'stub|placeholder'`
Expected: no output.

- [ ] **Step 7: Commit**

```bash
git add CLAUDE.md docs/result-contract.md
git commit -m "docs: thread-handoff is now a real four-experiment focus area"
```

---

## Final verification

- [ ] **Step 1: All three languages green**

Run:
```bash
cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check && cd ..
cd go && go build ./... && go vet ./... && go test ./... && cd ..
cd java && ./gradlew build && cd ..
```
Expected: every command succeeds.

- [ ] **Step 2: Confirm the 10-line-per-language output shape** (small smoke), e.g. Rust:

```bash
cd rust
for e in spin condvar channel; do TH_WARMUP=200 TH_ITERATIONS=5000 cargo run -q --release -p thread-handoff-$e; done
TH_WARMUP=500 TH_ITERATIONS=200000 cargo run -q --release -p thread-handoff-ring
```
Expected: 9 `handoff_rtt_*` lines (3 per latency experiment) + 1 `handoff_throughput` line = 10 lines, all valid result-contract JSON with `focus_area":"thread-handoff"`.

- [ ] **Step 3:** These are **fitness checks only — never journaled.** The first journal entry for `thread-handoff` comes from a genuine AWS `bench-infra` run (`make up` → `make bench` → record), which is **user-initiated**. Do not run AWS or journal anything as part of this plan.
