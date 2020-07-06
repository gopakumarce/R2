use dpdk_ffi::{
    rte_eal_init, rte_mempool, rte_pktmbuf_pool_create, RTE_MEMPOOL_CACHE_MAX_SIZE, SOCKET_ID_ANY,
};
use std::ffi::CString;
use std::mem;

fn get_opt(opt: &str) -> *const libc::c_char {
    let cstr = CString::new(opt).unwrap();
    let ptr = cstr.as_ptr();
    mem::forget(cstr);
    ptr
}

fn dpdk_init(_mem_sz: usize, _ncores: usize) -> i32 {
    let mut argv = vec![
        get_opt("r2"),
        get_opt("-m"),
        get_opt("128"),
        get_opt("--no-huge"),
        get_opt("--no-pci"),
        get_opt("--lcores=0"),
        get_opt("--master-lcore=0"),
    ];
    unsafe {
        let argv_ptr = argv.as_mut_ptr() as *mut *mut libc::c_char;
        let argv_len = argv.len() as libc::c_int;
        // DPDK option parsing can end up modifying the argv array and
        // duplicating entries etc. Leaking this memory intentionally to
        // avoid dealing with what dpdk does inside with the argv
        mem::forget(argv);
        rte_eal_init(argv_len, argv_ptr)
    }
}

fn dpdk_buffer_init(total_mem: usize, priv_sz: usize, buf_sz: usize) -> *const rte_mempool {
    let nbufs = (total_mem / buf_sz) as u32;
    let cstr = CString::new("dpdk_mbufs").unwrap();
    let name = cstr.as_ptr();
    mem::forget(cstr);
    unsafe {
        rte_pktmbuf_pool_create(
            name,
            nbufs,
            RTE_MEMPOOL_CACHE_MAX_SIZE,
            priv_sz as u16,
            buf_sz as u16,
            SOCKET_ID_ANY,
        )
    }
}

#[cfg(test)]
mod test;
