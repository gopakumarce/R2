use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use counters::flavors::{Counter, CounterType};
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use fwd::ZERO_IP;
use std::alloc::alloc;
use std::alloc::Layout;
use std::cmp::min;
use std::collections::VecDeque;
use std::mem;
use std::net::Ipv4Addr;
use std::ops::{Deref, DerefMut};
use std::slice::from_raw_parts_mut;
use std::sync::Arc;

// A BoxPart basically is a pointer to a Particle structure, this allows particles
// to come from pools, where the pool implementor has freedom to decide what memory
// is used for the particle raw data and even the Particle structure itself.
pub struct BoxPart<'p>(*mut Particle<'p>);

impl<'p> BoxPart<'p> {
    pub fn size() -> usize {
        mem::size_of::<Particle>()
    }

    pub fn align() -> usize {
        mem::align_of::<Particle>()
    }

    /// # Safety
    /// This function takes raw pointers and converts it into a Particle, the part pointer
    /// should have space enough to hold the Particle structure. The raw pointer should
    /// have a length of rlen bytes
    pub unsafe fn new(part: *mut u8, raw: *mut u8, rlen: usize) -> Self {
        #[allow(clippy::cast_ptr_alignment)]
        let part = part as *mut Particle;
        *part = Particle {
            raw: from_raw_parts_mut(raw, rlen),
            head: 0,
            tail: 0,
            next: None,
        };
        BoxPart(part)
    }

    // reinit is called on a particle thats was used before and given back to the
    // particle pool and now being allocated again from the pool
    pub fn reinit(&mut self, headroom: usize) {
        assert!(headroom <= self.raw.len());
        self.head = headroom;
        self.tail = headroom;
        self.next = None;
    }
}

impl<'p> Drop for BoxPart<'p> {
    // A particle is never expected to go out of scope without
    // being attached to a packet. The packet drop() will free
    // the particle(s), so nothing to be done here
    fn drop(&mut self) {}
}

// Deref mechanisms to allow accessing a BoxPart as a Particle
impl<'p> Deref for BoxPart<'p> {
    type Target = Particle<'p>;

    fn deref(&self) -> &Particle<'p> {
        unsafe { &*self.0 }
    }
}

impl<'p> DerefMut for BoxPart<'p> {
    fn deref_mut(&mut self) -> &mut Particle<'p> {
        unsafe { &mut *self.0 }
    }
}

/// The clients will all deal with BoxPkt structure - its nothing but a pointer
/// to the Packet structure. The Packet structure memory can come from anywhere
/// that the packet pool implementor choses, but of course the memory has to be
/// valid across all threads in R2 because we can send packets from one thread
/// to another.
pub struct BoxPkt<'p> {
    pkt: ManuallyDrop<*mut Packet<'p>>,
    /// Once the packet is freed, its queued back here
    queue: ManuallyDrop<Arc<ArrayQueue<BoxPkt<'p>>>>,
}

impl<'p> BoxPkt<'p> {
    pub fn size() -> usize {
        mem::size_of::<Packet>()
    }

    pub fn align() -> usize {
        mem::align_of::<Packet>()
    }

    /// # Safety
    /// This function takes raw pointers and converts it into a Packet, the raw pointer
    /// should have space enough to hold the Packet structure
    pub unsafe fn new(
        raw: *mut u8,
        particle: BoxPart<'p>,
        queue: Arc<ArrayQueue<BoxPkt<'p>>>,
    ) -> Self {
        #[allow(clippy::cast_ptr_alignment)]
        let pkt = raw as *mut Packet;
        *pkt = Packet {
            particle: ManuallyDrop::new(particle),
            length: 0,
            l2: 0,
            l2_len: 0,
            l3: 0,
            l3_len: 0,
            in_ifindex: 0,
            out_ifindex: 0,
            out_l3addr: ZERO_IP,
            _ph: PhantomData,
        };
        BoxPkt {
            pkt: ManuallyDrop::new(pkt),
            queue: ManuallyDrop::new(queue),
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
}

/// By default because BoxPkt is a pointer to a Packet, it wont be Send because
/// Rust will not allow pointers/addresses to be send across threads. We override
/// it here because we have the _guarantee_ that these addresses are valid across all
/// threads in R2
unsafe impl<'p> Send for BoxPkt<'p> {}

// Deref mechanisms to allow accessing a BoxPkt as Packet
impl<'p> Deref for BoxPkt<'p> {
    type Target = Packet<'p>;

    fn deref(&self) -> &Packet<'p> {
        unsafe { &**self.pkt }
    }
}

impl<'p> DerefMut for BoxPkt<'p> {
    fn deref_mut(&mut self) -> &mut Packet<'p> {
        unsafe { &mut **self.pkt }
    }
}

