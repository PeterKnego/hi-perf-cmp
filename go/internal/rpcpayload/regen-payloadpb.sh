#!/bin/sh
# Regenerate payloadpb/ (message + gRPC service) from schema/rpc_payload.proto.
# Requires protoc (3.21+) on PATH; protoc-gen-go and protoc-gen-go-grpc are
# version-pinned and installed to a temp dir. Output is committed (bench hosts
# need no protoc). Dev-time only.
set -eu
cd "$(dirname "$0")"
PLUGIN_DIR="$(mktemp -d)"
trap 'rm -rf "$PLUGIN_DIR"' EXIT
GOBIN="$PLUGIN_DIR" go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6
GOBIN="$PLUGIN_DIR" go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1
PATH="$PLUGIN_DIR:$PATH" protoc \
    --go_out=. --go_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload \
    --go-grpc_out=. --go-grpc_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload \
    schema/rpc_payload.proto
