// build.rs

use std::env;
use std::fs;
use std::path::Path;

const CONFIG_FILE: &str = "default.yaml";
const API_HELPER_FILE: &str = "apiHelper.py";
const RESOURCES: &str = "resources";
const SCRIPTS: &str = "scripts";

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let cargo_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    fs::copy(
        Path::new(&cargo_dir).join(RESOURCES).join(CONFIG_FILE),
        Path::new(&out_dir).join("../../..").join(CONFIG_FILE),
    )
    .unwrap();
    fs::copy(
        Path::new(&cargo_dir)
            .join(RESOURCES)
            .join(SCRIPTS)
            .join(API_HELPER_FILE),
        Path::new(&out_dir).join("../../..").join(API_HELPER_FILE),
    )
    .unwrap();
}
