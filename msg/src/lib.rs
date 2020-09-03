use counters::Counters;
use fwd::intf::ModifyInterfaceMsg;
use fwd::ipv4::IPv4TableMsg;
use fwd::EthMacAddMsg;
use graph::{Gclient, GnodeInit};
use log::Logger;
use std::sync::Arc;

pub enum R2Msg {
    GnodeAdd(GnodeAddMsg),
    EpollAdd(EpollAddMsg),
    IPv4TableAdd(IPv4TableMsg),
    ModifyInterface(ModifyInterfaceMsg),
    EthMacAdd(EthMacAddMsg),
    ClassAdd(ClassAddMsg),
}

impl R2Msg {
    pub fn clone(&self, counters: &mut Counters, logger: Arc<Logger>) -> Self {
        match self {
            R2Msg::GnodeAdd(gnode_add) => R2Msg::GnodeAdd(gnode_add.clone(counters, logger)),
            R2Msg::EpollAdd(epoll_add) => R2Msg::EpollAdd(epoll_add.clone()),
            R2Msg::IPv4TableAdd(table_add) => R2Msg::IPv4TableAdd(table_add.clone()),
            R2Msg::ModifyInterface(mod_intf) => R2Msg::ModifyInterface(mod_intf.clone()),
            R2Msg::EthMacAdd(mac_add) => R2Msg::EthMacAdd(mac_add.clone()),
            R2Msg::ClassAdd(class) => R2Msg::ClassAdd(class.clone()),
        }
    }
}

pub struct GnodeAddMsg {
    pub node: Box<dyn Gclient<R2Msg>>,
    pub init: GnodeInit,
}

impl GnodeAddMsg {
    pub fn clone(&self, counters: &mut Counters, logger: Arc<Logger>) -> Self {
        GnodeAddMsg {
            node: self.node.clone(counters, logger),
            init: self.init.clone(counters),
        }
    }
}

pub struct EpollAddMsg {
    pub fd: Option<i32>,
    pub thread: usize,
}

impl Clone for EpollAddMsg {
    fn clone(&self) -> EpollAddMsg {
        EpollAddMsg {
            fd: self.fd,
            thread: self.thread,
        }
    }
}

#[derive(Copy, Clone, Default)]
pub struct Sc {
    pub m1: u64,
    pub d: usize,
    pub m2: u64,
}

#[derive(Copy, Clone, Default)]
pub struct Curves {
    pub r_sc: Option<Sc>,
    pub u_sc: Option<Sc>,
    pub f_sc: Sc,
}

pub struct ClassAddMsg {
    pub ifindex: usize,
    pub name: String,
    pub parent: String,
    pub qlimit: usize,
    pub is_leaf: bool,
    pub curves: Curves,
}

impl Clone for ClassAddMsg {
    fn clone(&self) -> ClassAddMsg {
        ClassAddMsg {
            ifindex: self.ifindex,
            name: self.name.clone(),
            parent: self.parent.clone(),
            qlimit: self.qlimit,
            is_leaf: self.is_leaf,
            curves: self.curves,
        }
    }
}
