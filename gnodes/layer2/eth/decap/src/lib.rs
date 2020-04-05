use counters::flavors::{Counter, CounterType};
use counters::Counters;
use fwd::intf::Interface;
use fwd::EthMacAddMsg;
use fwd::{
    EthMacRaw, EthOffsets, ARP_HWTYPE_ETH, ARP_OPCODE_REPLY, ARP_OPCODE_REQ, ETHER_HDR_LEN,
    ETH_ALEN, ETH_TYPE_ARP, ETH_TYPE_IPV4,
};
use graph::Dispatch;
use graph::Gclient;
use log::Logger;
use msg::R2Msg;
use msg::R2Msg::EthMacAdd;
use names::l2_eth_decap;
use packet::BoxPkt;
use packet::PacketPool;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Copy, Clone)]
enum Next {
    DROP = 0,
    L3Ipv4Parse,
    TX,
}

const NEXT_NAMES: &[Next] = &[Next::DROP, Next::L3Ipv4Parse, Next::TX];

fn next_name(ifindex: usize, next: Next) -> String {
    match next {
        Next::DROP => names::DROP.to_string(),
        Next::L3Ipv4Parse => names::L3_IPV4_PARSE.to_string(),
        Next::TX => names::rx_tx(ifindex),
    }
}

struct Cnt {
    unknown_ethtype: Counter,
    unknown_arp: Counter,
    not_my_mac: Counter,
    bad_mac: Counter,
    mac_send_fail: Counter,
}

// The decap node gets a packet from IfNode and removes the layer2 header and forwards
// to the layer3 node. It also handles an ARP request and sends ARP response. It also
// does source mac learning, stores the source macs in a hash table. The thing to note
// is that the decap node and the encap node are seperate and the encap node needs the
// mac addresses to send the packet out. So the decap node broadcasts the mac-learning
// as a message which helps the encap node to get the mac address. In a 'router' scenario
// where the mac addresses are few in number, this should not matter. But in a l2 switch
// scenario, this can be a scale issue. It should be possible to combine the encap and
// decap nodes to one node and avoid this. The macs will still be needed by the control
// plane thread for example for display. The whole mac address learning business will
// need to be thought of more carefully in time (github issue #4). Also today we just
// support plain ethernet packets without vlan tags.
pub struct EthDecap<'p> {
    intf: Arc<Interface>,
    mac: HashMap<Ipv4Addr, EthMacRaw>,
    sender: Sender<R2Msg<'p>>,
    cnt: Cnt,
}

impl<'p> EthDecap<'p> {
    pub fn new(intf: Arc<Interface>, counters: &mut Counters, sender: Sender<R2Msg<'p>>) -> Self {
        let unknown_ethtype = Counter::new(
            counters,
            &l2_eth_decap(intf.ifindex),
            CounterType::Error,
            "unknown_ethtype",
        );
        let unknown_arp = Counter::new(
            counters,
            &l2_eth_decap(intf.ifindex),
            CounterType::Error,
            "unknown_arp",
        );
        let not_my_mac = Counter::new(
            counters,
            &l2_eth_decap(intf.ifindex),
            CounterType::Error,
            "not_my_mac",
        );
        let bad_mac = Counter::new(
            counters,
            &l2_eth_decap(intf.ifindex),
            CounterType::Error,
            "bad_mac",
        );
        let mac_send_fail = Counter::new(
            counters,
            &l2_eth_decap(intf.ifindex),
            CounterType::Error,
            "mac_send_fail",
        );
        EthDecap {
            intf,
            mac: HashMap::new(),
            sender,
            cnt: Cnt {
                unknown_ethtype,
                unknown_arp,
                not_my_mac,
                bad_mac,
                mac_send_fail,
            },
        }
    }

    pub fn name(&self) -> String {
        l2_eth_decap(self.intf.ifindex)
    }

