#!/bin/sh
# Regenerate payloadbop/ from schema/rpc_payload.bop with the 200sc/bebop
# generator at the version pinned in go.mod. Dev-time only; output is committed.
set -eu
cd "$(dirname "$0")"
mkdir -p payloadbop
go run github.com/200sc/bebop/main/bebopc-go \
    -i schema/rpc_payload.bop -o payloadbop/rpc_payload.go -package payloadbop
gofmt -w payloadbop/rpc_payload.go
