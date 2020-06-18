use std::ffi::{CString};
use std::mem;

extern "C" {
    fn rte_eal_init(argc: libc::c_int, argv: *mut *const libc::c_char) -> libc::c_int;
}

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
        let argv_ptr = argv.as_mut_ptr() as *mut *const libc::c_char;
        let argv_len = argv.len() as libc::c_int;
        // DPDK option parsing can end up modifying the argv array and 
        // duplicating entries etc. Leaking this memory intentionally to
        // avoid dealing with what dpdk does inside with the argv
        mem::forget(argv);
        rte_eal_init(argv_len, argv_ptr)
    }
}

#[cfg(test)]
mod test;
