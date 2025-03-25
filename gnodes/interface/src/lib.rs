use counters::flavors::{Counter, CounterType};
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use efd::Efd;
use fwd::intf::Interface;
use graph::Driver;
use graph::{Dispatch, Gclient, VEC_SIZE};
use log::Logger;
use msg::R2Msg;
use names::l2_eth_decap;
use packet::BoxPkt;
use sched::hfsc::Hfsc;
use std::sync::Arc;

#[derive(Copy, Clone)]
enum Next {
    Drop = 0,
    L2EthDecap,
}

const NEXT_NAMES: &[Next] = &[Next::Drop, Next::L2EthDecap];

fn next_name(ifindex: usize, next: Next) -> String {
    match next {
        Next::Drop => names::DROP.to_string(),
        Next::L2EthDecap => l2_eth_decap(ifindex),
    }
}

// The interface node (Ifnode) in the graph is responsible for reading packets from
// an interface and sending packets ouf of an interface - the IfNode has a 'driver'
// that handles the I/O part. Today the driver is just raw socket, it will eventually
// get extended to have more options like DPDK etc.. The IfNode for an interface is
// present in all forwarding threads, although only one thread is the 'owner' of the
// interface. All other threads handoff packets to the 'owner' vis MPSC 'thread_q'
pub struct IfNode {
    name: String,
    affinity: Option<usize>,
    intf: Arc<Interface>,
    sched: Hfsc,
    driver: Option<Box<dyn Driver + Send>>,
    sched_fail: Counter,
    threadq_fail: Counter,
    thread_q: Arc<ArrayQueue<BoxPkt>>,
    thread_wakeup: Arc<Efd>,
}

impl IfNode {
    // affinity: specifies which thread owns the IfNode
    // efd: event fd (efd) used to wakeup the owner thread when handing off packets on thread_q
    // intf: The common driver-agnostic parameters of an interface like ip address/mtu etc..
    pub fn new(
        counters: &mut Counters,
        affinity: Option<usize>,
        efd: Arc<Efd>,
        intf: Arc<Interface>,
        driver: Box<dyn Driver + Send>,
    ) -> Result<Self, i32> {
        let name = names::rx_tx(intf.ifindex);

        // By default the scheduler is HFSC today, eventually there will be other options
        let sched = sched::hfsc::Hfsc::new(common::MB!(10 * 1024));
        let sched_fail = Counter::new(counters, &name, CounterType::Error, "sched_fail");
        let threadq_fail = Counter::new(counters, &name, CounterType::Error, "threadq_fail");
        Ok(IfNode {
            name,
            affinity,
            intf,
            sched,
            driver: Some(driver),
            sched_fail,
            threadq_fail,
            thread_q: Arc::new(ArrayQueue::new(VEC_SIZE)),
            thread_wakeup: efd,
        })
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            assert_eq!(*n as usize, v.len());
            v.push(next_name(self.intf.ifindex, *n));
        }
        v
    }

    pub fn fd(&self) -> Option<i32> {
        if let Some(ref driver) = self.driver {
            driver.fd()
        } else {
            None
        }
    }
}

impl Gclient<R2Msg> for IfNode {
    fn clone(&self, counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<R2Msg>> {
        // Only the 'owner' IfNode really needs/uses a scheduler, so in all other nodes, the
        // sched doesnt really do anything, they handoff packets to the owner IfNode.
        let sched = sched::hfsc::Hfsc::new(common::MB!(10 * 1024));
        let sched_fail = Counter::new(counters, &self.name, CounterType::Error, "sched_fail");
        let threadq_fail = Counter::new(counters, &self.name, CounterType::Error, "threadq_fail");
        Box::new(IfNode {
            name: self.name.clone(),
            affinity: self.affinity,
            intf: self.intf.clone(),
            sched,
            driver: None,
            sched_fail,
            threadq_fail,
            thread_q: self.thread_q.clone(),
            thread_wakeup: self.thread_wakeup.clone(),
        })
    }

    fn dispatch(&mut self, thread: usize, vectors: &mut Dispatch) {
        let owner_thread = self.affinity.is_none() || (self.affinity == Some(thread));
        // Do packet Tx if we are the owner thread (thread the driver/device is pinnned to).
        // If so send the packet out on the driver, otherwise enqueue the packet to the MPSC
        // queue to the owner thread
        while let Some(p) = vectors.pop() {
            if owner_thread {
                // TODO: We have the scheduler, but we havent figured out the packet queueing
                // model. Till then we cant really put the scheduler to use
                if !self.sched.has_classes() {
                    self.driver.as_mut().unwrap().sendmsg(vectors.pool, p);
                }
            } else if self.thread_q.push(p).is_err() {
                self.threadq_fail.incr();
            } else {
                self.thread_wakeup.write(1);
            }
        }
        if owner_thread {
            while let Ok(p) = self.thread_q.pop() {
                if !self.sched.has_classes() {
                    self.driver.as_mut().unwrap().sendmsg(vectors.pool, p);
                }
            }
        }
        if self.sched.pkts_queued() != 0 {
            // Well, we are not caring to return the exact scheduler time at the moment, but
            // its a TODO to return here the smallest scheduler interval rather than 0
            vectors.wakeup(0);
        }
        // Do packet Rx, only on the thread this driver is pinned to
        if owner_thread {
            for _ in 0..VEC_SIZE {
                let pkt = self
                    .driver
                    .as_mut()
                    .unwrap()
                    .recvmsg(vectors.pool, self.intf.headroom);
                if pkt.is_none() {
                    break;
                }
                let mut pkt = pkt.unwrap();
                if pkt.len() == 0 {
                    break;
                }
                pkt.in_ifindex = self.intf.ifindex;
                vectors.push(Next::L2EthDecap as usize, pkt);
            }
        }
    }
    fn control_msg(&mut self, thread: usize, message: R2Msg) {
        match message {
            R2Msg::ModifyInterface(mod_intf) => {
                self.intf = mod_intf.intf;
            }
            R2Msg::ClassAdd(class) => {
                if (self.affinity.is_none() || (self.affinity == Some(thread)))
                    && self
                        .sched
                        .create_class(
                            class.name,
                            class.parent,
                            class.qlimit,
                            class.is_leaf,
                            class.curves,
                        )
                        .is_err()
                {
                    self.sched_fail.incr();
                }
            }
            _ => panic!("Unknown type"),
        }
    }
}
