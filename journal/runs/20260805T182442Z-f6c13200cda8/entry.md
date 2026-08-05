# 20260805T182442Z-f6c13200cda8

- commit: f6c13200cda82cb8fdb85ae128e86f61096ee669 clean
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
thread-handoff re-measure + first backoff/backoff_yield cells: the Aeron ladder's timed-park cost per language (Go time.Sleep collapse vs the aeron-go yielding fix vs nanosleep/parkNanos)

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### thread-handoff / backoff

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 926328.5 | 969296 | 986079 |
| java | 26556.5 | 26185 | 41433 |
| rust | 25220.3 | 25064 | 32222 |

### thread-handoff / backoff_yield

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 34204.8 | 33657 | 46029 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 347.6 | 305 | 575 |
| java | 12847.6 | 20241 | 30056 |
| rust | 444.4 | 407 | 1094 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 441.9 | 396 | 853 |
| java | 310.4 | 281 | 712 |
| rust | 353.0 | 383 | 414 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 54689576.5 |
| java | 6877201.2 |
| rust | 379472078.4 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 117.3 | 98 | 203 |
| java | 318.6 | 307 | 377 |
| rust | 236.4 | 232 | 240 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