impl<'p> Drop for BoxPkt<'p> {
    fn drop(&mut self) {
        // The packet goes back to the pool after this, do not touch
        // it anymore
        unsafe {
            let pkt = ManuallyDrop::take(&mut self.pkt);
            let queue = ManuallyDrop::take(&mut self.queue);
            self.queue
                .push(BoxPkt {
                    pkt: ManuallyDrop::new(pkt),
                    queue: ManuallyDrop::new(queue),
                })
                .unwrap();
        }
    }
}

/// External clients are free to implement their own versions of a packet pool, the pool
/// should provide the below methods. And all the addresses/memory in the pool should be
/// valid across all R2 threads. Each thread has a pool of their own. But the pools are
/// all created by the control thread and then passed over to the forwarding threads,
/// hence the reason we need the Send trait
pub trait PacketPool<'p>: Send {
    /// Allocate a packet with one particle. Expect allocation failures - hence the Option return
    fn pkt(&mut self, headroom: usize) -> Option<BoxPkt<'p>>;
    /// Allocate a particle (with the raw data), again expect allocation failure
    fn particle(&mut self, headroom: usize) -> Option<BoxPart<'p>>;
    /// Free a packet which has a single particle with it
    fn free_pkt(&mut self, pkt: BoxPkt<'p>);
    /// Free a particle
    fn free_part(&mut self, part: BoxPart<'p>);
    /// Return the fixed max-size of the particle's raw data buffer
    fn particle_sz(&self) -> usize;

    /// When the packet is freed, except the first particle, give all the other particles
    /// back to the particle pool. And then give the packet (with the first particle intact)
    /// also back to the pool. In other words an alloc from a packet pool is more optimized
    /// for the case of a 'single particle packet'
    fn free(&mut self, mut pkt: BoxPkt<'p>) {
        let mut part = pkt.particle.next.take();
        while let Some(mut p) = part {
            let next = p.next.take();
            // The particle goes back to the pool after this, do not touch
            // it anymore
            let p = unsafe { ManuallyDrop::take(&mut p) };
            self.free_part(p);
            part = next;
        }
        // The packet goes back to the pool after this, do not touch
        // it anymore
        self.free_pkt(pkt);
    }
}

/// Here we provide a default packet pool implementation, where the Packet, Particle and
/// the particle's raw data buffer all comes from the heap.
pub struct PktsHeap<'p> {
    alloc_fail: Counter,
    pkts: VecDeque<BoxPkt<'p>>,
    particles: VecDeque<BoxPart<'p>>,
    particle_sz: usize,
}

unsafe impl<'p> Send for PktsHeap<'p> {}

/// A from-heap packet/particle pool, the pool is created with a specification of the
/// number of packets, number of particles and max-size of each particle
impl<'p> PktsHeap<'p> {
    const PARTICLE_ALIGN: usize = 16;

    /// #Safety
    /// This API deals with constructing packets and particles starting from raw pointers,
    /// hence this is marked unsafe
    pub fn new(
        queue: Arc<ArrayQueue<BoxPkt<'p>>>,
        counters: &mut Counters,
        num_pkts: usize,
        num_parts: usize,
        particle_sz: usize,
    ) -> Self {
        assert!(num_parts >= num_pkts);
        let parts_left = num_parts - num_pkts;
        let particles = VecDeque::with_capacity(parts_left);
        let pkts = VecDeque::with_capacity(num_pkts);
        let alloc_fail = Counter::new(counters, "PKTS_HEAP", CounterType::Error, "PktAllocFail");
        let mut pool = PktsHeap {
            alloc_fail,
            pkts,
            particles,
            particle_sz,
        };

        unsafe {
            for _ in 0..num_pkts {
                let lraw = Layout::from_size_align(particle_sz, Self::PARTICLE_ALIGN).unwrap();
                let raw: *mut u8 = alloc(lraw);
                let lpart = Layout::from_size_align(BoxPart::size(), BoxPart::align()).unwrap();
                let part: *mut u8 = alloc(lpart);
                let lpkt = Layout::from_size_align(BoxPkt::size(), BoxPkt::align()).unwrap();
                let pkt: *mut u8 = alloc(lpkt);
                pool.pkts.push_front(BoxPkt::new(
                    pkt,
                    BoxPart::new(part, raw, particle_sz),
                    queue.clone(),
                ));
            }

            for _ in 0..parts_left {
                let lraw = Layout::from_size_align(particle_sz, Self::PARTICLE_ALIGN).unwrap();
                let raw: *mut u8 = alloc(lraw);
                let lpart = Layout::from_size_align(BoxPart::size(), BoxPart::align()).unwrap();
                let part: *mut u8 = alloc(lpart);
                pool.particles
                    .push_front(BoxPart::new(part, raw, particle_sz));
            }
        }
        pool
    }
}

