use super::*;
use crate::ipv4::add_route;
use crate::ipv4::del_route;
use apis_interface::{CurvesApi, InterfaceErr, InterfaceSyncHandler};
use fwd::intf::Interface;
use fwd::intf::ModifyInterfaceMsg;
use fwd::ZERO_IP;
use interface::IfNode;
use l2_eth_decap::EthDecap;
use l2_eth_encap::EthEncap;
use msg::EpollAddMsg;
use msg::{ClassAddMsg, GnodeAddMsg};
use msg::{Curves, Sc};
use std::net::Ipv4Addr;

pub struct InterfaceApis {
    r2: Arc<Mutex<R2>>,
}

impl InterfaceApis {
    pub fn new(r2: Arc<Mutex<R2>>) -> InterfaceApis {
        InterfaceApis { r2 }
    }
}

// Information pertaining to all interfaces in the system
pub struct IfdCtx {
    // A simple distribute interfaces across threads scheme,
    // thelast thread that was assigned an interface
    last_thread: usize,
    name2idx: HashMap<String, usize>,
    idx2name: HashMap<usize, String>,
    interfaces: HashMap<String, Arc<Interface>>,
}

impl IfdCtx {
    pub fn new() -> IfdCtx {
        IfdCtx {
            last_thread: 0,
            name2idx: HashMap::new(),
            idx2name: HashMap::new(),
            interfaces: HashMap::new(),
        }
    }

    fn add(&mut self, ifname: &str, ifindex: usize, interface: Arc<Interface>) {
        self.interfaces.insert(ifname.to_string(), interface);
        self.name2idx.insert(ifname.to_string(), ifindex);
        self.idx2name.insert(ifindex, ifname.to_string());
    }

    pub fn get(&self, ifname: &str) -> Option<&Arc<Interface>> {
        self.interfaces.get(ifname)
    }

    pub fn get_name(&self, ifindex: usize) -> Option<&String> {
        self.idx2name.get(&ifindex)
    }
}

// The HFSC service curves that define fair share, realtime and upper limit
fn unwrap_curves(curves: &CurvesApi) -> Curves {
    let mut rsc: Sc = Default::default();
    let mut r_sc = None;
    if let Some(ref r) = curves.r_sc {
        if let Some(m1) = r.m1 {
            rsc.m1 = m1 as u64;
        }
        if let Some(d) = r.d {
            rsc.d = d as usize;
        }
        if let Some(m2) = r.m2 {
            rsc.m2 = m2 as u64;
        }
        r_sc = Some(rsc);
    }

    let mut usc: Sc = Default::default();
    let mut u_sc = None;
    if let Some(ref u) = curves.u_sc {
        if let Some(m1) = u.m1 {
            usc.m1 = m1 as u64;
        }
        if let Some(d) = u.d {
            usc.d = d as usize;
        }
        if let Some(m2) = u.m2 {
            usc.m2 = m2 as u64;
        }
        u_sc = Some(usc);
    }

    let mut f_sc: Sc = Default::default();
    if let Some(ref f) = curves.f_sc {
        if let Some(m1) = f.m1 {
            f_sc.m1 = m1 as u64;
        }
        if let Some(d) = f.d {
            f_sc.d = d as usize;
        }
        if let Some(m2) = f.m2 {
            f_sc.m2 = m2 as u64;
        }
    }

    Curves { r_sc, u_sc, f_sc }
}

fn create_eth_nodes(r2: &mut R2, intf: Arc<Interface>) {
    let decap = EthDecap::new(intf.clone(), &mut r2.counters, r2.fwd2ctrl.clone());
    let init = GnodeInit {
        name: decap.name(),
        next_names: decap.next_names(),
        cntrs: GnodeCntrs::new(&decap.name(), &mut r2.counters),
    };
    let msg = GnodeAddMsg {
        node: Box::new(decap),
        init,
    };
    let msg = R2Msg::GnodeAdd(msg);
    r2.broadcast(msg);

    let encap = EthEncap::new(intf, &mut r2.counters);
    let init = GnodeInit {
        name: encap.name(),
        next_names: encap.next_names(),
        cntrs: GnodeCntrs::new(&encap.name(), &mut r2.counters),
    };
    let msg = GnodeAddMsg {
        node: Box::new(encap),
        init,
    };
    let msg = R2Msg::GnodeAdd(msg);
    r2.broadcast(msg);
}

