use counters::flavors::{Counter, CounterType};
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use dpdk_ffi::{
    bindgen::lcore_function_t, bindgen::rte_dev_iterator, bindgen::rte_dev_probe,
    bindgen::rte_eal_init, bindgen::rte_eal_remote_launch, bindgen::rte_eth_conf,
    bindgen::rte_eth_dev_configure, bindgen::rte_eth_dev_socket_id, bindgen::rte_eth_dev_start,
    bindgen::rte_eth_iterator_init, bindgen::rte_eth_iterator_next,
    bindgen::rte_eth_rx_mq_mode_ETH_MQ_RX_NONE, bindgen::rte_eth_rx_queue_setup,
    bindgen::rte_eth_rxconf, bindgen::rte_eth_tx_mq_mode_ETH_MQ_TX_NONE,
    bindgen::rte_eth_tx_queue_setup, bindgen::rte_eth_txconf, bindgen::rte_mbuf,
    bindgen::rte_mempool, bindgen::rte_mempool_obj_iter, bindgen::rte_pktmbuf_pool_create,
    bindgen::RTE_MAX_ETHPORTS, bindgen::RTE_PKTMBUF_HEADROOM, bindgen::SOCKET_ID_ANY,
    dpdk_mbuf_alloc, dpdk_mbuf_free, dpdk_rx_one, dpdk_tx_one,
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
    pub dpdk_pool: *mut rte_mempool,
    alloc_fail: Counter,
    pkts: VecDeque<BoxPkt>,
    particle_sz: usize,
}

// The *mut rte_pool prevents a send, but we *know* that we are sending this from control thread that
// creates the pool to data thread that then uses it, and the pools are valid no matter which thread
// accesses it
unsafe impl Send for PktsDpdk {}

// NOTE: As of today R2 supports only single buffer dpdk packets. It is not very hard to extend
// support to multi buffer mbufs, just postponing that to when its required.

// The format of the mbuf is as below.
// [[struct rte_mbuf][headroom][data area]]
// The mbuf->buf_addr for a newly allocated mbuf will point to the start of the [headroom] area,
// ie right after the struct rte_mbuf. Note that headroom + [data area] is equal to partcle_sz().
// On receiving a packet from dpdk, usually the data is written starting at the section [data area],
// ie the headroom is left intact for applications to "append" data at the head of the mbuf.
//
// The headroom itself is as below
// [mbuf-address R2-Particle-addres ...rest of headroom..]
// So the first two words of the headroom are stolen by R2 to store the mbuf structure address
// itself, and address of the in-heap Particle structure, so the real available headroom is less
// by two words
const MBUFPTR_SZ: usize = mem::size_of::<*mut rte_mbuf>();
const PARTPTR_SZ: usize = mem::size_of::<*mut u8>();
const HEADROOM_STEAL: usize = MBUFPTR_SZ + PARTPTR_SZ;
const HEADROOM: usize = RTE_PKTMBUF_HEADROOM as usize - HEADROOM_STEAL;

// For each mbuf, fill up the first two words in the headroom with the mbuf ptr and particle ptr
unsafe extern "C" fn dpdk_init_mbuf(
    _mp: *mut rte_mempool,
    _opaque: *mut core::ffi::c_void,
    mbuf: *mut core::ffi::c_void,
    _index: u32,
) {
    let m: *mut rte_mbuf = mbuf as *mut rte_mbuf;
    let lpart = Layout::from_size_align(BoxPart::size(), BoxPart::align()).unwrap();
    let part: *mut u8 = alloc(lpart);
    assert_ne!(part, std::ptr::null_mut::<u8>());
    let mut mbufptr: *mut *mut rte_mbuf = (*m).buf_addr as *mut *mut rte_mbuf;
    *mbufptr = m;
    mbufptr = mbufptr.add(1);
    let partptr: *mut *mut u8 = mbufptr as *mut *mut u8;
    *partptr = part;
}

impl PktsDpdk {
    pub fn new(
        name: &str,
        queue: Arc<ArrayQueue<BoxPkt>>,
        counters: &mut Counters,
        num_pkts: usize,
        num_parts: usize,
        particle_sz: usize,
    ) -> Self {
        assert!(num_parts >= num_pkts);
        let pkts = VecDeque::with_capacity(num_pkts);
        let alloc_fail = Counter::new(counters, name, CounterType::Error, "PktAllocFail");
        let dpdk_pool = dpdk_buffer_init(name, num_parts as u32, particle_sz as u16);
        let mut pool = PktsDpdk {
            dpdk_pool,
            alloc_fail,
            pkts,
            particle_sz,
        };
        assert_ne!(dpdk_pool, std::ptr::null_mut::<rte_mempool>());
        unsafe {
            for _ in 0..num_pkts {
                let lpkt = Layout::from_size_align(BoxPkt::size(), BoxPkt::align()).unwrap();
                let pkt: *mut u8 = alloc(lpkt);
                assert_ne!(pkt, std::ptr::null_mut::<u8>());
                pool.pkts.push_front(BoxPkt::new(pkt, queue.clone()));
            }
            rte_mempool_obj_iter(
                dpdk_pool,
                Some(dpdk_init_mbuf),
                std::ptr::null_mut::<core::ffi::c_void>(),
            );
        }
        pool
    }
}

