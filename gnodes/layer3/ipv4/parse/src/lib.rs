use counters::flavors::{Counter, CounterType};
use counters::Counters;
use fwd::IPHDR_MIN_LEN;
use graph::Dispatch;
use graph::Gclient;
use log::Logger;
use std::sync::Arc;

#[derive(Copy, Clone)]
enum Next {
    DROP = 0,
    L3Ipv4Fwd,
}

const NEXT_NAMES: &[Next] = &[Next::DROP, Next::L3Ipv4Fwd];

fn next_name(next: Next) -> String {
    match next {
        Next::DROP => names::DROP.to_string(),
        Next::L3Ipv4Fwd => names::L3_IPV4_FWD.to_string(),
    }
}

// The parse node is assumed to get a layer3 packet as input, and its role is to redirect
// the packet to the appropriate layer3 feature node (like v4, v6 or gre or mpls etc..).
// All it handles today is ipv4
pub struct IPv4Parse {
    bad_pkt: Counter,
}

impl IPv4Parse {
    pub fn new(counters: &mut Counters) -> IPv4Parse {
        let bad_pkt = Counter::new(
            counters,
            names::L3_IPV4_PARSE,
            CounterType::Error,
            "bad_pkt",
        );
        IPv4Parse { bad_pkt }
    }

    pub fn name(&self) -> String {
        names::L3_IPV4_PARSE.to_string()
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

impl<'p, T: 'p> Gclient<'p, T> for IPv4Parse {
    fn clone(&self, counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<'p, T>> {
        let bad_pkt = Counter::new(counters, &self.name(), CounterType::Error, "bad_pkt");
        Box::new(IPv4Parse { bad_pkt })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(mut p) = vectors.pop() {
            if p.set_l3(IPHDR_MIN_LEN) {
                vectors.push(Next::L3Ipv4Fwd as usize, p);
            } else {
                self.bad_pkt.incr();
            }
        }
    }
}
