# Result Contract

Every benchmark — in every language, for every focus area — is an independently
runnable artifact that prints **one JSON object per line** to **stdout**. This is
the only coupling between the benchmarks and the downstream tooling: the
`tools/journal` CLI runs an artifact (or reads a collected run) and parses the
line(s) it emits.

Keeping the contract this small means each benchmark stays a plain executable
with no tooling dependency, and the journal stays a plain line reader with no
per-language knowledge. The journal records runs over time and compares them —
see `journal/README.md`.

## Schema

| field        | type    | required | meaning                                                       |
|--------------|---------|----------|---------------------------------------------------------------|
| `language`   | string  | yes      | `rust` \| `java` \| `go`                                      |
| `focus_area` | string  | yes      | `network-rtt` \| `filesystem-write` \| `thread-handoff` \| `serialization` |
| `experiment` | string  | yes      | the variant under the focus area, e.g. `tcp` \| `udp` \| `quic`; `placeholder` for stubs |
| `metric`     | string  | yes      | what was measured, e.g. `rtt_p50`, `write_throughput`         |
| `value`      | number  | yes      | the measured value                                            |
| `unit`       | string  | yes      | unit of `value`, e.g. `ns`, `us`, `ms`, `bytes_per_sec`, `ops_per_sec` |
| `samples`    | integer | yes      | number of samples behind `value`                              |
| `notes`      | string  | no       | free-form context (config, caveats); `"stub"` for placeholders |

The comparison grid is **`experiment` × `language`** within a `focus_area`: align
results on `(focus_area, experiment, metric)` to compare languages, or on
`(focus_area, language, metric)` to compare experiments. The transport/variant
lives in `experiment`, NOT baked into `metric` (so `network-rtt`/`tcp`/`rtt_p50`,
never `tcp_rtt_p50`).

### Rules

- **One result per line.** A benchmark reporting several metrics (e.g. p50, p99)
  prints one line per metric.
- **stdout is for results only.** Send logs, progress, and diagnostics to stderr.
- **`value` is a JSON number.** `0` and `0.0` are both accepted.
- **Numbers carry their unit in `unit`** — never bake the unit into `value`.

## Example

```json
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":42000,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p99","value":81000,"unit":"ns","samples":100000}
```

## Current state

`network-rtt` is implemented for the `tcp`, `udp`, and `quic` experiments, and
`filesystem-write` for the `fsync`, `fdatasync`, `prealloc`, and `batch`
experiments (each a separate runnable artifact named `<focus_area>-<experiment>`).
`thread-handoff` is implemented for the `spin`, `condvar`, `channel`, and `ring`
experiments (each a runnable artifact named `thread-handoff-<experiment>`):
`spin`/`condvar`/`channel` emit `handoff_rtt_{p50,p99,mean}` (ns), `ring` emits
`handoff_throughput` (ops_per_sec). `serialization` is implemented for the
`sbe_gen`, `aeron_sbe`, and `bincode` experiments (Rust only, single-host):
each emits `encode_ns`/`decode_ns` (ns) and `encoded_bytes`/`decode_alloc_bytes`
(bytes). `shared-memory-ipc` is not yet scaffolded.

## Reference emitters

Each language has a shared **bench-common** library that owns Stats, env-config
parsing, the timed measurement loop, and result emission (including
`experiment`); experiment artifacts are thin and call into it.
- **Rust** — `bench-common` crate (`result::emit`).
- **Go** — `internal/bench` package (`bench.Emit`).
- **Java** — `net.knego.hiperf.common` (`Result#emit`) in the `:common` subproject.
