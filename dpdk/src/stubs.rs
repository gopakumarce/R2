use super::*;
use core::panic;
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use graph::Driver;
use packet::{BoxPart, BoxPkt, PacketPool};
use std::sync::Arc;

pub struct Dpdk {}

#[derive(Default)]
pub struct DpdkGlobal {}

pub struct PktsDpdk {}

unsafe impl Send for PktsDpdk {}

impl PktsDpdk {
    pub fn new(
        _: &str,
        _: Arc<ArrayQueue<BoxPkt>>,
        _: &mut Counters,
        _: usize,
        _: usize,
        _: usize,
    ) -> Self {
        panic!("DPDK feature not compiled in");
    }
}

impl PacketPool for PktsDpdk {
    fn pkt(&mut self, _: usize) -> Option<BoxPkt> {
        panic!("DPDK feature not compiled in");
    }

    fn particle(&mut self, _: usize) -> Option<BoxPart> {
        panic!("DPDK feature not compiled in");
    }

    fn free_pkt(&mut self, _: BoxPkt) {
        panic!("DPDK feature not compiled in");
    }

    fn free_part(&mut self, _: BoxPart) {
        panic!("DPDK feature not compiled in");
    }

    fn particle_sz(&self) -> usize {
        panic!("DPDK feature not compiled in");
    }

    fn pkt_with_particles(&mut self, _: BoxPart) -> Option<BoxPkt> {
        panic!("DPDK feature not compiled in");
    }

    fn opaque(&self) -> u64 {
        panic!("DPDK feature not compiled in");
    }
}

impl DpdkGlobal {
    pub fn new(_: usize, _: usize) -> Self {
        panic!("DPDK feature not compiled in");
    }

    pub fn add(&mut self, _: &mut Counters, _: Params) -> Result<Dpdk, PortInitErr> {
        Err(PortInitErr::UnknownHw)
    }
}

impl Driver for Dpdk {
    fn fd(&self) -> Option<i32> {
        panic!("DPDK feature not compiled in");
    }

    fn recvmsg(&mut self, _: &mut dyn PacketPool, _: usize) -> Option<BoxPkt> {
        panic!("DPDK feature not compiled in");
    }

    fn sendmsg(&mut self, _: &mut dyn PacketPool, _: BoxPkt) -> usize {
        panic!("DPDK feature not compiled in");
    }
}

pub fn dpdk_init(_: usize, _: usize) -> Result<(), i32> {
    panic!("DPDK feature not compiled in");
}

pub type LcoreFunctionT = ::std::option::Option<
    unsafe extern "C" fn(arg1: *mut ::std::os::raw::c_void) -> ::std::os::raw::c_int,
>;

pub fn dpdk_launch(_: usize, _: LcoreFunctionT, _: *mut core::ffi::c_void) {
    panic!("DPDK feature not compiled in");
}
