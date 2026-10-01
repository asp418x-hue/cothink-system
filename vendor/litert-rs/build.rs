use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=native/wrapper.h");
    println!("cargo:rerun-if-changed=native/engine.h");

    let bindings = bindgen::Builder::default()
        .header("native/wrapper.h")
        // Generate a `LiteRtLm` struct whose methods are loaded at runtime via
        // libloading (we dlopen the .dylib/.so ourselves; nothing is linked at
        // build time).
        .dynamic_library_name("LiteRtLm")
        .allowlist_function("litert_lm_.*")
        .allowlist_type("LiteRtLm.*")
        .allowlist_var("kLiteRtLm.*")
        .generate()
        .expect("Unable to generate LiteRT-LM bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
