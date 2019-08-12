use counters::flavors::{Counter, CounterType};
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use fwd::ZERO_IP;
use std::cmp::min;
use std::mem;
use std::net::Ipv4Addr;
use std::ops::{Deref, DerefMut};
use std::ptr::copy_nonoverlapping;
use std::slice::{from_raw_parts, from_raw_parts_mut};
use std::sync::Arc;

// A packet is composed of a chain of particles. The particle has some meta data
// and a 'raw' buffer of size 'rlen', which is what holds the actual data. Every
// particle in a packet will have the same fixed size 'raw' buffers. Though we
// dont mandate it, usually all particles in the entire system will have the same
// fixed raw buffer size. Particle structure is not directly accessed by any client,
// its public just because the packet/particle pool implementations provided outside,
// need to know about Particle
pub struct Particle {
    raw: *const u8,
    rlen: usize,
    head: usize,
    tail: usize,
    next: Option<BoxPart>,
}

impl Particle {
    pub fn new(raw: *const u8, rlen: usize) -> Particle {
        Particle {
            raw,
            rlen,
            head: 0,
            tail: 0,
            next: None,
        }
    }

    // reinit is called on a particle thats was used before and given back to the
    // particle pool and now being allocated again from the pool
    pub fn reinit(&mut self, headroom: usize) {
        assert!(headroom <= self.rlen);
        self.head = headroom;
        self.tail = headroom;
        self.next = None;
    }

    fn len(&self) -> usize {
        self.tail - self.head
    }

    fn slice(&self) -> &[u8] {
        unsafe { from_raw_parts(self.raw, self.rlen) }
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        unsafe { from_raw_parts_mut(self.raw as *mut u8, self.rlen) }
    }

    fn data(&self, offset: usize) -> Option<(&[u8], usize)> {
        if offset >= self.len() {
            return None;
        }
        Some((
            &self.slice()[self.head + offset..self.tail],
            self.len() - offset,
        ))
    }

    fn data_raw(&self, offset: usize) -> &[u8] {
        if offset >= self.rlen {
            &[]
        } else {
            &self.slice()[offset..]
        }
    }

    fn data_raw_mut(&mut self, offset: usize) -> &mut [u8] {
        if offset >= self.rlen {
            &mut []
        } else {
            &mut self.slice_mut()[offset..]
        }
    }

    // Add data behind the 'head', there might not be room for all the data, add
    // as much as possible, return how much was added
    fn prepend(&mut self, data: &[u8]) -> usize {
        let dlen = data.len();
        unsafe {
            if dlen > self.head {
                let count = self.head;
                let src = data.as_ptr().add(dlen - count);
                let dst = self.raw as *mut u8;
                copy_nonoverlapping(src, dst, count);
                self.head = 0;
                count
            } else {
                let count = dlen;
                let src = data.as_ptr().offset(0);
                let dst = (self.raw as *mut u8).add(self.head - count);
                copy_nonoverlapping(src, dst, count);
                self.head -= count;
                count
            }
        }
    }

    // Add data after the 'tail', there might not be room for all the data, add
    // as much as possible, return how much was added
    fn append(&mut self, data: &[u8]) -> usize {
        unsafe {
            let len = min(self.rlen - self.tail, data.len());
            let dst = (self.raw as *mut u8).add(self.tail);
            let src = data.as_ptr().offset(0);
            copy_nonoverlapping(src, dst, len);
            self.tail += len;
            len
        }
    }

    fn move_tail(&mut self, mv: isize) -> isize {
        let len = self.rlen as isize;
        let head = self.head as isize;
        let tail = self.tail as isize;
        let new_tail = tail + mv;
        if new_tail < head || new_tail > len {
            0
        } else {
            self.tail = new_tail as usize;
            mv
        }
    }

    fn move_head(&mut self, mv: isize) -> isize {
        let head = self.head as isize;
        let tail = self.tail as isize;
        let new_head = head + mv;
        if new_head < 0 || new_head > tail {
            0
        } else {
            self.head = new_head as usize;
            mv
        }
    }

    fn last_particle(&mut self) -> &mut Particle {
        if self.next.is_none() {
            self
        } else {
            self.next.as_mut().unwrap().last_particle()
        }
    }
}

// A BoxPart basically is a pointer to a Particle structure, this allows particles
// to come from pools, where the pool implementor has freedom to decide what memory
// is used for the particle raw data and even the Particle structure itself
pub struct BoxPart(pub *mut Particle);

impl Drop for BoxPart {
    fn drop(&mut self) {}
}

// Deref mechanisms to allow accessing a BoxPart as a Particle
impl Deref for BoxPart {
    type Target = Particle;

