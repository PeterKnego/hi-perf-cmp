#!/usr/bin/env sh
# Regenerate the committed FlatBuffers Go codec from schema/journal.fbs.
# Requires flatc 23.5.26 on PATH (regeneration only; normal builds use the
# committed output). Install: apt-get install flatbuffers-compiler, or download
# the prebuilt binary from https://github.com/google/flatbuffers/releases/tag/v23.5.26
set -eu
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$here/journalfb"
flatc --go -o "$here" "$here/schema/journal.fbs"
gofmt -w "$here/journalfb"
echo "regenerated + gofmt'd $here/journalfb" 1>&2
