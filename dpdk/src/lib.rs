use counters::flavors::{Counter, CounterType};
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use dpdk_ffi::{
    dpdk_mbuf_alloc, dpdk_mtod, dpdk_rx_one, dpdk_tx_one, rte_dev_iterator, rte_dev_probe,
    rte_eal_init, rte_eal_mp_remote_launch, rte_eth_conf, rte_eth_dev_configure,
    rte_eth_dev_socket_id, rte_eth_dev_start, rte_eth_iterator_init, rte_eth_iterator_next,
    rte_eth_rx_mq_mode_ETH_MQ_RX_NONE, rte_eth_rx_queue_setup, rte_eth_tx_queue_setup, rte_mbuf,
    rte_mempool, rte_pktmbuf_pool_create, rte_rmt_call_master_t_SKIP_MASTER, RTE_MAX_ETHPORTS,
    RTE_MEMPOOL_CACHE_MAX_SIZE, RTE_PKTMBUF_HEADROOM, SOCKET_ID_ANY,
};
use graph::Driver;
use packet::{BoxPart, BoxPkt, PacketPool};
use std::alloc::alloc;
use std::alloc::Layout;
use std::collections::VecDeque;
use std::ffi::CString;
use std::{mem, sync::Arc};

// TODO: These are to be made configurable at some point
const N_RX_DESC: u16 = 128;
const N_TX_DESC: u16 = 128;

pub struct PktsDpdk {
    dpdk_pool: *mut rte_mempool,
    alloc_fail: Counter,
    pkts: VecDeque<BoxPkt>,
    particles: VecDeque<BoxPart>,
    particle_sz: usize,
}

unsafe impl Send for PktsDpdk {}

impl PktsDpdk {
    const PARTICLE_ALIGN: usize = 16;

    /// #Safety
    /// This API deals with constructing packets and particles starting from raw pointers,
    /// hence this is marked unsafe
    pub fn new(
        queue: Arc<ArrayQueue<BoxPkt>>,
        counters: &mut Counters,
        num_pkts: usize,
        num_parts: usize,
        particle_sz: usize,
    ) -> Self {
        assert!(num_parts >= num_pkts);
        let parts_left = num_parts - num_pkts;
        let particles = VecDeque::with_capacity(parts_left);
        let pkts = VecDeque::with_capacity(num_pkts);
        let alloc_fail = Counter::new(counters, "PKTS_HEAP", CounterType::Error, "PktAllocFail");
        let dpdk_pool = dpdk_buffer_init(num_parts as u32, particle_sz as u16);
        let mut pool = PktsDpdk {
            dpdk_pool,
            alloc_fail,
            pkts,
            particles,
            particle_sz,
        };

        unsafe {
            for _ in 0..num_pkts {
                let lraw = Layout::from_size_align(particle_sz, Self::PARTICLE_ALIGN).unwrap();
                let raw: *mut u8 = alloc(lraw);
                let lpart = Layout::from_size_align(BoxPart::size(), BoxPart::align()).unwrap();
                let part: *mut u8 = alloc(lpart);
                let lpkt = Layout::from_size_align(BoxPkt::size(), BoxPkt::align()).unwrap();
                let pkt: *mut u8 = alloc(lpkt);
                pool.pkts.push_front(BoxPkt::new(
                    pkt,
                    BoxPart::new(part, raw, particle_sz),
                    queue.clone(),
                ));
            }

            for _ in 0..parts_left {
                let lpart = Layout::from_size_align(BoxPart::size(), BoxPart::align()).unwrap();
                let part: *mut u8 = alloc(lpart);
                pool.particles
                    .push_front(BoxPart::new(part, 0 as *mut u8, particle_sz));
            }
        }
        pool
    }

    fn mbuf_to_pkt(&mut self, mbuf: *mut rte_mbuf, headroom: usize) -> Option<BoxPkt> {
        if let Some(mut pkt) = self.pkts.pop_front() {
            unsafe {
                pkt.reinit_unsafe(headroom, dpdk_mtod(mbuf), self.particle_sz);
            }
            Some(pkt)
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn mbuf_to_particle(&mut self, mbuf: *mut rte_mbuf, headroom: usize) -> Option<BoxPart> {
        if let Some(mut part) = self.particles.pop_front() {
            unsafe {
                part.reinit_unsafe(headroom, dpdk_mtod(mbuf), self.particle_sz);
            }
            Some(part)
        } else {
            self.alloc_fail.incr();
            None
        }
    }
}

impl PacketPool for PktsDpdk {
    fn pkt(&mut self, headroom: usize) -> Option<BoxPkt> {
        assert!(headroom as u32 <= RTE_PKTMBUF_HEADROOM);
        if let Some(mbuf) = dpdk_mbuf_alloc(self.dpdk_pool) {
            self.mbuf_to_pkt(mbuf, headroom)
        } else {
            None
        }
    }

    fn particle(&mut self, headroom: usize) -> Option<BoxPart> {
        assert!(headroom as u32 <= RTE_PKTMBUF_HEADROOM);
        if let Some(mut mbuf) = dpdk_mbuf_alloc(self.dpdk_pool) {
            self.mbuf_to_particle(mbuf, headroom)
        } else {
            None
        }
    }

    fn free_pkt(&mut self, pkt: BoxPkt) {
        self.pkts.push_front(pkt);
    }

    fn free_part(&mut self, part: BoxPart) {
        self.particles.push_front(part);
    }

    fn particle_sz(&self) -> usize {
        self.particle_sz
    }
}

pub struct Dpdk {
    port: usize,
}

impl Driver for Dpdk {
    fn fd(&self) -> Option<i32> {
        None
    }

    fn recvmsg(&self, pool: &mut dyn PacketPool, headroom: usize) -> Option<BoxPkt> {
        unsafe {
            let mut mbuf = mem::MaybeUninit::uninit().assume_init();
            let nrx = dpdk_rx_one(self.port, 0, &mut mbuf);
            None
        }
    }

    fn sendmsg(&self, pkt: BoxPkt) -> usize {
        unsafe {
            let len = pkt.len();
            let mut mbuf = mem::MaybeUninit::uninit().assume_init();
            dpdk_tx_one(self.port, 0, &mut mbuf);
            len
        }
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

fn dpdk_buffer_init(nbufs: u32, buf_sz: u16) -> *mut rte_mempool {
    let cstr = CString::new("dpdk_mbufs").unwrap();
    let name = cstr.as_ptr();
    mem::forget(cstr);
    unsafe {
        rte_pktmbuf_pool_create(
            name,
            nbufs,
            RTE_MEMPOOL_CACHE_MAX_SIZE,
            0,
            buf_sz,
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
