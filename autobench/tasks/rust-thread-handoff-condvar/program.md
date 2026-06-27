# Task: rust-thread-handoff-condvar

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Minimize round-trip handoff latency** of the rust `thread-handoff`/`condvar` cell
— a timer ping-pongs a token with a parked responder via a mutex+condition-variable rendezvous, measured by the
existing artifact, which emits `handoff_rtt_p50`/`_p99`/`_mean` (unit `ns`).
Primary: `handoff_rtt_p50_ns` (minimize).

## Kind

`Local`, single-process run with `TH_WARMUP`/`TH_ITERATIONS`. Single-host, so the
local number is meaningful; the real reportable number comes from AWS graduation.

## Mutable / frozen

- Mutable: the cell's own source only (rust `thread-handoff-condvar`).
- Frozen: the shared bench library (owns timing/emission), every other cell,
  `autobench/**`, the result contract, docs. No new dependency.

**Goodhart trap:** the handoff must stay real — the timer must actually receive the
responder's echo each round trip, and the responder must service exactly
`warmup+iterations` round trips. A bounded **spin-then-park** hybrid (spin briefly
on the condition, then fall back to the real blocking wait/recv) is a legitimate
latency optimization; but the **parking fallback must remain** — do NOT degenerate
into an unbounded busy-wait (that would just duplicate the `spin` cell) or remove
the wait. The orchestrator reviews each KEEP diff.

## Noise

Latency on a shared dev box is scheduler/JIT-noisy (±several-fold); the dedicated
AWS box is stable (±~7%). Always use median-of-N; re-run within-noise deltas.

## Gates

1. build, 2. correctness smoke (TH_WARMUP=20, TH_ITERATIONS=200 → 3 handoff_rtt_*_ns > 0),
3. microbench median-of-N (TH_WARMUP=2000, TH_ITERATIONS=20000), 4. Gate A tests.

## TSV schema

`commit\thandoff_rtt_p50_ns\thandoff_rtt_p99_ns\thandoff_rtt_mean_ns\tstatus\tdescription` (p50 primary, minimize).
