#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Rust crate from the shared schema.
# Requires a JDK (regeneration only; normal builds use the committed output).
set -eu
here=$(dirname "$0")
jar="$here/../../serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$here/../schema/book_snapshot.xml"
out="$here/generated"
rm -rf "$out"
mkdir -p "$out"
java -Dsbe.target.language=Rust -Dsbe.output.dir="$out" -jar "$jar" "$schema"

# Overwrite the tool's Cargo.toml with the workspace-member manifest: package
# renamed so `booksnap = { package = "booksnap-codec", ... }` resolves; lib name
# "booksnap"; inherits [workspace.package]; no rust-version key.
cat > "$out/booksnap/Cargo.toml" <<'TOML'
[package]
name = "booksnap-codec"
version.workspace = true
edition.workspace = true

[lib]
name = "booksnap"
path = "src/lib.rs"
TOML

cargo fmt -p booksnap-codec
echo "regenerated + formatted $out/booksnap" 1>&2
