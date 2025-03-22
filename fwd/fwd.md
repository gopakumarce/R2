---
title: "Forwarding"
weight: 1
type: docs
description: >

---

# Forwarding objects

We discussed in brief in the architecture details section about forwarding objects, consult that document for more details. Any packet goes through a chain of objects. For example a packet starts from an Interface object, it then gets rid of the layer2 encap and assuming its an IPv4 packet, it will try to find an ipv4 table object where it looks up to find an ipv4 leaf object which will provide an adjacency object that tells the packet which Interface object the packet has to go out of and what layer2 encaps to apply on the packet before it goes out of the interface.

So this module captures all these object parameters - what is capture here is really highly "independent" data - other than the basic types and the objects in this module, this module is not supposed to depend on anything like device drivers for example (that will be utter blasphemy!). This module is the top/first tier in the module heirarchy (refer Docs/modules.md).
