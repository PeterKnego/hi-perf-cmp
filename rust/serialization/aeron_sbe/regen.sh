#!/usr/bin/env sh
# Regenerate the vendored real-logic SBE Rust crate from schema/journal.xml.
# Requires a JDK (only for regeneration; normal builds use the committed output).
#
# This script is one-shot: it regenerates, re-applies the workspace-member
# manifest the tool does not know about, and formats the output so a plain
# `cargo build` / `cargo fmt --check` stays green afterwards.
set -eu
here=$(dirname "$0")
jar="$here/vendor/sbe-all-1.38.1.jar"
out="$here/generated"
rm -rf "$out"
mkdir -p "$out"
java -Dsbe.target.language=Rust -Dsbe.output.dir="$out" -jar "$jar" "$here/schema/journal.xml"

# The tool emits its own Cargo.toml (package "serialization", edition 2021).
# Overwrite it with the workspace-member manifest this repo depends on: package
# renamed so the path-dep `journal = { package = "journal-aeron-sbe", ... }`
# resolves, lib name "journal", inheriting [workspace.package]; no rust-version
# key (the workspace defines none).
cat > "$out/journal/Cargo.toml" <<'TOML'
[package]
name = "journal-aeron-sbe"
version.workspace = true
edition.workspace = true

[lib]
name = "journal"
path = "src/lib.rs"
TOML

# The tool output is not rustfmt-formatted; format it so `cargo fmt --check`
# passes at the workspace level (the generated crate is committed).
cargo fmt -p journal-aeron-sbe

echo "regenerated + formatted $out/journal" 1>&2
