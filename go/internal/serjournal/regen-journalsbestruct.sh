#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Go *struct* (owned) codec from the
# shared schema. Requires a JDK (regeneration only; builds use committed output).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/serialization/aeron_sbe/schema/journal.xml"
rm -rf "$here/journalsbestruct"
java -Dsbe.target.language=Golang \
     -Dsbe.target.namespace=journalsbestruct \
     -Dsbe.output.dir="$here" -jar "$jar" "$schema"
gofmt -w "$here/journalsbestruct"
echo "regenerated + gofmt'd $here/journalsbestruct" 1>&2
