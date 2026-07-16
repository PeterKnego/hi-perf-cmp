#!/bin/sh
# Regenerate journalbop/ from schema/journal.bop with the 200sc/bebop
# generator at the version pinned in go.mod. Dev-time only; the output is
# committed so bench hosts need no generator.
set -eu
cd "$(dirname "$0")"
mkdir -p journalbop
go run github.com/200sc/bebop/main/bebopc-go \
    -i schema/journal.bop -o journalbop/journal.go -package journalbop
gofmt -w journalbop/journal.go
