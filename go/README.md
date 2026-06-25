# Go benchmarks

Single Go module. One `cmd/` binary per focus area; `internal/result` holds the
shared result-contract emitter.

## Build

```sh
go build ./...
```

## Run a benchmark

```sh
go run ./cmd/network-rtt
go run ./cmd/filesystem-write
go run ./cmd/thread-handoff
```

Each binary prints one [result-contract](../docs/result-contract.md) JSON line
on stdout. They are currently **stubs** — they emit a placeholder result.
