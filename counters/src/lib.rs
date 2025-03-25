use self::flavors::CounterRO;
use bin::Bin;
use shm::{shm_close, shm_open_ro, shm_open_rw, shm_unlink};
use std::collections::HashMap;
use std::mem::size_of;
use std::str;

// This module provides counters in shared memory
// The shared memory is divided into four areas:
// 0. struct Hdr
// 1. A Directory area which leads us to counter names and values
// 2. Area for the counter values themselves
// 3. Area for counter names
//
// The directory has the first 4Mb of shm space, values gets 16Mb and the names
// come after that. The values and names are allocated in power of twos and the
// free ones go back to a simple power of two bin from which we reallocate them

const MMAP_SIZE: usize = common::MB!(64);

// Holds the directory entries
const DIR: BinInfo = BinInfo {
    binsz: size_of::<Dir>() as u32,
    binmax: 1,
    pagesz: common::KB!(4),
    start: 64,
    totsz: common::MB!(4),
};

// Holds the actual counters
const VEC: BinInfo = BinInfo {
    binsz: size_of::<u64>() as u32,
    binmax: 32,
    pagesz: common::KB!(4),
    start: DIR.start + DIR.totsz,
    totsz: common::MB!(16),
};

// Holds the names of the counters
const NAME: BinInfo = BinInfo {
    binsz: 32,
    binmax: 2,
    pagesz: common::KB!(4),
    start: VEC.start + VEC.totsz,
    totsz: common::MB!(32),
};

struct Hdr {
    num_counters: u32,
}

struct BinInfo {
    // The objects in each bin are a multiple of binsz, max multiple being binmax
    binsz: u32,
    // The largest object we can ask for is binsz * binmax
    binmax: u32,
    // We want to allocate objects of at least pagesz/object-size in count
    pagesz: u32,
    // Assuming we carve up a large memory range into multiple ranges for different
    // purposes, the first one starts at offset 0, and this one starts at 'start'
    start: u64,
    // max size of this memory range
    totsz: u64,
}

// One particular directory entry. It holds an offset into the name area for the name
// of this counter, a length of the name string, offset into the counter area for the
// actual counter/counters, and length of the counters
#[derive(Default, Copy, Clone)]
struct Dir {
    name_off: u32,
    name_len: u32,
    vec_off: u32,
    vec_len: u32,
}

#[derive(Default, Copy, Clone)]
struct Dummy {
    dir: u64,
    base: u64,
}

struct Dummies {
    counter: Dummy,
    pktsbytes: Dummy,
    array: Dummy,
}

pub struct Counters {
    shname: String,
    fd: i32,
    base: u64,
    dir: Bin,
    vec: Bin,
    names: Bin,
    dummies: Dummies,
}

impl Counters {
    // If we run out of counters, we dont want to panic or make the caller to bail out and
    // take special actions etc.. It is very very possible to run out of counters, and usually
    // its fine to let the program just continue in that case. So we provide a dummy counter,
    // the same dummy counter that will be given to everyone who asks for counters and runs out.
    fn dummies(&mut self) {
        let (dir, base) = self.get("dummy1", 1);
        self.dummies.counter.dir = dir;
        self.dummies.counter.base = base;
        let (dir, base) = self.get("dummy2", 2);
        self.dummies.pktsbytes.dir = dir;
        self.dummies.pktsbytes.base = base;
        let (dir, base) = self.get("dummyN", VEC.binmax as usize);
        self.dummies.array.dir = dir;
        self.dummies.array.base = base;
    }

    /// Allocate a new counter pool, open a named shared memory of 'name'
    pub fn new(name: &str) -> Result<Counters, i32> {
        assert!(DIR.start >= size_of::<Hdr>() as u64);
        let (fd, base) = shm_open_rw(name, MMAP_SIZE);
        if base == 0 {
            return Err(fd);
        }

        let dummy = Dummy { dir: 0, base: 0 };
        let mut counters = Counters {
            shname: name.to_string(),
            fd,
            base,
            dir: Bin::new(DIR.binsz, DIR.totsz, DIR.pagesz),
            vec: Bin::new(VEC.binsz, VEC.totsz, VEC.pagesz),
            names: Bin::new(NAME.binsz, NAME.totsz, NAME.pagesz),
            dummies: Dummies {
                counter: dummy,
                pktsbytes: dummy,
                array: dummy,
            },
        };
        unsafe {
            let hdr = base as *mut Hdr;
            (*hdr).num_counters = 0;
        }
        counters.dummies();
        Ok(counters)
    }