impl<'p> PacketPool<'p> for PktsHeap<'p> {
    fn pkt(&mut self, headroom: usize) -> Option<BoxPkt<'p>> {
        if let Some(mut pkt) = self.pkts.pop_front() {
            pkt.reinit(headroom);
            Some(pkt)
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn particle(&mut self, headroom: usize) -> Option<BoxPart<'p>> {
        if let Some(mut part) = self.particles.pop_front() {
            part.reinit(headroom);
            Some(part)
        } else {
            self.alloc_fail.incr();
            None
        }
    }

    fn free_pkt(&mut self, pkt: BoxPkt<'p>) {
        self.pkts.push_front(pkt);
    }

    fn free_part(&mut self, part: BoxPart<'p>) {
        self.particles.push_front(part);
    }

    fn particle_sz(&self) -> usize {
        self.particle_sz
    }
}

// A packet is composed of a chain of particles. The particle has some meta data
// and a 'raw' buffer, which is what holds the actual data. Every
// particle in a packet will have the same fixed size 'raw' buffers. Though we
// dont mandate it, usually all particles in the entire system will have the same
// fixed raw buffer size.
pub struct Particle<'p> {
    raw: &'p mut [u8],
    head: usize,
    tail: usize,
    next: Option<ManuallyDrop<BoxPart<'p>>>,
}

impl<'p> Particle<'p> {
    fn len(&self) -> usize {
        self.tail - self.head
    }

    fn data(&self, offset: usize) -> Option<(&[u8], usize)> {
        if offset >= self.len() {
            return None;
        }
        Some((
            &self.raw[self.head + offset..self.tail],
            self.len() - offset,
        ))
    }

    fn data_raw(&self, offset: usize) -> &[u8] {
        if offset >= self.raw.len() {
            &[]
        } else {
            &self.raw[offset..]
        }
    }

    fn data_raw_mut(&mut self, offset: usize) -> &mut [u8] {
        if offset >= self.raw.len() {
            &mut []
        } else {
            &mut self.raw[offset..]
        }
    }

    // Add data behind the 'head', there might not be room for all the data, add
    // as much as possible, return how much was added
    fn prepend(&mut self, data: &[u8]) -> usize {
        let dlen = data.len();
        if dlen > self.head {
            let count = self.head;
            self.raw[0..count].clone_from_slice(&data[dlen - count..dlen]);
            self.head = 0;
            count
        } else {
            let count = dlen;
            self.raw[self.head - count..self.head].clone_from_slice(&data[0..count]);
            self.head -= count;
            count
        }
    }

    // Add data after the 'tail', there might not be room for all the data, add
    // as much as possible, return how much was added
    fn append(&mut self, data: &[u8]) -> usize {
        let len = min(self.raw.len() - self.tail, data.len());
        self.raw[self.tail..self.tail + len].clone_from_slice(&data[0..len]);
        self.tail += len;
        len
    }

    fn move_tail(&mut self, mv: isize) -> isize {
        let len = self.raw.len() as isize;
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

    fn last_particle(&mut self) -> &mut Particle<'p> {
        let mut p = self;
        while p.next.is_some() {
            p = p.next.as_deref_mut().unwrap();
        }
        p
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
pub struct Packet<'p> {
    // The first particle
    particle: ManuallyDrop<BoxPart<'p>>,
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
    /// The ifindex of the interface on which this packet came in
    pub in_ifindex: usize,
    /// The ifindex of the interface on which the packet will go out of
    pub out_ifindex: usize,
    /// The next-hop IPv4 address out of out_ifindex, to use for ARP
    pub out_l3addr: Ipv4Addr,
    // Just to shut the compiler from complaining 'p is not used
    _ph: PhantomData<&'p ()>,
}

#[allow(clippy::len_without_is_empty)]
impl<'p> Packet<'p> {
    fn push_particle(&mut self, next: BoxPart<'p>) {
        let p = self.particle.last_particle();
        p.next = Some(ManuallyDrop::new(next));
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn headroom(&self) -> usize {
        self.particle.head
    }

    pub fn prepend(&mut self, pool: &mut dyn PacketPool<'p>, bytes: &[u8]) -> bool {
        let mut l = bytes.len();
        while l != 0 {
            let n = self.particle.prepend(&bytes[0..l]);
            if n != l {
                let p = pool.particle(pool.particle_sz());
                if p.is_none() {
                    return false;
                }
                let p = ManuallyDrop::new(p.unwrap());
                let prev = mem::replace(&mut self.particle, p);
                self.particle.next = Some(prev);
            }
            l -= n;
        }
        self.length += bytes.len();
        true
    }

    pub fn append(&mut self, pool: &mut dyn PacketPool<'p>, bytes: &[u8]) -> bool {
        let mut offset = 0;
        while offset != bytes.len() {
            let p = self.particle.last_particle();
            let n = p.append(&bytes[offset..]);
            offset += n;
            if n == 0 {
                let p = pool.particle(0);
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
    pub fn push_l2(&mut self, pool: &mut dyn PacketPool<'p>, bytes: &[u8]) -> bool {
        if !self.prepend(pool, bytes) {
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
    pub fn push_l3(&mut self, pool: &mut dyn PacketPool<'p>, bytes: &[u8]) -> bool {
        if !self.prepend(pool, bytes) {
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
#[cfg(test)]
mod test;
