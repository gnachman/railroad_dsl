// build.rs

extern crate cbindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    cbindgen::generate(&crate_dir)
        .expect("cbindgen failed")
        .write_to_file(PathBuf::from(crate_dir).join("include/railroad_dsl.h"));
}
