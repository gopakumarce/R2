use super::*;
use std::cmp;

// Three flavors of counters: A single u64, two u64s and a vector of u64s
// We use the code lib.rs which allocates a set of contiguous counters. The
// raw counters are not exposed by the library, instead we expose the flavours
// of counters as mentioned above. The raw counters deal with raw addresses,
// the flavours here hide the addresses and provide incr() / decr() APIs.

pub enum CounterType {
    Error,
    Info,
    Pkts,
}

pub struct Counter {
    dir: u64,
    count: u64,
}

fn counter_name(node: &str, ctype: CounterType, name: &str) -> String {
    let mut cntr = node.to_string();
    match ctype {
        flavors::CounterType::Error => cntr.push_str("/error/"),
        flavors::CounterType::Info => cntr.push_str("/info/"),
        flavors::CounterType::Pkts => cntr.push_str("/pkts/"),
    }
    cntr.push_str(name);
    let max_namelen = (NAME.binsz * NAME.binmax) as usize;
    if cntr.len() > max_namelen {
        cntr.truncate(max_namelen);
    }
    cntr
}

impl Counter {
    pub fn incr(&self) {
        self.add(1);
    }

    pub fn decr(&mut self) {
        self.sub(1);
    }

    pub fn add(&self, val: u64) {
        unsafe {
            let count = self.count as *mut u64;
            *count += val;
        }
    }

    pub fn sub(&self, val: u64) {
        unsafe {
            let count = self.count as *mut u64;
            *count -= val;
        }
    }

    pub fn new(counters: &mut Counters, node: &str, ctype: CounterType, name: &str) -> Counter {
        let (mut dir, mut base) = counters.get(&counter_name(node, ctype, name), 1);
        if dir == 0 {
            dir = counters.dummies.counter.dir;
            base = counters.dummies.counter.base;
        }
        Counter { dir, count: base }
    }

    #[allow(dead_code)]
    pub fn free(&self, counters: &mut Counters) {
        if self.dir != counters.dummies.counter.dir {
            counters.free(self.dir);
        }
    }
}

pub struct PktsBytes {
    dir: u64,
    pkts: u64,
    bytes: u64,
}

impl PktsBytes {
    pub fn incr(&self, val: u64) {
        self.add(1, val)
    }

    pub fn decr(&self, val: u64) {
        self.sub(1, val)
    }

    pub fn add(&self, pkts: u64, bytes: u64) {
        unsafe {
            let count = self.pkts as *mut u64;
            *count += pkts;
            let count = self.bytes as *mut u64;
            *count += bytes;
        }
    }

    pub fn sub(&self, pkts: u64, bytes: u64) {
        unsafe {
            let count = self.pkts as *mut u64;
            *count -= pkts;
            let count = self.bytes as *mut u64;
            *count -= bytes;
        }
    }

    pub fn new(counters: &mut Counters, node: &str, ctype: CounterType, name: &str) -> PktsBytes {
        let (mut dir, mut base) = counters.get(&counter_name(node, ctype, name), 2);
        if dir == 0 {
            dir = counters.dummies.pktsbytes.dir;
            base = counters.dummies.pktsbytes.base;
        }
        PktsBytes {
            dir,
            pkts: base,
            bytes: base + VEC.binsz as u64,
        }
    }

    #[allow(dead_code)]
    pub fn free(&self, counters: &mut Counters) {
        if self.dir != counters.dummies.pktsbytes.dir {
            counters.free(self.dir);
        }
    }
}

pub struct CounterArray {
    dir: u64,
    array: Vec<u64>,
}

impl CounterArray {
    pub fn incr(&self, index: usize) {
        self.add(index, 1)
    }

    pub fn decr(&self, index: usize) {
        self.sub(index, 1)
    }

    pub fn add(&self, index: usize, val: u64) {
        unsafe {
            let count = self.array[index] as *mut u64;
            *count += val
        }
    }

    pub fn sub(&self, index: usize, val: u64) {
        unsafe {
            let count = self.array[index] as *mut u64;
            *count -= val
        }
    }

    pub fn new(
        counters: &mut Counters,
        node: &str,
        ctype: CounterType,
        name: &str,
        size: usize,
    ) -> CounterArray {
        let veclen = cmp::min(size, VEC.binmax as usize);
        let (mut dir, mut base) = counters.get(&counter_name(node, ctype, name), veclen);
        if dir == 0 {
            dir = counters.dummies.array.dir;
            base = counters.dummies.array.base;
        }
        let mut vec = Vec::new();
        for i in 0..veclen {
            vec.push(base + i as u64 * VEC.binsz as u64);
        }
        CounterArray { dir, array: vec }
    }

    #[allow(dead_code)]
    pub fn free(&self, counters: &mut Counters) {
        if self.dir != counters.dummies.array.dir {
            counters.free(self.dir);
        }
    }
}

#[derive(Default, Clone)]
pub struct CounterRO {
    val: Vec<u64>,
}

impl CounterRO {
    pub fn new(base: u64, len: u32) -> CounterRO {
        assert!(len <= VEC.binsz * VEC.binmax);
        let mut val = vec![];
        for i in 0..len / VEC.binsz {
            val.push(base + i as u64 * VEC.binsz as u64);
        }
        CounterRO { val }
    }

    pub fn search(
        parent: &CountersRO,
        node: &str,
        ctype: CounterType,
        name: &str,
    ) -> Option<CounterRO> {
        if let Some(cntr) = parent.hash.get(&counter_name(node, ctype, name)) {
            Some(cntr.clone())
        } else {
            None
        }
    }

    pub fn num_cntrs(&self) -> usize {
        self.val.len()
    }

    pub fn read(&self, index: usize) -> u64 {
        unsafe { *(self.val[index] as *const u64) }
    }
}
