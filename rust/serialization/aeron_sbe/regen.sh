#!/usr/bin/env sh
# Regenerate the vendored real-logic SBE Rust crate from schema/journal.xml.
# Requires a JDK (only for regeneration; normal builds use the committed output).
set -eu
here=$(dirname "$0")
jar="$here/vendor/sbe-all-1.38.1.jar"
out="$here/generated"
rm -rf "$out"
mkdir -p "$out"
java -Dsbe.target.language=Rust -Dsbe.output.dir="$out" -jar "$jar" "$here/schema/journal.xml"
echo "regenerated $out/journal" 1>&2
