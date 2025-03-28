#[cfg(feature = "dpdk")]
mod dpdk;

#[cfg(not(feature = "dpdk"))]
mod stubs;

// Re-export the APIs so main and foobar don't need to care about stubs.rs
#[cfg(feature = "dpdk")]
pub use dpdk::*;

#[cfg(not(feature = "dpdk"))]
pub use stubs::*;

pub enum DpdkHw {
    AfPacket,
    PCI,
}

pub struct Params<'a> {
    pub name: &'a str,
    pub hw: DpdkHw,
}

#[derive(Debug)]
pub enum PortInitErr {
    ProbeFail,
    ConfigFail,
    QueueFail,
    StartFail,
    UnknownHw,
}
