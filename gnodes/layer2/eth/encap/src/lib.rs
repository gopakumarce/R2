use counters::flavors::{Counter, CounterType};
use counters::Counters;
use fwd::intf::MAX_INTERFACES;
use fwd::IPHDR_DADDR_OFF;
use fwd::IPHDR_MIN_LEN;
use fwd::{
    intf::Interface, EthMacRaw, EthOffsets, ARP_HWTYPE_ETH, ARP_OPCODE_REQ, BCAST_MAC, ETH_ALEN,
    ETH_TYPE_ARP, ETH_TYPE_IPV4, ZERO_IP, ZERO_MAC,
};
use graph::Dispatch;
use graph::Gclient;
use log::Logger;
use msg::R2Msg;
use names::l2_eth_encap;
use packet::BoxPkt;
use packet::PacketPool;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;

// If the system has say 4000 interfaces, EncapMux prevents having every single node that needs
// to send a packet out to have 4000 nodes as their next-node. Instead those nodes have EncapMux
// as their next node and EncapMux will have 4000 next nodes. So all that EncapMux does is to
// take the input packet and enqueu it to the right EthEncap node. This convenience of course
// comes with the hit that all output packets incur one unnecessary dequeue/enqueue
#[derive(Default)]
pub struct EncapMux {
    next_names: Vec<String>,
}

impl EncapMux {
    pub fn new() -> EncapMux {
        EncapMux {
            next_names: (0..MAX_INTERFACES).map(names::l2_eth_encap).collect(),
        }
    }

    pub fn name(&self) -> String {
        names::ENCAPMUX.to_string()
    }

    pub fn next_names(&self) -> Vec<String> {
        self.next_names.clone()
    }
}

impl<T> Gclient<T> for EncapMux {
    fn clone(&self, _counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<T>> {
        Box::new(EncapMux {
            next_names: self.next_names.clone(),
        })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(p) = vectors.pop() {
            vectors.push(p.out_ifindex, p);
        }
    }
}

#[derive(Copy, Clone)]
enum Next {
    DROP = 0,
    TX,
}

const NEXT_NAMES: &[Next] = &[Next::DROP, Next::TX];

fn next_name(ifindex: usize, next: Next) -> String {
    match next {
        Next::DROP => names::DROP.to_string(),
        Next::TX => names::rx_tx(ifindex),
    }
}

struct Cnt {
    bad_mac: Counter,
}

// Encapsulate an ethernet packet and send it to the interface. If the mac address table
// does not have the mac address, generate an ARP request. The ARP response will be received
// on the Decap node and it will broadcast the learned mac which the Encap node will also
// receive. This mechanism needs rethinking (see github issue #4)
pub struct EthEncap {
    intf: Arc<Interface>,
    mac: HashMap<Ipv4Addr, EthMacRaw>,
    cnt: Cnt,
}

impl EthEncap {
    pub fn new(intf: Arc<Interface>, counters: &mut Counters) -> Self {
        let bad_mac = Counter::new(
            counters,
            &l2_eth_encap(intf.ifindex),
            CounterType::Error,
            "bad_mac",
        );
        EthEncap {
            intf,
            mac: HashMap::new(),
            cnt: Cnt { bad_mac },
        }
    }

    pub fn name(&self) -> String {
        l2_eth_encap(self.intf.ifindex)
    }

