use counters::{flavors::Counter, flavors::CounterType, Counters};
use fwd::IPHDR_DADDR_OFF;
use fwd::IPHDR_MIN_LEN;
use fwd::{ipv4::IPv4Table, Fwd};
use graph::Dispatch;
use graph::Gclient;
use log::Logger;
use msg::R2Msg;
use std::net::Ipv4Addr;
use std::sync::Arc;

#[derive(Copy, Clone)]
enum Next {
    DROP = 0,
    EncapMux,
}

const NEXT_NAMES: &[Next] = &[Next::DROP, Next::EncapMux];

fn next_name(next: Next) -> String {
    match next {
        Next::DROP => names::DROP.to_string(),
        Next::EncapMux => names::ENCAPMUX.to_string(),
    }
}

struct IPv4Cnt {
    no_route: Counter,
    invalid_l3: Counter,
}

// The IPv4 Forwarding node: all it does is a route lookup the destinaton address in a
// tree-bitmap data structure, find the 'adjacency' information that says where the
// packet has to go out and send it to the Encap node for that output interface.
pub struct IPv4Fwd {
    table: Arc<IPv4Table>,
    cnt: IPv4Cnt,
}

impl IPv4Fwd {
    pub fn new(table: Arc<IPv4Table>, counters: &mut Counters) -> IPv4Fwd {
        let invalid_l3 = Counter::new(
            counters,
            names::L3_IPV4_FWD,
            CounterType::Error,
            "invalid_l3",
        );
        let no_route = Counter::new(counters, names::L3_IPV4_FWD, CounterType::Pkts, "no_route");
        IPv4Fwd {
            table,
            cnt: IPv4Cnt {
                no_route,
                invalid_l3,
            },
        }
    }

    pub fn name(&self) -> String {
        names::L3_IPV4_FWD.to_string()
    }

    pub fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            assert_eq!(*n as usize, v.len());
            v.push(next_name(*n));
        }
        v
    }
}

impl Gclient<R2Msg> for IPv4Fwd {
    fn clone(&self, counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<R2Msg>> {
        let no_route = Counter::new(counters, &self.name(), CounterType::Pkts, "no_route");
        let invalid_l3 = Counter::new(counters, &self.name(), CounterType::Error, "invalid_l3");
        Box::new(IPv4Fwd {
            table: self.table.clone(),
            cnt: IPv4Cnt {
                no_route,
                invalid_l3,
            },
        })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(mut p) = vectors.pop() {
            let (iphdr, hdrlen) = p.get_l3();
            if hdrlen < IPHDR_MIN_LEN {
                self.cnt.invalid_l3.incr();
                continue;
            }
            let daddr = Ipv4Addr::new(
                iphdr[IPHDR_DADDR_OFF],
                iphdr[IPHDR_DADDR_OFF + 1],
                iphdr[IPHDR_DADDR_OFF + 2],
                iphdr[IPHDR_DADDR_OFF + 3],
            );
            if let Some((_prefix, _mask, leaf)) = self.table.root.longest_match(daddr) {
                match &leaf.next {
                    Fwd::Adjacency(adj) => {
                        p.out_ifindex = adj.ifindex;
                        p.out_l3addr = adj.nhop;
                        if p.out_l3addr == fwd::ZERO_IP {
                            // destination is in connected subnet
                            p.out_l3addr = daddr;
                        }
                        vectors.push(Next::EncapMux as usize, p);
                    }
                    _ => {
                        let _ = self.cnt.no_route.incr();
                    }
                }
            } else {
                self.cnt.no_route.incr();
            }
        }
    }

    fn control_msg(&mut self, _thread: usize, message: R2Msg) {
        match message {
            R2Msg::IPv4TableAdd(table) => {
                self.table = table.table;
            }
            _ => panic!("Unknown type"),
        }
    }
}
