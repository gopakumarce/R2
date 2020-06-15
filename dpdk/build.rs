use std::env;
use std::process::Command;

// TODO: Need meson, ninja, tar, curl, unxz, gcc/linker stuff
// Somehow check for presense of all this even before the build script
// runs and bail out if something is missing
fn main() {
    let out = env::var("OUT_DIR").unwrap();
    println!("DPDK Output directory is {}", &out);

    // Download and build dpdk
    let mut cmd = Command::new("./build.sh");
    cmd.arg(&out);
    if let Err(status) = cmd.status() {
        println!("dpdk build failed with {}", status);
        return;
    }

    println!(
        "cargo:rustc-link-search=native={}/{}",
        &out, "dpdk/build/lib/"
    );
    println!("cargo:rustc-link-lib=static=rte_eal");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build.sh");
}
