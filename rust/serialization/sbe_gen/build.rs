use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema = manifest.join("schema/journal.xml");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out = out_dir.join("sbe");

    println!("cargo:rerun-if-changed={}", schema.display());
    let xml = fs::read_to_string(&schema).expect("read schema");
    fs::create_dir_all(&out).expect("create out dir");
    sbe_gen::generate_to(&xml, &out, &sbe_gen::GeneratorOptions::default())
        .expect("sbe_gen generate");

    // The generated `types.rs`/`message_header.rs` carry a crate-root-style
    // `#![allow(dead_code, non_camel_case_types)]` inner attribute. Rust does
    // not permit an inner attribute injected via `include!` into an
    // already-open item body (only a real file-module accepts it), so each
    // generated file must become its own file-module via `#[path]`. `#[path]`
    // requires a string literal (no `env!`/`concat!`), so we bake the
    // absolute OUT_DIR-relative paths in here, at build.rs time, into a small
    // shim module that `src/lib.rs` pulls in with a single
    // `include!(concat!(env!("OUT_DIR"), "/sbe_mod.rs"))`.
    let shim = format!(
        r#"#[allow(dead_code, non_camel_case_types, unused_imports, unused_parens, clippy::all)]
pub mod sbe {{
    #[path = {types:?}]
    pub mod types;
    #[path = {message_header:?}]
    pub mod message_header;
    #[path = {journal_record:?}]
    pub mod journal_record;
    pub use message_header::MessageHeader;
}}
"#,
        types = out.join("types.rs").display().to_string(),
        message_header = out.join("message_header.rs").display().to_string(),
        journal_record = out.join("journal_record.rs").display().to_string(),
    );
    fs::write(out_dir.join("sbe_mod.rs"), shim).expect("write sbe module shim");
}