    fn deref(&self) -> &Particle {
        unsafe { &*self.0 }
    }
}

impl DerefMut for BoxPart {
    fn deref_mut(&mut self) -> &mut Particle {
        unsafe { &mut *self.0 }
    }
}

/// The network packet structure is made up of some metadata stored in this
/// structure plus a chain of Particles which actually hold the real network
/// data. The packet hides the fact that data is a chain of particles, it
/// makes it appear as if the data is one big buffer. So all the offsets etc..
/// the packet refers to is the offsets into that one big buffer. Each packet
/// has a 'headroom' - some empty space at the beginning of the packet that
/// allows data to be 'pushed' to the beginning of the packet without allocating
/// new particles etc.. And the headroom of the packet is actually headroom
/// of the first particle of the packet. So typically the first particle in the
/// chain will have non-zero headroom and the other particles have zero headroom
/// And when we speak of any data offsets in the packet, its always with reference
/// to the headroom - there is no data before the headroom, so headroom is
/// offset zero, which is the first byte of data in the packet
/// The Packet structure is never directly used by clients, clients will use
/// BoxPkt structure, this is kept public just for packet/particle pool implementations
/// outside the file which needs to know about these structures
pub struct Packet {
    // The first particle
    particle: BoxPart,
    // Total length of data in the packet
    length: usize,
    // The offset (from headroom) of the layer2 header
    l2: usize,
    // Size of layer2 header
    l2_len: usize,
    // The offset (from headroom) of the layer3 header
    l3: usize,
    // Size of Layer3 header
    l3_len: usize,
    /// The pool from which this packet was allocated
    pub pool: Arc<dyn PacketPool>,
    /// The ifindex of the interface on which this packet came in
    pub in_ifindex: usize,
    /// The ifindex of the interface on which the packet will go out of
    pub out_ifindex: usize,
    /// The next-hop IPv4 address out of out_ifindex, to use for ARP
    pub out_l3addr: Ipv4Addr,
}

#[allow(clippy::len_without_is_empty)]
impl Packet {
    pub fn new(pool: Arc<dyn PacketPool>, particle: BoxPart) -> Packet {
        Packet {
            particle,
            length: 0,
            l2: 0,
            l2_len: 0,
            l3: 0,
            l3_len: 0,
            pool,
            in_ifindex: 0,
            out_ifindex: 0,
            out_l3addr: ZERO_IP,
        }
    }

    // reinit() is called on packets which were previously used and returned to the packet pool,
    // and now its being allocated from the pool again
    pub fn reinit(&mut self, headroom: usize) {
        self.length = 0;
        self.l2 = 0;
        self.l2_len = 0;
        self.l3 = 0;
        self.l3_len = 0;
        self.l3_len = 0;
        self.in_ifindex = 0;
        self.out_ifindex = 0;
        self.out_l3addr = ZERO_IP;
        self.particle.reinit(headroom);
    }

