use super::{Arc, Fwd, IpLookupTable, Ipv4Addr};

pub struct IPv4TableMsg {
    pub table: Arc<IPv4Table>,
}

impl IPv4TableMsg {
    pub fn new(table: Arc<IPv4Table>) -> IPv4TableMsg {
        IPv4TableMsg { table }
    }
}

impl Clone for IPv4TableMsg {
    fn clone(&self) -> IPv4TableMsg {
        IPv4TableMsg {
            table: self.table.clone(),
        }
    }
}

pub struct IPv4Leaf {
    pub next: Fwd,
}

impl IPv4Leaf {
    pub fn new(fwd: Fwd) -> IPv4Leaf {
        IPv4Leaf { next: fwd }
    }
}

#[derive(Default)]
pub struct IPv4Table {
    pub root: IpLookupTable<Ipv4Addr, Arc<IPv4Leaf>>,
}

impl IPv4Table {
    pub fn new() -> IPv4Table {
        IPv4Table {
            root: IpLookupTable::new(),
        }
    }

    pub fn add(&mut self, ip: Ipv4Addr, masklen: u32, value: Arc<IPv4Leaf>) -> bool {
        let dup = self.root.insert(ip, masklen, value);
        dup.is_none()
    }

    pub fn del(&mut self, ip: Ipv4Addr, masklen: u32) -> bool {
        let ret = self.root.remove(ip, masklen);
        ret.is_some()
    }
}
