#!/bin/sh
# Regenerate journalpb/ from schema/journal.proto. Requires protoc (3.21+) on
# PATH; protoc-gen-go is version-pinned and installed to a temp dir. The
# protoc and plugin versions used are recorded in the generated file header.
# Dev-time only; the output is committed so bench hosts need no protoc.
set -eu
cd "$(dirname "$0")"
PLUGIN_DIR="$(mktemp -d)"
trap 'rm -rf "$PLUGIN_DIR"' EXIT
GOBIN="$PLUGIN_DIR" go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6
PATH="$PLUGIN_DIR:$PATH" protoc \
    --go_out=. \
    --go_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/serjournal \
    schema/journal.proto