    fn push_particle(&mut self, next: BoxPart) {
        let p = self.particle.last_particle();
        p.next = Some(next);
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn headroom(&self) -> usize {
        self.particle.head
    }

    pub fn prepend(&mut self, bytes: &[u8]) -> bool {
        let mut l = bytes.len();
        while l != 0 {
            let n = self.particle.prepend(&bytes[0..l]);
            if n != l {
                let p = self.pool.particle(self.pool.particle_sz());
                if p.is_none() {
                    return false;
                }
                let p = p.unwrap();
                let prev = mem::replace(&mut self.particle, p);
                self.particle.next = Some(prev);
            }
            l -= n;
        }
        self.length += bytes.len();
        true
    }

    pub fn append(&mut self, bytes: &[u8]) -> bool {
        let mut offset = 0;
        while offset != bytes.len() {
            let p = self.particle.last_particle();
            let n = p.append(&bytes[offset..]);
            offset += n;
            if n == 0 {
                let p = self.pool.particle(0);
                if p.is_none() {
                    return false;
                }
                let p = p.unwrap();
                self.push_particle(p);
            }
        }
        self.length += bytes.len();
        true
    }

    pub fn move_tail(&mut self, mv: isize) -> isize {
        let p = self.particle.last_particle();
        if p.move_tail(mv) != mv {
            0
        } else {
            let len = self.length as isize;
            self.length = (len + mv) as usize;
            mv
        }
    }

    fn move_head(&mut self, mv: isize) -> isize {
        let p = &mut self.particle;
        if p.move_head(mv) != mv {
            0
        } else {
            let len = self.length as isize;
            self.length = (len - mv) as usize;
            mv
        }
    }

    // Consider the first byte of the packet as the l2 header, of 'len' bytes,
    // and move the first byte of the packet beyond the l2 header
    pub fn pull_l2(&mut self, len: usize) -> usize {
        let p = &self.particle;
        let l2 = p.head;
        let mv = len as isize;
        if self.move_head(mv) != mv {
            0
        } else {
            self.l2 = l2;
            self.l2_len = len;
            len
        }
    }

    // the 'bytes' worth of data is the layer2 header that we want to add to the
    // head of the packet
    pub fn push_l2(&mut self, bytes: &[u8]) -> bool {
        if !self.prepend(bytes) {
            return false;
        }
        let p = &self.particle;
        self.l2 = p.head;
        self.l2_len = bytes.len();
        true
    }

    pub fn set_l2(&mut self, len: usize) -> bool {
        let p = &self.particle;
        if p.len() >= len {
            self.l2 = p.head;
            self.l2_len = len;
            true
        } else {
            false
        }
    }

    pub fn get_l2(&self) -> (&[u8], usize) {
        if self.l2_len == 0 {
            (&[], 0)
        } else {
            let p = &self.particle;
            let d = p.data_raw(self.l2);
            if d.len() < self.l2_len {
                (&[], 0)
            } else {
                (d, self.l2_len)
            }
        }
    }

    // Consider the first byte of the packet as the l3 header, of 'len' bytes,
    // and move the first byte of the packet beyond the l3 header
    pub fn pull_l3(&mut self, len: usize) -> usize {
        let p = &self.particle;
        let l3 = p.head;
        let mv = len as isize;
        if self.move_head(mv) != mv {
            0
        } else {
            self.l3 = l3;
            self.l3_len = len;
            len
        }
    }

    // the 'bytes' worth of data is the layer3 header that we want to add to the
    // head of the packet
    pub fn push_l3(&mut self, bytes: &[u8]) -> bool {
        if !self.prepend(bytes) {
            return false;
        }
        let p = &self.particle;
        self.l3 = p.head;
        self.l3_len = bytes.len();
        true
    }

    pub fn set_l3(&mut self, len: usize) -> bool {
        let p = &self.particle;
        if p.len() >= len {
            self.l3 = p.head;
            self.l3_len = len;
            true
        } else {
            false
        }
    }

    pub fn get_l3(&self) -> (&[u8], usize) {
        if self.l3_len == 0 {
            (&[], 0)
        } else {
            let p = &self.particle;
            let d = p.data_raw(self.l3);
            if d.len() < self.l3_len {
                (&[], 0)
            } else {
                (d, self.l3_len)
            }
        }
    }

    pub fn data(&self, offset: usize) -> Option<(&[u8], usize)> {
        let mut l = 0;
        let mut p = &self.particle;
        loop {
            let d = p.data(offset - l);
            if d.is_some() {
                return d;
            }
            l += p.len();
            if let Some(ref next) = p.next {
                p = next;
            } else {
                break;
            }
        }
        None
    }

    pub fn data_raw(&self) -> &[u8] {
        let p = &self.particle;
        p.data_raw(0)
    }

    pub fn data_raw_mut(&mut self) -> &mut [u8] {
        let p = &mut self.particle;
        p.data_raw_mut(0)
    }

    pub fn slices(&self) -> Vec<(&[u8], usize)> {
        let mut v = Vec::new();
        let mut p = &self.particle;
        loop {
            if let Some(t) = p.data(0) {
                v.push(t);
            }
            if let Some(ref next) = p.next {
                p = next;
            } else {
                break;
            }
        }
        v
    }
}

/// The clients will all deal with BoxPkt structure - its nothing but a pointer
/// to the Packet structure. The Packet structure memory can come from anywhere
/// that the packet pool implementor choses, but of course the memory has to be
/// valid across all threads in R2 because we can send packets from one thread
/// to another.
pub struct BoxPkt(pub *mut Packet);

/// By default because BoxPkt is a pointer to a Packet, it wont be Send/Sync because
/// Rust will not allow pointers/addresses to be shared across threads. We override
/// it here because we have the _guarantee_ that these addresses are valid across all
/// threads in R2
unsafe impl Send for BoxPkt {}
unsafe impl Sync for BoxPkt {}

// Deref mechanisms to allow accessing a BoxPkt as Packet
impl Deref for BoxPkt {
    type Target = Packet;