    // Allocate a counter with 'name', contiguous 'nvecs' 64 bit counters
    fn get(&mut self, name: &str, nvecs: usize) -> (u64, u64) {
        let mut ret = (0, 0);
        let veclen = nvecs as u32 * VEC.binsz;
        let daddr = self.dir.get(DIR.binsz);
        let vaddr = self.vec.get(veclen);
        let naddr = self.names.get(name.len() as u32);
        if let Some(daddr) = daddr {
            if let Some(vaddr) = vaddr {
                if let Some(naddr) = naddr {
                    let daddr = daddr + self.base + DIR.start;
                    let vaddr = vaddr + self.base + VEC.start;
                    let naddr = naddr + self.base + NAME.start;
                    unsafe {
                        let bytes = name.as_bytes();
                        for (n, byte) in bytes.iter().enumerate() {
                            let n8 = (naddr + n as u64) as *mut u8;
                            *n8 = *byte;
                        }
                        let d = daddr as *mut Dir;
                        (*d).name_off = (naddr - self.base) as u32;
                        (*d).name_len = name.len() as u32;
                        (*d).vec_off = (vaddr - self.base) as u32;
                        (*d).vec_len = veclen;
                        // This is the total number of counters allocated, might not be in-use
                        let hdr = self.base as *mut Hdr;
                        (*hdr).num_counters = self.dir.offset() as u32 / DIR.binsz;
                    }
                    ret = (daddr, vaddr)
                } else {
                    self.dir.free(daddr, veclen);
                    self.vec.free(vaddr, veclen);
                }
            } else {
                self.dir.free(daddr, veclen);
            }
        }
        ret
    }

    // Free a counter indicated by a 'dir' directory entry, return the name address
    // to the name bin, return the counter address (potentially more than one
    // contiguous counter) to its bin, then return the dir address itself to the dir bin
    #[allow(dead_code)]
    fn free(&mut self, dir: u64) {
        unsafe {
            let d = dir as *mut Dir;
            let noff = (*d).name_off as u64 - NAME.start;
            self.names.free(noff, (*d).name_len);
            let voff = (*d).vec_off as u64 - VEC.start;
            self.vec.free(voff, (*d).vec_len);
            (*d).name_off = 0;
            (*d).name_len = 0;
            (*d).vec_off = 0;
            (*d).vec_len = 0;
            let dir = dir - (self.base + DIR.start);
            self.dir.free(dir, DIR.binsz);
        }
    }
}

impl Drop for Counters {
    fn drop(&mut self) {
        shm_close(self.fd);
        shm_unlink(&self.shname[0..]);
    }
}

pub struct CountersRO {
    fd: i32,
    base: u64,
    pub hash: HashMap<String, CounterRO>,
}

impl CountersRO {
    /// A readonly version of the current set of counters (at the time of reading
    /// shared memory). Walk through the directory entries and create a hashmap of
    /// the names of each counter and the address of the counter
    pub fn new(name: &str) -> Result<CountersRO, i32> {
        let (fd, base) = shm_open_ro(name, MMAP_SIZE);
        if base == 0 {
            return Err(fd);
        }
        let mut counters = CountersRO {
            fd,
            base,
            hash: HashMap::new(),
        };
        unsafe {
            let hdr = counters.base as *mut Hdr;
            for i in 0..(*hdr).num_counters {
                let d = (counters.base + DIR.start + (i * DIR.binsz) as u64) as *mut Dir;
                let dir: Dir = *d;
                if dir.name_len == 0
                    || dir.vec_len == 0
                    || dir.name_len > NAME.binmax * NAME.binsz
                    || dir.vec_len > VEC.binmax * VEC.binsz
                {
                    continue;
                }
                let mut vec_names = vec![];
                for i in 0..dir.name_len {
                    let names = (counters.base + (dir.name_off + i) as u64) as *const u8;
                    vec_names.push(*names);
                }
                let name = str::from_utf8(&vec_names[0..]).unwrap_or("UNKNOWN");
                let cntr = CounterRO::new(counters.base + dir.vec_off as u64, dir.vec_len);
                counters.hash.insert(name.to_string(), cntr);
            }
        }
        Ok(counters)
    }
}

impl Drop for CountersRO {
    fn drop(&mut self) {
        shm_close(self.fd);
    }
}

mod bin;
pub mod flavors;

#[cfg(test)]
mod test;
