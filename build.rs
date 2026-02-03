// build.rs

use std::env;
use std::fs;
use std::path::Path;

const CONFIG_PATH: &str = "configs";
const API_HELPER_FILE: &str = "apiHelper.py";
const RESOURCES: &str = "resources";
const SCRIPTS: &str = "scripts";

fn main() {
    // If we set CARGO_PKG_VERSION this way, then it will override the default value, which is
    // taken from the `version` in Cargo.toml.
    if let Ok(val) = std::env::var("ODYSSEY_RELEASE_VERSION") {
        println!("cargo:rustc-env=CARGO_PKG_VERSION={}", val);
    }
    println!("cargo:rerun-if-env-changed=ODYSSEY_RELEASE_VERSION");

    println!(
        "cargo:rustc-env=CARGO_COMPILE_TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let cargo_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();

    let config_dest = Path::new(&out_dir).join("../../..").join(CONFIG_PATH);
    let config_src = Path::new(&cargo_dir).join(RESOURCES).join(CONFIG_PATH);
    fs::create_dir_all(&config_dest).expect("Expect destination configs dir to be created");
    for entry in fs::read_dir(config_src).expect("Expect configs dir to exists") {
        let entry = entry.expect("Expect config file to exist");
        let ty = entry.file_type().expect("Expect filetype");
        if ty.is_dir() {
            continue;
        } else {
            fs::copy(entry.path(), config_dest.join(entry.file_name())).expect("Expect config file to be copies");
        }
    }

    fs::copy(
        Path::new(&cargo_dir)
            .join(RESOURCES)
            .join(SCRIPTS)
            .join(API_HELPER_FILE),
        Path::new(&out_dir).join("../../..").join(API_HELPER_FILE),
    )
    .unwrap();
}
