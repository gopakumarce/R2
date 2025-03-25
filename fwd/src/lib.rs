use std::net::Ipv4Addr;
use std::sync::Arc;
use treebitmap::IpLookupTable;
pub mod ipv4;
use ipv4::IPv4Leaf;
pub mod adj;
use adj::Adjacency;
pub mod intf;
use intf::Interface;
use std::str::FromStr;

pub const ETH_TYPE_ARP: u16 = 0x0806;
pub const ETH_TYPE_IPV4: u16 = 0x0800;
pub const ARP_HWTYPE_ETH: u16 = 0x0001;
pub const ARP_OPCODE_REQ: u16 = 0x0001;
pub const ARP_OPCODE_REPLY: u16 = 0x0002;
pub const ETH_ALEN: usize = 6;
pub const ETHER_HDR_LEN: usize = 14;
pub const ETHER_MTU: usize = 1500;
pub const BCAST_MAC: &[u8; ETH_ALEN] = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
pub const ZERO_MAC: &[u8; ETH_ALEN] = &[0, 0, 0, 0, 0, 0];
pub const ZERO_IP: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
pub const IPHDR_MIN_LEN: usize = 20;
pub const IPHDR_DADDR_OFF: usize = 16;

pub enum EthOffsets {
    EthDaddrOff = 0,
    EthSaddrOff = 6,
    EthTypeOff = 12,
    EthHwtypeOff = 14,
    EthProtoOff = 16,
    EthHwSzOff = 18,
    EthProtoSzOff = 19,
    EthOpcodeOff = 20,
    EthSenderMacOff = 22,
    EthSenderIpOff = 28,
    EthTargetMacOff = 32,
    EthTargetIpOff = 38,
}

#[allow(dead_code)]
pub struct EthHdr {
    dhost: [u8; ETH_ALEN],
    shost: [u8; ETH_ALEN],
    eth_type: u16,
}

pub struct EthMacRaw {
    pub bytes: Arc<Vec<u8>>,
}

pub struct EthMacAddMsg {
    pub ifindex: usize,
    pub ip: Ipv4Addr,
    pub mac: EthMacRaw,
}

impl Clone for EthMacAddMsg {
    fn clone(&self) -> EthMacAddMsg {
        EthMacAddMsg {
            ifindex: self.ifindex,
            ip: self.ip,
            mac: EthMacRaw {
                bytes: self.mac.bytes.clone(),
            },
        }
    }
}

#[allow(dead_code)]
pub struct IpHdr {
    ihl: u8,
    version: u8,
    tos: u8,
    tot_len: u16,
    id: u16,
    frag_off: u16,
    ttl: u8,
    protocol: u8,
    check: u16,
    saddr: u32,
    daddr: u32,
    /*The options start here. */
}

#[derive(Clone)]
pub enum Fwd {
    IPv4Leaf(Arc<IPv4Leaf>),
    Adjacency(Arc<Adjacency>),
    Interface(Arc<Interface>),
}

pub fn str_to_mac(mac: &str) -> Option<Vec<u8>> {
    let mac = mac.split(':');
    let mut bytes = Vec::new();
    for m in mac {
        if let Ok(byte) = u8::from_str_radix(m, 16) {
            bytes.push(byte);
        } else {
            return None;
        }
    }
    if bytes.len() == ETH_ALEN {
        Some(bytes)
    } else {
        None
    }
}

pub fn ip_mask_decode(ip_and_mask: &str) -> Option<(Ipv4Addr, u32)> {
    let im = ip_and_mask.split('/');
    let im: Vec<&str> = im.collect();
    if im.len() != 2 {
        return None;
    }
    if let Ok(ipv4) = Ipv4Addr::from_str(im[0]) {
        if let Ok(masklen) = im[1].parse::<u32>() {
            Some((ipv4, masklen))
        } else {
            None
        }
    } else {
        None
    }
}
