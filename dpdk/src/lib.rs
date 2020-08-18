use dpdk_ffi::{
    dpdk_rx_one, dpdk_tx_one, rte_dev_iterator, rte_dev_probe, rte_eal_init,
    rte_eal_mp_remote_launch, rte_eth_conf, rte_eth_dev_configure, rte_eth_dev_socket_id,
    rte_eth_dev_start, rte_eth_iterator_init, rte_eth_iterator_next,
    rte_eth_rx_mq_mode_ETH_MQ_RX_NONE, rte_eth_rx_queue_setup, rte_eth_tx_queue_setup, rte_mbuf,
    rte_mempool, rte_pktmbuf_pool_create, rte_rmt_call_master_t_SKIP_MASTER, RTE_MAX_ETHPORTS,
    RTE_MEMPOOL_CACHE_MAX_SIZE, SOCKET_ID_ANY,
};
use graph::Driver;
use packet::BoxPkt;
use std::ffi::CString;
use std::mem;

// TODO: These are to be made configurable at some point
const N_RX_DESC: u16 = 128;
const N_TX_DESC: u16 = 128;

fn pkt_to_mbuf(pkt: &BoxPkt) -> *mut rte_mbuf {
    unsafe { 0 as *mut rte_mbuf }
}

pub struct Dpdk {
    port: usize,
}

impl Driver for Dpdk {
    fn fd(&self) -> Option<i32> {
        None
    }

    fn recvmsg(&self, pkt: &mut BoxPkt) {
        let mut mbuf = pkt_to_mbuf(pkt);
        let nrx = dpdk_rx_one(self.port, 0, &mut mbuf);
    }

    fn sendmsg(&self, pkt: BoxPkt) -> usize {
        let len = pkt.len();
        let mut mbuf = pkt_to_mbuf(&pkt);
        dpdk_tx_one(self.port, 0, &mut mbuf);
        len
    }
}
pub enum port_init_err {
    PROBE_FAIL,
    CONFIG_FAIL,
    QUEUE_FAIL,
    START_FAIL,
}
fn get_opt(opt: &str) -> *const libc::c_char {
    let cstr = CString::new(opt).unwrap();
    let ptr = cstr.as_ptr();
    mem::forget(cstr);
    ptr
}

fn dpdk_init(_mem_sz: usize, _ncores: usize) -> Result<(), usize> {
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
        if rte_eal_init(argv_len, argv_ptr) < 0 {
            return Err(0);
        }
        Ok(())
    }
}

extern "C" fn dpdk_thread(arg: *mut core::ffi::c_void) -> i32 {
    0
}

fn dpdk_launch() {
    unsafe {
        rte_eal_mp_remote_launch(
            Some(dpdk_thread),
            0 as *mut core::ffi::c_void,
            rte_rmt_call_master_t_SKIP_MASTER,
        );
    }
}

fn dpdk_buffer_init(total_mem: usize, priv_sz: usize, buf_sz: usize) -> *mut rte_mempool {
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

fn dpdk_port_cfg(port: u16) -> Result<(), port_init_err> {
    unsafe {
        let mut cfg: rte_eth_conf = mem::MaybeUninit::uninit().assume_init();
        cfg.rxmode.mq_mode = rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
        if rte_eth_dev_configure(port, 1, 1, &mut cfg) < 0 {
            return Err(port_init_err::CONFIG_FAIL);
        }
    }
    Ok(())
}

fn dpdk_queue_cfg(
    port: u16,
    n_rxd: u16,
    n_txd: u16,
    pool: *mut rte_mempool,
) -> Result<(), port_init_err> {
    unsafe {
        let socket = rte_eth_dev_socket_id(port);
        if socket < 0 {
            return Err(port_init_err::CONFIG_FAIL);
        }
        let ret = rte_eth_rx_queue_setup(
            port,
            0,
            N_RX_DESC,
            socket as u32,
            0 as *const dpdk_ffi::rte_eth_rxconf,
            pool,
        );
        if ret != 0 {
            return Err(port_init_err::QUEUE_FAIL);
        }
        let ret = rte_eth_tx_queue_setup(
            port,
            0,
            N_TX_DESC,
            socket as u32,
            0 as *const dpdk_ffi::rte_eth_txconf,
        );
        if ret != 0 {
            return Err(port_init_err::QUEUE_FAIL);
        }
    }
    Ok(())
}

fn dpdk_port_probe(intf: &str, af_idx: isize) -> Result<u16, port_init_err> {
    let mut port: u16 = RTE_MAX_ETHPORTS as u16;
    let params = format!(
        "eth_af_packet{},iface={},blocksz=4096,framesz=2048,framecnt=2048,qpairs=1",
        af_idx, intf
    );
    let cstr = CString::new(params).unwrap();
    let args = cstr.as_ptr();
    unsafe {
        if rte_dev_probe(args) == 0 {
            let mut iter: rte_dev_iterator = mem::MaybeUninit::uninit().assume_init();
            rte_eth_iterator_init(&mut iter, args);
            let mut id = rte_eth_iterator_next(&mut iter);
            while id != RTE_MAX_ETHPORTS as u16 {
                port = id;
                id = rte_eth_iterator_next(&mut iter);
            }
        } else {
            return Err(port_init_err::PROBE_FAIL);
        }
        if port == RTE_MAX_ETHPORTS as u16 {
            return Err(port_init_err::PROBE_FAIL);
        }
    }
    Ok(port)
}

fn dpdk_af_packet_init(
    intf: &str,
    af_idx: isize,
    pool: *mut rte_mempool,
) -> Result<u16, port_init_err> {
    let mut port: u16 = RTE_MAX_ETHPORTS as u16;
    unsafe {
        match dpdk_port_probe(intf, af_idx) {
            Err(err) => return Err(err),
            Ok(p) => port = p,
        };
        if let Err(err) = dpdk_port_cfg(port) {
            return Err(err);
        }
        if let Err(err) = dpdk_queue_cfg(port, N_RX_DESC, N_TX_DESC, pool) {
            return Err(err);
        }
        if rte_eth_dev_start(port) < 0 {
            return Err(port_init_err::START_FAIL);
        }
    }
    Ok(port)
}

#[cfg(test)]
mod test;
