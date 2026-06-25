# Rust benchmarks

Cargo workspace. One binary crate per focus area.

## Build

```sh
cargo build --release
```

## Run a benchmark

```sh
cargo run --release -p network-rtt
cargo run --release -p filesystem-write
cargo run --release -p thread-handoff
```

Each binary prints one [result-contract](../docs/result-contract.md) JSON line
on stdout. They are currently **stubs** — they emit a placeholder result.