    fn deref(&self) -> &Packet {
        unsafe { &*self.0 }
    }
}

impl DerefMut for BoxPkt {
    fn deref_mut(&mut self) -> &mut Packet {
        unsafe { &mut *self.0 }
    }
}

// When the packet is dropped, except the first particle, give all the other particles
// back to the particle pool. And then give the packet (with the first particle intact)
// also back to the pool. In other words an alloc from a packet pool is more optimized
// for the case of a 'single particle packet'
impl Drop for BoxPkt {
    fn drop(&mut self) {
        let mut part = self.particle.next.take();
        while let Some(mut p) = part {
            let next = p.next.take();
            // The particle goes back to the pool after this, do not touch
            // it anymore
            self.pool.free_part(&p);
            part = next;
        }
        // The packet goes back to the pool after this, do not touch
        // it anymore
        self.pool.free_pkt(self);
    }
}

/// External clients are free to implement their own versions of a packet pool, the pool
/// should provide the below methods. And all the addresses/memory in the pool should be
/// valid across all R2 threads. Also the pool should be implemented in a thread safe
/// manner since packets can move across threads in R2 - and hence also why we have
/// Send + Sync in the trait.
pub trait PacketPool: Send + Sync {
    /// Allocate a packet with one particle. Expect allocation failures - hence the Option return
    fn pkt(&self, headroom: usize) -> Option<BoxPkt>;
    /// Allocate a particle (with the raw data), again expect allocation failure
    fn particle(&self, headroom: usize) -> Option<BoxPart>;
    /// Free a packet which has a single particle with it
    fn free_pkt(&self, pkt: &BoxPkt);
    /// Free a particle
    fn free_part(&self, part: &BoxPart);
    /// Return the fixed max-size of the particle's raw data buffer
    fn particle_sz(&self) -> usize;
    /// Free the entire pool
    fn free(&self);
}

/// Here we provide a default packet pool implementation, where the Packet, Particle and
/// the particle's raw data buffer all comes from the heap (using Box/Vec). We use the
/// fixed size MPSC lockfree ArrayQueue for thread safety
pub struct PktsHeap {
    alloc_fail: Counter,
    pkts: ArrayQueue<BoxPkt>,
    particles: ArrayQueue<BoxPart>,
    particle_sz: usize,
}

/// The pool has to be thread safe and can be used across threads. Since the pool deals
/// with raw pointers, by default its not Send + Sync. Here since we _know_ that the
/// addresses are all from the heap (Box/Vec) and are valid across threads, we override
/// and force impl Send + Sync
unsafe impl Send for PktsHeap {}
unsafe impl Sync for PktsHeap {}

/// A from-heap packet/particle pool, the pool is created with a specification of the
/// number of packets, number of particles and max-size of each particle
impl PktsHeap {
    pub fn new(
        counters: &mut Counters,
        num_pkts: usize,
        num_parts: usize,
        particle_sz: usize,
    ) -> Arc<PktsHeap> {
        assert!(num_parts >= num_pkts);
        let parts_left = num_parts - num_pkts;
        let particles = ArrayQueue::new(parts_left);
        let pkts = ArrayQueue::new(num_pkts);
        let alloc_fail = Counter::new(counters, "PKTS_HEAP", CounterType::Error, "PktAllocFail");
        let pool = Arc::new(PktsHeap {
            alloc_fail,
            pkts,
            particles,
            particle_sz,
        });

        for _ in 0..num_pkts {
            let raw: Box<[u8]> = vec![0; particle_sz].into_boxed_slice();
            let part = Box::new(Particle::new(raw.as_ptr(), particle_sz));
            Box::leak(raw);
            let pkt = Box::new(Packet::new(pool.clone(), BoxPart(Box::into_raw(part))));
            pool.pkts.push(BoxPkt(Box::into_raw(pkt))).unwrap();
        }

        for _ in 0..parts_left {
            let raw: Box<[u8]> = vec![0; particle_sz].into_boxed_slice();
            let part = Box::new(Particle::new(raw.as_ptr(), particle_sz));
            Box::leak(raw);
            pool.particles.push(BoxPart(Box::into_raw(part))).unwrap();
        }

        pool
    }
}

impl PacketPool for PktsHeap {
    fn pkt(&self, headroom: usize) -> Option<BoxPkt> {
        if let Ok(mut pkt) = self.pkts.pop() {
            (*pkt).reinit(headroom);
            Some(pkt)
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn particle(&self, headroom: usize) -> Option<BoxPart> {
        if let Ok(mut part) = self.particles.pop() {
            (*part).reinit(headroom);
            Some(part)
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn free_pkt(&self, pkt: &BoxPkt) {
        self.pkts.push(BoxPkt(pkt.0)).unwrap();
    }

    fn free_part(&self, part: &BoxPart) {
        self.particles.push(BoxPart(part.0)).unwrap();
    }

    fn particle_sz(&self) -> usize {
        self.particle_sz
    }

    fn free(&self) {}
}

#[cfg(test)]
mod test;
