#!/usr/bin/env bash
# run_bench.sh <language> <focus_area> <experiment> <mode>
#
# Execs the right per-language benchmark invocation from the synced source tree,
# printing ONLY result-contract JSON lines to stdout (logs go to stderr). The
# matrix driver (run role) handles server/client orchestration across hosts.
#
# Artifact naming: experiments are separate artifacts named <focus_area>-<experiment>
# (e.g. network-rtt-tcp). The still-stubbed focus areas have no real experiments,
# so their single artifact is named just <focus_area> (experiment "placeholder").
#
# Environment (consumed by the network-rtt benchmark, per the RTT_* contract):
#   RTT_MODE          loopback (default) | server | client
#   RTT_HOST          responder address (client mode)
#   RTT_TCP_PORT      TCP echo port           (default 9100)
#   RTT_UDP_PORT      UDP echo port           (default 9101)
#   RTT_QUIC_PORT     QUIC echo port          (default 9102)
#   RTT_PAYLOAD_BYTES / RTT_WARMUP / RTT_ITERATIONS  measurement params
#
# rpc-roundtrip consumes the RPC_* contract (cross-host, like RTT_*):
#   RPC_MODE          loopback (default) | server | client
#   RPC_HOST          responder address (client mode)
#   RPC_UDP_PORT      sbe_udp datagram port   (default 9200)
#   RPC_TCP_PORT      bebop_tcp port          (default 9201)
#   RPC_GRPC_PORT     grpc HTTP/2 port        (default 9202)
#   RPC_WARMUP / RPC_ITERATIONS  measurement params
#
#   SRC_DIR           path to the synced repo root (required)
#   JAVA_HOME         JDK home (required for java)
#
# filesystem-write consumes the FSW_* vars (below); thread-handoff and
# serialization ignore RTT_*/RPC_*/FSW_* (their own SER_*/TH_* vars are
# exported by the caller, not defaulted here).
set -euo pipefail

usage() {
  echo "usage: $0 <rust|go|java> <network-rtt|filesystem-write|thread-handoff|serialization|smr-collections|rpc-roundtrip> <experiment> <loopback|server|client>" >&2
  exit 2
}

[ "$#" -eq 4 ] || usage
LANGUAGE="$1"
FOCUS_AREA="$2"
EXPERIMENT="$3"
MODE="$4"

SRC_DIR="${SRC_DIR:?SRC_DIR must point at the synced repo root}"

case "$FOCUS_AREA" in
  network-rtt|filesystem-write|thread-handoff|serialization|smr-collections|rpc-roundtrip) ;;
  *) echo "unknown focus_area: $FOCUS_AREA" >&2; usage ;;
esac

# Artifact name: <focus_area>-<experiment> for real experiments; the bare
# <focus_area> for the placeholder stubs (no per-experiment artifact yet).
if [ "$EXPERIMENT" = "placeholder" ]; then
  ARTIFACT="${FOCUS_AREA}"
else
  ARTIFACT="${FOCUS_AREA}-${EXPERIMENT}"
fi

# Export the RTT contract with defaults so each benchmark sees a consistent env.
export RTT_MODE="${MODE}"
export RTT_HOST="${RTT_HOST:-}"
export RTT_TCP_PORT="${RTT_TCP_PORT:-9100}"
export RTT_UDP_PORT="${RTT_UDP_PORT:-9101}"
export RTT_QUIC_PORT="${RTT_QUIC_PORT:-9102}"
export RTT_PAYLOAD_BYTES="${RTT_PAYLOAD_BYTES:-64}"
export RTT_WARMUP="${RTT_WARMUP:-10000}"
export RTT_ITERATIONS="${RTT_ITERATIONS:-100000}"

# Export the RPC contract with defaults, mirroring RTT_* above.
export RPC_MODE="${MODE}"
export RPC_HOST="${RPC_HOST:-}"
export RPC_UDP_PORT="${RPC_UDP_PORT:-9200}"
export RPC_TCP_PORT="${RPC_TCP_PORT:-9201}"
export RPC_GRPC_PORT="${RPC_GRPC_PORT:-9202}"
export RPC_WARMUP="${RPC_WARMUP:-10000}"
export RPC_ITERATIONS="${RPC_ITERATIONS:-100000}"

# Export the filesystem-write contract. FSW_DIR defaults to the CWD, which the
# run role points at the NVMe-backed scratch dir. tmpfs would give meaningless
# durability numbers, so a real-disk dir is required.
export FSW_DIR="${FSW_DIR:-$PWD}"
export FSW_ENTRY_BYTES="${FSW_ENTRY_BYTES:-256}"
export FSW_WARMUP="${FSW_WARMUP:-5000}"
export FSW_ITERATIONS="${FSW_ITERATIONS:-50000}"
export FSW_BATCH="${FSW_BATCH:-32}"

# Export the smr-collections contract. Identical across languages.
export SMRC_CAP="${SMRC_CAP:-262144}"
export SMRC_LEVELS="${SMRC_LEVELS:-1024}"
export SMRC_TICK="${SMRC_TICK:-1}"
export SMRC_PRICE_MIN="${SMRC_PRICE_MIN:-0}"
export SMRC_STEADY="${SMRC_STEADY:-60000}"
export SMRC_WARMUP="${SMRC_WARMUP:-10000}"
export SMRC_ITERS="${SMRC_ITERS:-100000}"

case "$LANGUAGE" in
  rust)
    exec "${SRC_DIR}/rust/target/release/${ARTIFACT}"
    ;;
  go)
    exec "${SRC_DIR}/go/bin/${ARTIFACT}"
    ;;
  java)
    : "${JAVA_HOME:?JAVA_HOME must be set for the java run}"
    export JAVA_HOME
    export PATH="${JAVA_HOME}/bin:${PATH}"
    cd "${SRC_DIR}/java"
    # -q keeps gradle's own chatter off stdout; redirect any stray stderr noise
    # away so stdout carries only result-contract JSON lines.
    exec ./gradlew ":${ARTIFACT}:run" -q --no-daemon 2>/dev/null
    ;;
  *)
    echo "unknown language: $LANGUAGE" >&2
    usage
    ;;
esac
