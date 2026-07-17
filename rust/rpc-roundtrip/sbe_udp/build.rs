use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema = manifest.join("schema/rpc_payload.xml");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out = out_dir.join("sbe");

    println!("cargo:rerun-if-changed={}", schema.display());
    let xml = fs::read_to_string(&schema).expect("read schema");
    fs::create_dir_all(&out).expect("create out dir");
    sbe_gen::generate_to(&xml, &out, &sbe_gen::GeneratorOptions::default())
        .expect("sbe_gen generate");

    let shim = format!(
        r#"#[allow(dead_code, non_camel_case_types, unused_imports, unused_parens, clippy::all)]
mod sbe {{
    #[path = {types:?}]
    pub mod types;
    #[path = {message_header:?}]
    pub mod message_header;
    #[path = {rpc_payload:?}]
    pub mod rpc_payload;
    pub use message_header::MessageHeader;
}}
"#,
        types = out.join("types.rs").display().to_string(),
        message_header = out.join("message_header.rs").display().to_string(),
        rpc_payload = out.join("rpc_payload.rs").display().to_string(),
    );
    fs::write(out_dir.join("sbe_mod.rs"), shim).expect("write sbe module shim");
}
