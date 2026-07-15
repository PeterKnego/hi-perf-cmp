#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Go codec from the shared schema.
# Requires a JDK (regeneration only; normal builds use the committed output).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/smr-collections/schema/book_snapshot.xml"
out="$here"                       # tool creates booksnap/ under here
rm -rf "$here/booksnap"
java -Dsbe.target.language=Golang -Dsbe.output.dir="$out" -jar "$jar" "$schema"
gofmt -w "$here/booksnap"
echo "regenerated + gofmt'd $here/booksnap" 1>&2
