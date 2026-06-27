# Task: rust-thread-handoff-disruptor

A `Local` reference cell: the Rust `thread-handoff/disruptor` artifact measures
SPSC handoff throughput using the `disruptor` crate (LMAX Disruptor port,
`BusySpin`), so the hand-rolled `rust-thread-handoff-ring` champion can be
compared against a mature library on the same harness/box.

## Objective

Maximize `handoff_throughput` (`ops_per_sec`), measured identically to `ring`
(warmup, then time `TH_ITERATIONS` published events, draining the consumer before
stopping the clock). `TH_RING_CAP` is the disruptor buffer size (power-of-two).

## Mutable / frozen

- Mutable: `rust/thread-handoff/disruptor/src/**`.
- Frozen: `bench-common`, every other cell, the result contract, `autobench/**`.
  The `disruptor` dependency is the point of this cell (an experiment-specific
  dep, like QUIC) — keep it; do not vendor-fork it to win a number.

## Result (AWS c6id, median-of-5)

disruptor-rs v4.3 BusySpin: ~148.0M ops/s. Our `ring` champion: ~367.6M ops/s
(~2.5x faster). See `journal/runs/20260627T193417Z-*/entry.md`.

## TSV schema

`commit	handoff_throughput_ops_per_sec	status	description` (maximize).
