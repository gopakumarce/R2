extern "C" {
    fn rte_eal_init(argc: libc::c_int, argv: *const &libc::c_char) -> libc::c_int;
}

fn dpdk_init(_mem_sz: usize, _ncores: usize) -> i32 {
    let argv = vec![
        "r2",
        "-m",
        "128",
        "--no-huge",
        "--no-pci",
        "--lcores=0",
        "--master-lcore=0",
        "-c",
        "0x1",
    ];
    unsafe { rte_eal_init(argv.len() as libc::c_int, argv.as_ptr() as *const &i8) }
}

#[cfg(test)]
mod test;
