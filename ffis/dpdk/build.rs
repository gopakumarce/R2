use std::env;

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    println!("DPDK Output directory is {}", &out);

    println!("cargo:rustc-link-lib=dylib=rte_eal");
    println!("cargo:rustc-link-lib=dylib=rte_kvargs");
    println!("cargo:rustc-link-lib=dylib=rte_mempool");
    println!("cargo:rustc-link-lib=dylib=rte_mbuf");
    println!("cargo:rustc-link-lib=dylib=rte_ethdev");
    println!("cargo:rerun-if-changed=build.rs");
}