    pub fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            assert_eq!(*n as usize, v.len());
            v.push(next_name(self.intf.ifindex, *n));
        }
        v
    }

    fn do_arp_request(&self, pool: &mut dyn PacketPool, in_pkt: &BoxPkt) -> Option<BoxPkt> {
        let pkt = pool.pkt(0 /* no headroom */);
        pkt.as_ref()?;
        let mut pkt = pkt.unwrap();
        let raw = pkt.data_raw_mut();

        // Dest mac all ones
        let off = EthOffsets::EthDaddrOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(BCAST_MAC);
        // Src mac
        let off = EthOffsets::EthSaddrOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(&self.intf.l2_addr[0..ETH_ALEN]);
        // 0x0806 ARP
        let off = EthOffsets::EthTypeOff as usize;
        raw[off..off + 2].copy_from_slice(&ETH_TYPE_ARP.to_be_bytes());
        // Hardware type ethernet 0x0001
        let off = EthOffsets::EthHwtypeOff as usize;
        raw[off..off + 2].copy_from_slice(&ARP_HWTYPE_ETH.to_be_bytes());
        // Ether type ipv4 0x0800
        let off = EthOffsets::EthProtoOff as usize;
        raw[off..off + 2].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());
        // Hw addr length 6
        let off = EthOffsets::EthHwSzOff as usize;
        raw[off] = 6;
        // Procol addr length 4
        let off = EthOffsets::EthProtoSzOff as usize;
        raw[off] = 4;
        // Arp opcode request
        let off = EthOffsets::EthOpcodeOff as usize;
        raw[off..off + 2].copy_from_slice(&ARP_OPCODE_REQ.to_be_bytes());
        // src mac
        let off = EthOffsets::EthSenderMacOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(&self.intf.l2_addr[0..ETH_ALEN]);
        // src ipv4 addr
        let off = EthOffsets::EthSenderIpOff as usize;
        raw[off..off + 4].copy_from_slice(&self.intf.ipv4_addr.octets());
        // dst mac
        let off = EthOffsets::EthTargetMacOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(ZERO_MAC);
        // dst ipv4 addr
        let off = EthOffsets::EthTargetIpOff as usize;
        if in_pkt.out_l3addr == ZERO_IP {
            // If adjacency has zero nexthop, its a connected adj, use destination IP
            // to arp
            let (l3, l3len) = in_pkt.get_l3();
            assert!(l3len >= IPHDR_MIN_LEN);
            raw[off..off + 4].copy_from_slice(&l3[IPHDR_DADDR_OFF..IPHDR_DADDR_OFF + 4]);
        } else {
            raw[off..off + 4].copy_from_slice(&in_pkt.out_l3addr.octets());
        }

        let bytes = 2 * ETH_ALEN + 2 + 2 + 2 + 1 + 1 + 2 + ETH_ALEN + 4 + ETH_ALEN + 4;
        pkt.move_tail(bytes as isize);
        pkt.out_ifindex = self.intf.ifindex;
        Some(pkt)
    }

    pub fn mac_add(&mut self, ip: Ipv4Addr, mac: EthMacRaw) {
        if mac.bytes.len() < ETH_ALEN {
            self.cnt.bad_mac.incr();
            return;
        }
        if self.mac.get(&ip).is_none() {
            self.mac.insert(ip, mac);
        }
    }

    fn add_eth_hdr(&self, pool: &mut dyn PacketPool, pkt: &mut BoxPkt, mac: &EthMacRaw) -> bool {
        if !pkt.prepend(pool, &ETH_TYPE_IPV4.to_be_bytes()) {
            return false;
        }
        if !pkt.prepend(pool, &self.intf.l2_addr[0..ETH_ALEN]) {
            return false;
        }
        if !pkt.prepend(pool, &(*mac.bytes)) {
            return false;
        }
        pkt.set_l2(ETH_ALEN);
        true
    }
}

impl Gclient<R2Msg> for EthEncap {
    fn clone(&self, counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<R2Msg>> {
        let bad_mac = Counter::new(counters, &self.name(), CounterType::Error, "bad_mac");
        Box::new(EthEncap {
            intf: self.intf.clone(),
            mac: HashMap::new(),
            cnt: Cnt { bad_mac },
        })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(mut p) = vectors.pop() {
            let mac = self.mac.get(&p.out_l3addr);
            if let Some(mac) = mac {
                if self.add_eth_hdr(vectors.pool, &mut p, mac) {
                    vectors.push(Next::TX as usize, p);
                }
            } else {
                let arp = self.do_arp_request(vectors.pool, &p);
                if let Some(arp) = arp {
                    vectors.push(Next::TX as usize, arp);
                }
            }
        }
    }

    fn control_msg(&mut self, _thread: usize, message: R2Msg) {
        match message {
            R2Msg::ModifyInterface(mod_intf) => {
                self.intf = mod_intf.intf;
            }
            R2Msg::EthMacAdd(mac_add) => {
                self.mac_add(mac_add.ip, mac_add.mac);
            }
            _ => panic!("Unknown type"),
        }
    }
}
