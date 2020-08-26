#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!("bindgen/include/lib.rs");

// Included below are APIs which we would have 'liked' bindgen to create a
// binding for but it did not, for whatever reason. One common reason being
// that the API was declared as inline. Obviously this has the issue that if
// later dpdk versions change how this is done, then we have to come back and
// update this

// This is rte_eth_rx_burst() which is declared as inline and hence bindgen
// does not generate the bindings.
pub fn dpdk_rx_one(port_id: u16, queue_id: usize, mbuf: *mut *mut rte_mbuf) -> u16 {
    unsafe {
        let dev = &rte_eth_devices[port_id as usize];
        let cb = dev.rx_pkt_burst.unwrap();
        let ptr = (*dev.data).rx_queues.add(queue_id);
        cb(*ptr, mbuf, 1)
    }
}

// This is rte_eth_tx_burst() which is declared as inline and hence bindgen
// does not generate the bindings.
pub fn dpdk_tx_one(port_id: u16, queue_id: usize, mbuf: *mut *mut rte_mbuf) {
    unsafe {
        let dev = &rte_eth_devices[port_id as usize];
        let cb = dev.tx_pkt_burst.unwrap();
        let ptr = (*dev.data).tx_queues.add(queue_id);
        cb(*ptr, mbuf, 1);
    }
}

// This is rte_pktmbuf_alloc() which is declared as inline and hence bindgen
// does not generate the bindings. The original rte_pktmbuf_alloc() has cache
// allocation etc. which is ignored below, it directly goes to the pool
pub fn dpdk_mbuf_free(m: *mut rte_mbuf) {
    unsafe {
        let mbuf: *mut core::ffi::c_void = m as *mut core::ffi::c_void;
        let mp: *mut rte_mempool = (*m).pool;
        let ops: *mut rte_mempool_ops = &mut rte_mempool_ops_table.ops[(*mp).ops_index as usize];
        let cb = (*ops).enqueue.unwrap();
        cb(mp, &mbuf, 1);
    }
}

// This is rte_pktmbuf_free() which is declared as inline and hence bindgen
// does not generate the bindings. The original rte_pktmbuf_free() has cache
// allocation etc. which is ignored below, it directly goes to the pool
pub fn dpdk_mbuf_alloc(mp: *mut rte_mempool) -> Option<*mut rte_mbuf> {
    unsafe {
        let mut m: *mut libc::c_void = 0 as *mut libc::c_void;
        let ops: *mut rte_mempool_ops = &mut rte_mempool_ops_table.ops[(*mp).ops_index as usize];
        let cb = (*ops).dequeue.unwrap();
        cb(mp, &mut m, 1);
        Some(m as *mut rte_mbuf)
    }
}

// This is rte_pktmbuf_mtod_offset(), bindgen not generated because of inline
pub fn dpdk_mtod(m: *mut rte_mbuf) -> *mut u8 {
    unsafe {
        let addr: *mut u8 = (*m).buf_addr as *mut u8;
        addr.add((*m).data_off as usize)
    }
}
