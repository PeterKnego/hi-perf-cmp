# thread-handoff — Backoff idle-strategy cells (`backoff`, `backoff_yield`) — Design

**Date:** 2026-08-05
**Status:** Approved (user-directed), implemented in the same session.

## Purpose

The thread-handoff grid measures the two ends of the wait spectrum — `spin`
(hot core) and `condvar`/`channel` (blocking park) — but not the middle that
real duty-cycle systems actually run: the Aeron-style **spin → yield → timed
park** backoff ladder (Agrona `BackoffIdleStrategy`; the agent idle loops in
Aeron, gomatch, and ultima_cluster's uc2 agents).

The ladder's short rungs depend on the platform's **timed-park granularity**,
and that is sharply language-specific — which is this grid's whole premise:

- **Go**: `time.Sleep` overshoots sub-millisecond requests by orders of
  magnitude (measured in aeron-go `1ce3720`, on gomatch's fleet: a 6 µs
  request costs ~425 µs; ≥ 8 µs costs ~1 ms). The 1 µs → 1 ms doubling ladder
  collapses to its top rung by the fourth park.
- **Java**: `LockSupport.parkNanos` overshoots by ~tens of µs on Linux — bad,
  but two orders less bad.
- **Rust**: `thread::sleep` (nanosleep) sits in the same tens-of-µs band.

aeron-go `1ce372035f` fixes the Go collapse by serving parks shorter than a
floor (default 1 ms) with a yield-to-deadline loop (`runtime.Gosched()`),
recovering ~80 % of busy-spin's latency without pinning a core and beating
spin on p99 (a spinning goroutine never yields its P). Cluster-measured
there: backoff 793 µs → yielding 224 µs → spin 184 µs median ack at 10 k/s.

Two new cells put both facts on this grid's controlled methodology:

- **`backoff`** (rust, go, java): each language's *idiomatic naive* ladder.
  **Go deliberately keeps `time.Sleep`** (project-owner decision): the
  overshoot is the datum — this cell measures what a straightforward port of
  the Agrona pattern actually costs per language.
- **`backoff_yield`** (go only): the aeron-go yielding strategy, same ladder
  parameters plus the 1 ms sleep floor. The `backoff` → `backoff_yield` delta
  is the fix's value measured on this grid.

## Shape: paced ping-pong

A hot ping-pong never deepens the ladder — work always arrives within the
spin rungs, and every backoff cell would measure `spin`. The pathology lives
on the **idle → wakeup** path, so the requester paces:

1. Requester **busy-waits a gap** (`TH_GAP_NS`, default 100 µs) — untimed,
   and a busy-wait rather than a sleep so the requester's own send timing
   does not inherit the very overshoot being measured.
2. Timed round trip as in `spin`: store req, spin-wait resp. The requester
   always spins — it is the measurement side; the **responder** is the
   system-under-test, waiting for req under the idle strategy.

During the gap the responder's ladder ramps: spins and yields exhaust in ~µs,
then it parks. At gap = 100 µs, a Java/Rust responder is a few honest park
rungs deep (wakeup ≤ ~60 µs); a Go `time.Sleep` responder is inside a ~1 ms
actual sleep by its first park, so wakeup is the sleep remainder (~ hundreds
of µs); a Go yielding responder never sleeps below the floor and wakes in µs.
Predicted ordering: `backoff_yield` ≈ spin ≪ go `backoff`, with rust/java
`backoff` in between — the cross-language timed-park story, plus the fix's
delta, in one table.

## Ladder parameters (fixed, aeron-go defaults)

maxSpins 10, maxYields 20, minPark 1 µs, maxPark 1 ms, doubling; reset on
work. `backoff_yield` adds sleepFloor 1 ms (all sub-floor parks yield to
deadline). Parameters are constants, not env — comparability across languages
is the point; only the pacing gap (`TH_GAP_NS`) is configurable.

Implementations: Java uses the real Agrona `BackoffIdleStrategy` (dependency
local to the artifact, per house rule; Agrona is already in the tree via
smr-collections). Go and Rust hand-roll the identical ladder in a small
shared/tested module (`go/internal/idle`; ladder module inside the Rust
artifact — std-only). The ladder's state progression is unit-tested via an
injected parker; wall-clock overshoot itself is deliberately not unit-tested
(host-dependent — it is what the fleet cells measure).

## Metrics & config

Standard `handoff_rtt_{p50,p99,mean}` (ns), experiment `backoff` /
`backoff_yield`. New env `TH_GAP_NS` (default 100 000), exported by the
ansible run role like the other `TH_*` vars; existing cells ignore it.
Fleet runtime note: go `backoff` runs ~(gap + ~1 ms) × 110 k ≈ 2 min — the
slowest cell of the focus area, budgeted, not a hang.

## Out of scope

- Wiring the yielding strategy into any non-benchmark code here (uc2's agents
  are Rust; gomatch already carries it).
- A Rust/Java yielding variant: their naive ladders are only ~tens of µs off;
  add later if the `backoff` rows show it matters.
- Revisiting `condvar`/`channel`/`ring` — untouched.
