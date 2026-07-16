use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide the manifest directory"),
    );
    let license = manifest_dir.join("third_party/fira_mono/LICENSE.txt");
    println!("cargo::rerun-if-changed={}", license.display());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR"));
    let executable_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR must be inside the Cargo profile directory");
    fs::copy(license, executable_dir.join("licenses.txt"))
        .expect("the Fira Mono license must be copied alongside the game executable");
}
