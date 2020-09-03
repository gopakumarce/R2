# DPDK interacting with R2

What R2 wants to leverage from DPDK is the wealth of device drivers it supports. It gives us a quick start and as and when R2 gets mature and has drivers of its own,
the DPDK support can be phased out. We do not intend to use any other packet forwarding functionalities in DPDK. And including DPDK violates the "safety" aspect of
Rust big time - all FFI is unsafe code! So use of DPDK should be a transit path to get us somewhere and then we replace the dependency on it (which is drivers)

## Configuration

In the r2 config file (see Docs/r2_configs.md), add a section called dpdk as below. The on=true means R2 is running with dpdk enabled, it can be set to false and then rest of the dpdk configs dont matter because dpdk is turned off. The mem=128 says dpdk uses 128Mb for mbuf pool. The ncores=3 says that core0 is used as the main core (non data plane) and core1 and core2 are the data plane cores. core0 is used as the main core always as of today

```
[dpdk]
on=true
mem=128
ncores=3
```

## Packet pools

R2 has the PacketPool trait in packet cargo, which is implemented for DPDK also. DPDK has the concept of mbufs with its own mbuf header with its own l3/l2 fields etc.,
we dont plan to use anything in the mbuf header other than being able to specify the mbuf packet/data length in the mbuf pkt_len and data_len fields. Also we support
only a single mbuf packet (as of today), even though R2 itself supports chained particles. So the mapping is as follows

pkt - comes from heap
particle - comes from heap
particle.raw - this is the mbuf

The dpdk mbuf structure is like this - [[struct rte_mbuf][headroom][data area]]. The headroom + data-area is the size we specify to dpdk when we create an mbuf pool.

When a packet is freed, it just goes back to the pool's packet free queue. For a particle, we dont maintain a free queue, instead we let the freed particle go back
into the dpdk mbuf pool (we have to give it back to the mbuf pool or else dpdk driver wont find an mbuf to give us a packet). And when we need a particle, we allocate
an mbuf from dpdk mbuf pool, but then how do we get the heap-particle from the mbuf ? We do that by stealing two pointers from the headroom. So the actual layout
of the mbuf that we use is as below

[[struct rte_mbuf][mbuf-ptr heap-ptr remaining-headroom][data area]]

So the mbuf buf_addr starts right after the rte_mbuf structure, in our case poiting to the area we use to store the mbuf pointer itself. And the next word we use
to store the heap-particle pointer. Each mbuf is allocated its own heap-particle when mbuf pool is initialized. So when mbuf is allocated, we can get the BoxPart
structure also using the heap-ptr address. So we eat into the available headroom a bit. So this allows us to get from mbuf to BoxPart

The mbuf pointer itself is stored to get from BoxPart to mbuf .. So if BoxPart is freed, we know what mbuf needs to be freed to the dpdk mbuf pool. Obviously, all 
this is hugely unsafe pointer math.

## Driver Rx/Tx

DPDK initializes each port and assigns it a port number. Each dpdk port is a structure that implements the Driver trait in graph cargo. The Driver trait expects
and send and receive function, which is implemented using DPDK's rx-burst and tx-burst APIs (with burst size 1 as of today). And the drivers/ports are themselves
just a part of the IfNode graph node.

## DPDK EAL Thread

DPDK does the actual work of reading packets, processing them and sending them out in "EAL Threads". And for R2, an EAL thread is nothing but a thread that processes
the graph. Unfortunately the EAL thread runs inside the dpdk FFI code, so we have to dress up the Rust graph processing routing with unsafes to make it palatable
to FFI.

Other than the above mentioned items, the rest of the architecture continues to be the same - DPDK or not, we have features in graph nodes, we have a graph
processor thread, we have driver nodes. And dpdk sits in the driver nodes, and the grap processor is run as a DPDK EAL thread, that is about it.
