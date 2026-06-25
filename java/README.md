# Java benchmarks

Single Gradle build (Kotlin DSL). One application subproject per focus area;
`:common` holds the shared result-contract record and emitter. Targets Java 21
via a Gradle toolchain.

## Build

```sh
./gradlew build
```

## Run a benchmark

```sh
./gradlew :network-rtt:run -q
./gradlew :filesystem-write:run -q
./gradlew :thread-handoff:run -q
```

Each prints one [result-contract](../docs/result-contract.md) JSON line on
stdout. They are currently **stubs** — they emit a placeholder result.