fn mbuf_to_raw(m: *mut rte_mbuf) -> *mut u8 {
    unsafe {
        let raw = (*m).buf_addr as u64;
        (raw + HEADROOM_STEAL as u64) as *mut u8
    }
}

impl PacketPool for PktsDpdk {
    fn pkt(&mut self, headroom: usize) -> Option<BoxPkt> {
        assert!(headroom <= HEADROOM);
        if let Some(p) = self.particle(headroom) {
            if let Some(mut pkt) = self.pkts.pop_front() {
                pkt.reinit(p);
                Some(pkt)
            } else {
                self.free_part(p);
                None
            }
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn particle(&mut self, headroom: usize) -> Option<BoxPart> {
        assert!(headroom <= HEADROOM);
        if let Some(m) = dpdk_mbuf_alloc(self.dpdk_pool) {
            unsafe {
                let mut mbufptr: *mut *mut rte_mbuf = (*m).buf_addr as *mut *mut rte_mbuf;
                mbufptr = mbufptr.add(1);
                let partptr: *mut *mut u8 = mbufptr as *mut *mut u8;
                let part = BoxPart::new(*partptr, mbuf_to_raw(m), self.particle_sz());
                Some(part)
            }
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn free_pkt(&mut self, pkt: BoxPkt) {
        assert!(!pkt.has_part());
        self.pkts.push_front(pkt);
    }

    fn free_part(&mut self, part: BoxPart) {
        assert!(!part.has_next());
        unsafe {
            let mut partptr: *const *mut u8 = part.data_raw(0).as_ptr() as *const *mut u8;
            partptr = partptr.sub(1);
            let mut mbufptr: *const *mut rte_mbuf = partptr as *const *mut rte_mbuf;
            mbufptr = mbufptr.sub(1);
            let m: *mut rte_mbuf = *mbufptr;
            dpdk_mbuf_free(m);
        }
    }

    fn particle_sz(&self) -> usize {
        self.particle_sz
    }

    fn pkt_with_particles(&mut self, part: BoxPart) -> Option<BoxPkt> {
        if let Some(mut pkt) = self.pkts.pop_front() {
            pkt.reinit(part);
            Some(pkt)
        } else {
            self.alloc_fail.incr();
            self.free_part(part);
            None
        }
    }

    fn opaque(&self) -> u64 {
        self.dpdk_pool as u64
    }
}

pub enum DpdkHw {
    AfPacket,
    PCI,
}

pub struct Params<'a> {
    pub name: &'a str,
    pub hw: DpdkHw,
}

pub struct Dpdk {
    port: u16,
    init_done: bool,
    init_fail: Counter,
    no_pkts: Counter,
    send_err: Counter,
    recv_err: Counter,
}

#[derive(Default)]
pub struct DpdkGlobal {
    index: u16,
}

impl DpdkGlobal {
    pub fn new(mem_sz: usize, ncores: usize) -> Self {
        if let Err(err) = dpdk_init(mem_sz, ncores) {
            panic!("DPDK Init failed {}", err);
        }
        DpdkGlobal { index: 0 }
    }

    pub fn add(&mut self, counters: &mut Counters, params: Params) -> Result<Dpdk, PortInitErr> {
        let index = self.index;
        let name = format!("InitErr_{}", params.name);
        let init_fail = Counter::new(counters, "DpdkIfnode", CounterType::Pkts, &name);
        let name = format!("NoPkts_{}", params.name);
        let no_pkts = Counter::new(counters, "DpdkIfnode", CounterType::Pkts, &name);
        let name = format!("SendErr_{}", params.name);
        let send_err = Counter::new(counters, "DpdkIfnode", CounterType::Pkts, &name);
        let name = format!("RecvErr_{}", params.name);
        let recv_err = Counter::new(counters, "DpdkIfnode", CounterType::Pkts, &name);
        match params.hw {
            DpdkHw::AfPacket => match dpdk_af_packet_init(params.name, index) {
                Ok(port) => {
                    self.index += 1;
                    Ok(Dpdk {
                        port,
                        init_done: false,
                        init_fail,
                        no_pkts,
                        send_err,
                        recv_err,
                    })
                }
                Err(err) => Err(err),
            },
            _ => Err(PortInitErr::UnknownHw),
        }
    }
}

impl Dpdk {
    fn init(&mut self, pool: &mut dyn PacketPool) -> Result<(), PortInitErr> {
        let mbuf_pool = pool.opaque() as *mut rte_mempool;
        unsafe {
            dpdk_queue_cfg(self.port, N_RX_DESC, N_TX_DESC, mbuf_pool)?;
            if rte_eth_dev_start(self.port) < 0 {
                return Err(PortInitErr::StartFail);
            }
            Ok(())
        }
    }
}

impl Driver for Dpdk {
    fn fd(&self) -> Option<i32> {
        None
    }

    fn recvmsg(&mut self, pool: &mut dyn PacketPool, headroom: usize) -> Option<BoxPkt> {
        // The port has to be initialized with pools AFTER the node is added to the specific graph,
        // because the control thread does not have access to the pools once the graph thread is created.
        // We could have added some kind of node-init() API per node rather than doing it like this here,
        // thats something to be considered for future if more nodes need a run time init
        if !self.init_done {
            if self.init(pool).is_err() {
                self.init_fail.add(1);
                return None;
            }
            self.init_done = true;
        }

        if headroom > HEADROOM {
            return None;
        }
        let mut m: *mut rte_mbuf = std::ptr::null_mut::<rte_mbuf>();
        let nrx = dpdk_rx_one(self.port, 0, &mut m);
        if nrx == 0 {
            None
        } else {
            unsafe {
                let mut mbufptr: *mut *mut rte_mbuf = (*m).buf_addr as *mut *mut rte_mbuf;
                assert_eq!(*mbufptr, m);
                mbufptr = mbufptr.add(1);
                let partptr: *mut *mut u8 = mbufptr as *mut *mut u8;
                let mut part = BoxPart::new(*partptr, mbuf_to_raw(m), pool.particle_sz());
                assert_eq!((*m).data_off, RTE_PKTMBUF_HEADROOM as u16);
                // Remember, the first two words from mbuf->buf_addr are used up for storing
                // mbuf pointer and particle pointer, and the particle data starts after that.
                // So the headroom is lower than dpdk headroom by those two words
                part.reinit(HEADROOM);
                if let Some(mut pkt) = pool.pkt_with_particles(part) {
                    let len = ((*m).data_len as usize) as isize;
                    if pkt.move_tail(len) == len {
                        Some(pkt)
                    } else {
                        self.recv_err.add(1);
                        None
                    }
                } else {
                    self.no_pkts.add(1);
                    None
                }
            }
        }
    }

    fn sendmsg(&mut self, pool: &mut dyn PacketPool, mut pkt: BoxPkt) -> usize {
        // The port has to be initialized with pools AFTER the node is added to the specific graph,
        // because the control thread does not have access to the pools once the graph thread is created.
        // We could have added some kind of node-init() API per node rather than doing it like this here,
        // thats something to be considered for future if more nodes need a run time init
        if !self.init_done {
            if self.init(pool).is_err() {
                self.init_fail.add(1);
                return 0;
            }
            self.init_done = true;
        }

        unsafe {
            let m = pkt.head_mut().as_mut_ptr();
            let mut partptr: *mut *mut u8 = m as *mut *mut u8;
            partptr = partptr.sub(1);
            let mut mbufptr: *mut *mut rte_mbuf = partptr as *mut *mut rte_mbuf;
            mbufptr = mbufptr.sub(1);
            let mut mbuf: *mut rte_mbuf = *mbufptr;
            let data = pkt.data(0).unwrap().0.as_ptr() as u64;
            let head = (*mbuf).buf_addr as u64;
            (*mbuf).data_off = (data - head) as u16;
            let len = pkt.len();
            (*mbuf).data_len = len as u16;
            (*mbuf).pkt_len = len as u32;
            // As soon as pkt goes out of scope, rust will free it, so we need to bump up refcnt
            // so that dpdk still has valid mbuf with it. We are not using any atomic ops here
            // because we use one pool per thread.
            (*mbuf).__bindgen_anon_2.refcnt_atomic.cnt += 1;
            if dpdk_tx_one(self.port, 0, &mut mbuf) != 1 {
                dpdk_mbuf_free(mbuf);
                self.send_err.add(1);
                0
            } else {
                len
            }
        }
    }
}

#[derive(Debug)]
pub enum PortInitErr {
    ProbeFail,
    ConfigFail,
    QueueFail,
    StartFail,
    UnknownHw,
}
fn get_opt(opt: &str) -> *const libc::c_char {
    let cstr = CString::new(opt).unwrap();
    let ptr = cstr.as_ptr();
    mem::forget(cstr);
    ptr
}

pub fn dpdk_init(mem_sz: usize, ncores: usize) -> Result<(), i32> {
    let mut lcores = "--lcores=0".to_string();
    for c in 1..ncores {
        lcores.push(',');
        lcores.push_str(&c.to_string());
    }
    let mut argv = vec![
        get_opt("r2"),
        get_opt("-m"),
        get_opt(&format!("{}", mem_sz)),
        get_opt("--no-huge"),
        get_opt("--no-pci"),
        get_opt(&lcores),
        get_opt("--master-lcore=0"),
        //get_opt("--log-level=*:8"),
    ];
    unsafe {
        let argv_ptr = argv.as_mut_ptr() as *mut *mut libc::c_char;
        let argv_len = argv.len() as libc::c_int;
        // DPDK option parsing can end up modifying the argv array and
        // duplicating entries etc. Leaking this memory intentionally to
        // avoid dealing with what dpdk does inside with the argv
        mem::forget(argv);
        let ret = rte_eal_init(argv_len, argv_ptr);
        if ret < 0 {
            return Err(ret);
        }
        Ok(())
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn dpdk_launch(core: usize, dpdk_thread: lcore_function_t, arg: *mut core::ffi::c_void) {
    unsafe {
        rte_eal_remote_launch(dpdk_thread, arg, core as u32);
    }
}

pub fn dpdk_buffer_init(name: &str, nbufs: u32, buf_sz: u16) -> *mut rte_mempool {
    let cstr = CString::new(name).unwrap();
    let name = cstr.as_ptr();
    mem::forget(cstr);
    unsafe { rte_pktmbuf_pool_create(name, nbufs, 0, 0, buf_sz, SOCKET_ID_ANY) }
}

fn dpdk_port_cfg(port: u16) -> Result<(), PortInitErr> {
    unsafe {
        let mut cfg: rte_eth_conf = mem::MaybeUninit::zeroed().assume_init();
        cfg.rxmode.mq_mode = rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
        cfg.txmode.mq_mode = rte_eth_tx_mq_mode_ETH_MQ_TX_NONE;
        if rte_eth_dev_configure(port, 1, 1, &cfg) < 0 {
            return Err(PortInitErr::ConfigFail);
        }
    }
    Ok(())
}

fn dpdk_queue_cfg(
    port: u16,
    n_rxd: u16,
    n_txd: u16,
    pool: *mut rte_mempool,
) -> Result<(), PortInitErr> {
    unsafe {
        let ret = rte_eth_rx_queue_setup(
            port,
            0,
            n_rxd,
            rte_eth_dev_socket_id(port) as u32,
            std::ptr::null::<rte_eth_rxconf>(),
            pool,
        );
        if ret != 0 {
            return Err(PortInitErr::QueueFail);
        }
        let ret = rte_eth_tx_queue_setup(
            port,
            0,
            n_txd,
            rte_eth_dev_socket_id(port) as u32,
            std::ptr::null::<rte_eth_txconf>(),
        );
        if ret != 0 {
            return Err(PortInitErr::QueueFail);
        }
    }
    Ok(())
}

fn dpdk_port_probe(intf: &str, af_idx: u16) -> Result<u16, PortInitErr> {
    let mut port: u16 = RTE_MAX_ETHPORTS as u16;
    let params = format!("eth_af_packet{},iface={}", af_idx, intf);
    let cstr = CString::new(params).unwrap();
    let args = cstr.as_ptr();
    unsafe {
        if rte_dev_probe(args) == 0 {
            let mut iter: rte_dev_iterator = mem::MaybeUninit::zeroed().assume_init();
            rte_eth_iterator_init(&mut iter, args);
            let mut id = rte_eth_iterator_next(&mut iter);
            while id != RTE_MAX_ETHPORTS as u16 {
                port = id;
                id = rte_eth_iterator_next(&mut iter);
            }
        } else {
            return Err(PortInitErr::ProbeFail);
        }
        if port == RTE_MAX_ETHPORTS as u16 {
            return Err(PortInitErr::ProbeFail);
        }
    }
    Ok(port)
}

fn dpdk_af_packet_init(intf: &str, af_idx: u16) -> Result<u16, PortInitErr> {
    let port: u16 = dpdk_port_probe(intf, af_idx)?;
    dpdk_port_cfg(port)?;
    Ok(port)
}

#[cfg(test)]
mod test;
