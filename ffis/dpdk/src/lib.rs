#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!("bindgen/include/lib.rs");

// Included below are APIs which we would have 'liked' bindgen to create a
// binding for but it did not, for whatever reason. One common reason being
// that the API was declared as inline

// This is rte_eth_rx_burst() which is declared as inline and hence bindgen
// does not generate the bindings. Obviously this has the issue that if later
// dpdk versions change how this is done, then we have to come back and update
// this
pub fn dpdk_rx_one(port_id: usize, queue_id: usize, mbuf: *mut *mut rte_mbuf) {
    unsafe {
        let dev = &rte_eth_devices[port_id];
        let cb = dev.rx_pkt_burst.unwrap();
        let ptr = (*dev.data).rx_queues.add(queue_id);
        cb(*ptr, mbuf, 1);
    }
}

// This is rte_eth_tx_burst() which is declared as inline and hence bindgen
// does not generate the bindings. Obviously this has the issue that if later
// dpdk versions change how this is done, then we have to come back and update
// this
pub fn dpdk_tx_one(port_id: usize, queue_id: usize, mbuf: *mut *mut rte_mbuf) {
    unsafe {
        let dev = &rte_eth_devices[port_id];
        let cb = dev.tx_pkt_burst.unwrap();
        let ptr = (*dev.data).tx_queues.add(queue_id);
        cb(*ptr, mbuf, 1);
    }
}
