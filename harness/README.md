# Harness (placeholder)

The cross-language perf-testing harness will live here. It is **not yet
implemented** — this directory is a placeholder so the structure is in place.

## Intended role

Orchestrate the per-language benchmark artifacts and compare their results:

1. Build/locate each benchmark binary (Rust, Java, Go) for a given focus area.
2. Run them under controlled conditions (warmup, iteration count, pinning, etc.).
3. Collect the [result-contract](../docs/result-contract.md) JSON lines each
   emits on stdout.
4. Align results by `focus_area` + `metric` and produce a comparison
   (table / report), writing artifacts to `../results/`.

The harness depends only on the result contract — not on any language's build
system internals — so benchmarks and harness can evolve independently.

The implementation language and CLI are deliberately left open until the first
real benchmarks exist.
