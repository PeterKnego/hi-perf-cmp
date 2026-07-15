#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Java codec (Agrona flyweights) from the
# shared schema. Requires a JDK (regeneration only).
set -eu
here=$(dirname "$0")
root="$here/../.."
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/smr-collections/schema/book_snapshot.xml"
out="$here/src/main/java"
rm -rf "$out/booksnap"
java -Dsbe.target.language=Java -Dsbe.output.dir="$out" -jar "$jar" "$schema"
echo "regenerated $out/booksnap" 1>&2