pub fn create_interface_node(
    r2: &mut R2,
    ifname: &str,
    ifindex: usize,
    l2_addr: Vec<u8>,
) -> Result<(), i32> {
    let interface = Arc::new(Interface::new(ifname, ifindex, l2_addr, MAX_HEADROOM));
    // We simply spread interfaces across threads, a better strategy might be needed going foward
    let thread = r2.ifd.last_thread;
    r2.ifd.last_thread = (thread + 1) % r2.nthreads;
    let thread_mask = 1 << thread;
    let efd = r2.threads[thread].efd.clone();
    let intf = match IfNode::new(&mut r2.counters, thread_mask, efd, interface.clone()) {
        Ok(intf) => intf,
        Err(errno) => return Err(-errno),
    };

    // If the interface has file descriptors that indicate I/O readiness, we add it to the
    // list of descriptors we are polling on. Every forwarding thread is polling on its own
    // set of descriptors, every thread will receive this message, but only the ones marked
    // in thread_mask will add the fd to its epoll
    let msg = EpollAddMsg {
        fd: intf.fd(),
        thread_mask,
    };
    let msg = R2Msg::EpollAdd(msg);
    r2.broadcast(msg);

    // Broadcast a message and ask every forwarding thread to add an IfNode in their graph.
    // All threads will create an IfNode, but only the ones specified in thread_mask will
    // do device I/O
    let init = GnodeInit {
        name: intf.name(),
        next_names: intf.next_names(),
        cntrs: GnodeCntrs::new(&intf.name(), &mut r2.counters),
    };
    let msg = GnodeAddMsg {
        node: Box::new(intf),
        init,
    };
    let msg = R2Msg::GnodeAdd(msg);
    r2.broadcast(msg);

    r2.ifd.add(ifname, ifindex as usize, interface.clone());
    create_eth_nodes(r2, interface);

    Ok(())
}

impl InterfaceSyncHandler for InterfaceApis {
    fn handle_add_if(&self, name: String, ifindex: i32, mac: String) -> thrift::Result<()> {
        let l2_addr;
        if let Some(mac) = fwd::str_to_mac(&mac) {
            l2_addr = mac;
        } else {
            return Err(InterfaceErr::new(
                "Unable to decode mac address".to_string(),
            ))
            .map_err(From::from);
        }
        let mut r2 = self.r2.lock().unwrap();
        if r2.ifd.name2idx.get(&name).is_some()
            || r2.ifd.idx2name.get(&(ifindex as usize)).is_some()
        {
            return Err(InterfaceErr::new(format!(
                "Interface {}, index {} exists",
                name, ifindex
            )))
            .map_err(From::from);
        }
        if let Err(errno) = create_interface_node(&mut r2, &name, ifindex as usize, l2_addr) {
            return Err(InterfaceErr::new(format!(
                "Cannot create interface, errno {}",
                errno
            )))
            .map_err(From::from);
        };
        Ok(())
    }

    fn handle_add_ip(&self, ifname: String, ip_and_mask: String) -> thrift::Result<()> {
        let mut r2 = self.r2.lock().unwrap();
        let intf;
        let addr;
        let masklen;
        let ifindex;
        if let Some(i) = r2.ifd.interfaces.get(&ifname) {
            intf = i;
            ifindex = intf.ifindex;
        } else {
            return Err(InterfaceErr::new(format!(
                "Cannot find interface {}",
                ifname
            )))
            .map_err(From::from);
        }
        if let Some((a, m)) = fwd::ip_mask_decode(&ip_and_mask) {
            addr = a;
            masklen = m;
        } else {
            return Err(InterfaceErr::new(format!("Bad IP/MASK {}", ip_and_mask)))
                .map_err(From::from);
        }
        if addr == ZERO_IP || masklen == 0 {
            return Err(InterfaceErr::new(format!("ZERO IP/MASK {}", ip_and_mask)))
                .map_err(From::from);
        }
        let (cur_addr, cur_masklen) = intf.get_v4addr();
        let mut new_intf = (**intf).clone();
        new_intf.set_v4addr(addr, masklen);
        // We broadcast a message to forwarding threads with a copy of the new interface
        // parameters, and the forwarding threads are expected to swap out the old interface
        // structure with the new one
        let msg = R2Msg::ModifyInterface(ModifyInterfaceMsg {
            intf: Arc::new(new_intf),
        });
        r2.broadcast(msg);
        drop(r2);
        // Delete the old connected route corresponding to the old IP,
        // and add a new connected route for the new IP
        let mut r2 = self.r2.lock().unwrap();
        if cur_addr != ZERO_IP {
            del_route(
                &mut r2,
                cur_addr,
                cur_masklen,
                Ipv4Addr::new(0, 0, 0, 0),
                ifindex,
            );
        }
        add_route(&mut r2, addr, masklen, Ipv4Addr::new(0, 0, 0, 0), ifindex);
        Ok(())
    }

    fn handle_add_class(
        &self,
        ifname: String,
        name: String,
        parent: String,
        qlimit: i32,
        is_leaf: bool,
        curves: CurvesApi,
    ) -> thrift::Result<()> {
        let mut r2 = self.r2.lock().unwrap();
        let intf;
        if let Some(i) = r2.ifd.interfaces.get(&ifname) {
            intf = i;
        } else {
            return Err(InterfaceErr::new(format!(
                "Cannot find interface {}",
                ifname
            )))
            .map_err(From::from);
        }
        let curves = unwrap_curves(&curves);
        // For QoS, we cant really have the control thread update a copy of the QoS heirarchy and
        // send a message asking forwarding threads to swap to the new one - even though thats what
        // we would have liked to do. We cant do that because the QoS heirarchy and the data in it
        // keeps changing with every packet, so we cant do that without locking the corresponding
        // IfNode. Between locking an IfNode and sending a message to the thread hosting the
        // IfNode, we prefer the message model.
        let class = ClassAddMsg {
            ifindex: intf.ifindex,
            name,
            parent,
            qlimit: qlimit as usize,
            is_leaf,
            curves,
        };
        r2.broadcast(R2Msg::ClassAdd(class));
        Ok(())
    }
}
