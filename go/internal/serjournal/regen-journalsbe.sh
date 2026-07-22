#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Go *flyweight* codec from the shared
# schema. Requires a JDK (regeneration only; normal builds use committed output).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/serialization/aeron_sbe/schema/journal.xml"
rm -rf "$here/journalsbe"
java -Dsbe.target.language=Golang \
     -Dsbe.go.generate.generate.flyweights=true \
     -Dsbe.target.namespace=journalsbe \
     -Dsbe.output.dir="$here" -jar "$jar" "$schema"
gofmt -w "$here/journalsbe"
echo "regenerated + gofmt'd $here/journalsbe" 1>&2
