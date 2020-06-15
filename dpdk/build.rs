use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let dst = out.as_path().display().to_string();
    println!("DPDK Output directory is {}", &dst);

    // Download and build dpdk
    let mut cmd = Command::new("./build.sh");
    cmd.arg(&dst);
    if let Err(status) = cmd.status() {
        println!("dpdk build failed with {}", status);
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build.sh");
}
