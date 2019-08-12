use super::*;
use std::net::Ipv4Addr;

pub const MAX_INTERFACES: usize = 4 * 1024;

pub struct Interface {
    pub ifname: String,
    pub ifindex: usize,
    pub bandwidth: usize,
    pub mtu: usize,
    pub ipv4_addr: Ipv4Addr,
    pub mask_len: u32,
    pub l2_addr: Vec<u8>,
    pub headroom: usize,
}

impl Interface {
    pub fn new(ifname: &str, ifindex: usize, l2_addr: Vec<u8>, headroom: usize) -> Interface {
        Interface {
            ifname: ifname.to_string(),
            ifindex,
            bandwidth: common::MB!(10 * 1024),
            mtu: ETHER_MTU,
            ipv4_addr: Ipv4Addr::new(0, 0, 0, 0),
            mask_len: 0,
            l2_addr,
            headroom,
        }
    }

    pub fn get_v4addr(&self) -> (Ipv4Addr, u32) {
        (self.ipv4_addr, self.mask_len)
    }

    pub fn set_v4addr(&mut self, addr: Ipv4Addr, mask_len: u32) {
        self.ipv4_addr = addr;
        self.mask_len = mask_len;
    }
}

impl Clone for Interface {
    fn clone(&self) -> Interface {
        Interface {
            ifname: self.ifname.clone(),
            ifindex: self.ifindex,
            bandwidth: self.bandwidth,
            mtu: self.mtu,
            ipv4_addr: self.ipv4_addr,
            mask_len: self.mask_len,
            l2_addr: self.l2_addr.clone(),
            headroom: self.headroom,
        }
    }
}

pub struct ModifyInterfaceMsg {
    pub intf: Arc<Interface>,
}

impl Clone for ModifyInterfaceMsg {
    fn clone(&self) -> ModifyInterfaceMsg {
        ModifyInterfaceMsg {
            intf: self.intf.clone(),
        }
    }
}