    pub fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            assert_eq!(*n as usize, v.len());
            v.push(next_name(self.intf.ifindex, *n));
        }
        v
    }

    fn do_arp_reply(
        &self,
        pool: &mut dyn PacketPool<'p>,
        src_ip: Ipv4Addr,
        src_mac: &[u8],
    ) -> Option<BoxPkt<'p>> {
        let pkt = pool.pkt(0 /* no headroom */);
        pkt.as_ref()?;
        let mut pkt = pkt.unwrap();
        let raw = pkt.data_raw_mut();

        // Dest mac
        let off = EthOffsets::EthDaddrOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(src_mac);
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
        raw[off..off + 2].copy_from_slice(&ARP_OPCODE_REPLY.to_be_bytes());
        // src mac
        let off = EthOffsets::EthSenderMacOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(&self.intf.l2_addr[0..ETH_ALEN]);
        // src ipv4 addr
        let off = EthOffsets::EthSenderIpOff as usize;
        raw[off..off + 4].copy_from_slice(&self.intf.ipv4_addr.octets());
        // dst mac
        let off = EthOffsets::EthTargetMacOff as usize;
        raw[off..off + ETH_ALEN].copy_from_slice(src_mac);
        // dst ipv4 addr
        let off = EthOffsets::EthTargetIpOff as usize;
        raw[off..off + 4].copy_from_slice(&src_ip.octets());

        let bytes = 2 * ETH_ALEN + 2 + 2 + 2 + 1 + 1 + 2 + ETH_ALEN + 4 + ETH_ALEN + 4;
        pkt.move_tail(bytes as isize);
        pkt.out_ifindex = self.intf.ifindex;
        Some(pkt)
    }

    fn process_arp(
        &mut self,
        pool: &mut dyn PacketPool<'p>,
        mac: &[u8],
        len: usize,
    ) -> Option<BoxPkt<'p>> {
        let off = EthOffsets::EthOpcodeOff as usize;
        let op = u16::from_be_bytes([mac[off], mac[off + 1]]);
        let off = EthOffsets::EthProtoOff as usize;
        let proto = u16::from_be_bytes([mac[off], mac[off + 1]]);
        if op == ARP_OPCODE_REPLY && proto == ETH_TYPE_IPV4 {
            self.process_arp_reply(mac, len);
            None
        } else if op == ARP_OPCODE_REQ && proto == ETH_TYPE_IPV4 {
            self.process_arp_req(pool, mac, len)
        } else {
            self.cnt.unknown_arp.incr();
            None
        }
    }

    fn mac_learn(&mut self, ip: Ipv4Addr, mac: &[u8]) {
        if self.mac.get(&ip).is_some() {
            // do nothing
        } else {
            let mut bytes = Vec::new();
            bytes.extend(mac);
            let raw = EthMacRaw {
                bytes: Arc::new(bytes),
            };
            self.mac.insert(
                ip,
                EthMacRaw {
                    bytes: raw.bytes.clone(),
                },
            );
            if self
                .sender
                .send(EthMacAdd(EthMacAddMsg {
                    ifindex: self.intf.ifindex,
                    ip,
                    mac: raw,
                }))
                .is_err()
            {
                self.cnt.mac_send_fail.incr();
            }
        }
    }

    fn process_arp_req(
        &mut self,
        pool: &mut dyn PacketPool<'p>,
        mac: &[u8],
        _len: usize,
    ) -> Option<BoxPkt<'p>> {
        let off = EthOffsets::EthTargetIpOff as usize;
        let dst_ip = &mac[off..off + 4];
        if self.intf.ipv4_addr.octets() != dst_ip {
            self.cnt.unknown_arp.incr();
            return None;
        }

        let off = EthOffsets::EthSenderIpOff as usize;
        let src_ip = Ipv4Addr::new(mac[off], mac[off + 1], mac[off + 2], mac[off + 3]);
        let off = EthOffsets::EthSenderMacOff as usize;
        let src_mac = &mac[off..off + ETH_ALEN];
        self.mac_learn(src_ip, src_mac);
        self.do_arp_reply(pool, src_ip, src_mac)
    }

    fn process_arp_reply(&mut self, mac: &[u8], _len: usize) {
        let off = EthOffsets::EthTargetIpOff as usize;
        let dst_ip = &mac[off..off + 4];
        if self.intf.ipv4_addr.octets() != dst_ip {
            self.cnt.unknown_arp.incr();
            return;
        }
        let off = EthOffsets::EthTargetMacOff as usize;
        let dst_mac = &mac[off..off + ETH_ALEN];
        if dst_mac != &self.intf.l2_addr[0..ETH_ALEN] {
            self.cnt.unknown_arp.incr();
            return;
        }
        let off = EthOffsets::EthSenderIpOff as usize;
        let src_ip = Ipv4Addr::new(mac[off], mac[off + 1], mac[off + 2], mac[off + 3]);
        let off = EthOffsets::EthSenderMacOff as usize;
        let src_mac = &mac[off..off + ETH_ALEN];
        self.mac_learn(src_ip, src_mac);
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
}

impl<'p> Gclient<'p, R2Msg<'p>> for EthDecap<'p> {
    fn clone(
        &self,
        counters: &mut Counters,
        _log: Arc<Logger>,
    ) -> Box<dyn Gclient<'p, R2Msg<'p>> + 'p> {
        let unknown_ethtype = Counter::new(
            counters,
            &self.name(),
            CounterType::Error,
            "unknown_ethtype",
        );
        let unknown_arp = Counter::new(counters, &self.name(), CounterType::Error, "unknown_arp");
        let not_my_mac = Counter::new(counters, &self.name(), CounterType::Error, "not_my_mac");
        let bad_mac = Counter::new(counters, &self.name(), CounterType::Error, "bad_mac");
        let mac_send_fail =
            Counter::new(counters, &self.name(), CounterType::Error, "mac_send_fail");
        Box::new(EthDecap {
            intf: self.intf.clone(),
            mac: HashMap::new(),
            sender: self.sender.clone(),
            cnt: Cnt {
                unknown_ethtype,
                unknown_arp,
                not_my_mac,
                bad_mac,
                mac_send_fail,
            },
        })
    }

    fn dispatch<'d>(&mut self, _thread: usize, vectors: &mut Dispatch<'d, 'p>) {
        while let Some(mut p) = vectors.pop() {
            assert_eq!(p.pull_l2(ETHER_HDR_LEN), ETHER_HDR_LEN);
            let (mac, len) = p.get_l2();

            let off = EthOffsets::EthTypeOff as usize;
            let ethtype = u16::from_be_bytes([mac[off], mac[off + 1]]);
            if ethtype == ETH_TYPE_ARP {
                if let Some(arp) = self.process_arp(vectors.pool, mac, len) {
                    vectors.push(Next::TX as usize, arp);
                }
            } else {
                let off = EthOffsets::EthDaddrOff as usize;
                if self.intf.l2_addr[0..ETH_ALEN] != mac[off..off + ETH_ALEN] {
                    self.cnt.not_my_mac.incr();
                    continue;
                }

                if ethtype == ETH_TYPE_IPV4 {
                    vectors.push(Next::L3Ipv4Parse as usize, p);
                } else {
                    self.cnt.unknown_ethtype.incr();
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
