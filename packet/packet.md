---
title: "packet"
weight: 1
type: docs
description: >

---

# Packet and Particle

The packet object consists of a struct Packet and a chain of struct Particle objects. There Particle objects hold the actual data - it corresponds to an mbuf / skb etc.. in the bsd / linux worlds. The packet holds just meta data like total length of the packet, the l2 and l3 header start in the data etc.. The particle object also holds minimal data like the head and tail of the actual data buffer etc.. The question about what is a good particle size etc.. are offloaded to the user of the library. Typically these days the popular particle size of choice is 2048. Which means that the standard ethernet mtu frames (1500) will fit in one particle, and a jumbo ethernet (9000) frame will need four particles chained together. As we can see, the Particle structure chains a set of particles together - and also all the Packet get_data() kind of APIs lets users get into various offsets into the packet with chaining hidden from them. Rest of the packet library is standard networking operations on a packet - push and pull data to/from the front of the packet, append/remove data to/from the tail of the packet, get offsets to data inside the packet, store the layer2 and layer3 data offsets etc.

## BoxPkt, BoxPart and raw data

The memory address that the actual packet data resides very often comes from 'special' memory areas like the HighMem in case of a DPDK packet pool. The Packet and Particle structures itself usually can come from anywhere - stack / heap wherever. Although coming from stack wont usually work if the Packet/Particle has to cross thread boundaries. Its not as often as having the raw data come from special memory, but many times the system designer would want the Packet/Particle structure itself to come from memory areas of their choice - for example if the packet has to cross process boundaries. So the BoxPkt and BoxPart just gives the system designer an option to have the Packet and Particle come from memory of their choice. So a BoxPkt and BoxPart is similar to a Box<Packet> and Box<Particle> - except that the Box<Foo> version always comes from the heap whereas BoxPkt and BoxPart can come from any memory area of choice. So all the clients / applications / graph nodes will deal with the BoxPkt and not the Packet directly. The deref and derefmut traits allow the BoxPkt to be accessed as its just a Packet.

## Pools

The packet, particle and particle data (particle.raw) are each assumed to come from some pre-allocated pool, and hence the BoxPkt and BoxPart structures which are basically storing an address to Packet and Particle respectively. The pool itself can be implemented by the user of this library in whichever way the user wants - the packet, particle and the raw data can come from the memory of choice of the pool designer, the PacketPool trait simply defines APIs to get and free packets/particles. How exactly its done is upto the designer. Also, the pools are expected to be per thread (which usually maps to per core).Pools adhere to the philosophy we follow in all places in R2 - that we 'create stuff' in one place (main / a control thread) and 'send' it to the forwarding threads. So the pool trait itself needs is marked as Send capable. Since pools are per thread, that means the packet buffers are also per thread - so the pool itself does not need to be lock free etc.. But as explained in the architecture section, an interface/driver pinned to one thread can of course send packets out of an interface pinned in another thread. So then how does the packet get returned back to the pool in the original thread ? Each thread has a queue to which other threads can return packets to. The Packet structure itself has information about the queue stored in it, when the packet goes out of scope, the drop() imlpementation for the packet enqueues the packet back to the queue. And each graph run() will take packets out of the queue and give it back to the pool (its an area to investigate where before trying to allocate a packet from the pool, we can try picking one out of the queue first). Today the queue is a bounded MPSC crossbeam ArrayQueue - there are suggestions from people to try and switch it out to an implementation of something like an LMAX disruptor queue.

### Default Heap Pool

And a very usable simple example of pool is also provided in the PktsHeap structure where packets, particles and raw data all comes from the heap. And they are all stored in a simple VecDequeue.
