# Result Contract

Every benchmark — in every language, for every focus area — is an independently
runnable artifact that prints **one JSON object per line** to **stdout**. This is
the only coupling between the benchmarks and the (future) comparison harness:
the harness runs an artifact and parses the line(s) it emits.

Keeping the contract this small means each benchmark stays a plain executable
with no harness dependency, and the harness stays a plain line reader with no
per-language knowledge.

## Schema

| field        | type    | required | meaning                                                       |
|--------------|---------|----------|---------------------------------------------------------------|
| `language`   | string  | yes      | `rust` \| `java` \| `go`                                      |
| `focus_area` | string  | yes      | `network-rtt` \| `filesystem-write` \| `thread-handoff`       |
| `metric`     | string  | yes      | what was measured, e.g. `rtt_p50`, `write_throughput`         |
| `value`      | number  | yes      | the measured value                                            |
| `unit`       | string  | yes      | unit of `value`, e.g. `ns`, `us`, `ms`, `bytes_per_sec`, `ops_per_sec` |
| `samples`    | integer | yes      | number of samples behind `value`                              |
| `notes`      | string  | no       | free-form context (config, caveats); `"stub"` for placeholders |

### Rules

- **One result per line.** A benchmark reporting several metrics (e.g. p50, p99)
  prints one line per metric.
- **stdout is for results only.** Send logs, progress, and diagnostics to stderr.
- **`value` is a JSON number.** `0` and `0.0` are both accepted.
- **Numbers carry their unit in `unit`** — never bake the unit into `value`.

## Example

```json
{"language":"rust","focus_area":"network-rtt","metric":"rtt_p50","value":42000,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"network-rtt","metric":"rtt_p99","value":81000,"unit":"ns","samples":100000}
```

## Current state

All benchmarks are **stubs** that emit a single placeholder line
(`metric: "placeholder"`, `notes: "stub"`) so the trees build and run today.
Real measurement logic replaces the placeholder per focus area in later work.

## Reference emitters

- **Rust** — hand-rendered `println!` in each `src/main.rs` (zero deps).
- **Go** — `internal/result` package (`result.Emit`).
- **Java** — `net.knego.hiperf.common.Result#emit` in the `:common` subproject.
