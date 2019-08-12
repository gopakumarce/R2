use super::*;

pub struct Adjacency {
    pub nhop: Ipv4Addr,
    pub ifindex: usize,
}

impl Adjacency {
    pub fn new(nhop: Ipv4Addr, ifindex: usize) -> Adjacency {
        Adjacency { nhop, ifindex }
    }
}
