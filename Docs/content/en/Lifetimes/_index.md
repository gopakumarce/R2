---
weight: 1
type: docs
description: >

---

# Lifetimes

As you can see, the use of Rust's lifetime markers has not proliferated throughout the code base. This is because really there is only one structure that has a 'reference', and that is used in almost every single place in the system - that one structure is the Packet. The Packet has one or more Particle structures, and the Particle as a mutable 'raw' &[u8] slice that holds the real data in the packet.

The packets, particles and raw data in the system are all allocated one time during initialization of R2, and they are never deallocated. That is, the packets, particles and raw data are permanent - ie in Rust, they have 'static' lifetimes. So as we can see, the Particle has &'static mut raw[u8] - since it has a static lifetime, even though Packet/Particle is used throughout the system, theres no lifetime proliferation. So needless to say, if you add a non-static (ie temporary) reference inside the Packet/Particle structure, all hell will break loose and you will need a lot of lifetimes everywhere.

The other structure which is pervasively used everywhere is a graph node client - ie any structure that implements the Gclient structure. The graph nodes are created by control threads and 'sent' to forwarding threads, so any non-static reference inside the structure that implements Gclient trait also will mean a blow up of lifetime usages everywhere. Also the graph node is 'sent' after creation from control to forwarding threads embedded the R2Msg message structure - so the node having non-static references will mean that the R2Msg will need lifetimes too, and R2Msg is also widely used in the system, so that will be a snow ball effect of having more lifetimes.

In summary, be very very careful and wise and thoughtful while adding references inside data structures - Rust will force you to be thoughtful by complaining about lifetimes errors all over the place.
