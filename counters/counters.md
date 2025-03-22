---
title: "Counters"
weight: 1
type: docs
description: >

---

# Shared memory counters

Counters are allocated in shared memory, so that external utilities can display the counters without disturbing the R2 process. The shared memory itself is managed in unix/shm, and theres not a lot to describe there - its just standard unix shared mem. 

Also note that there is typically only one counter shared memory area for the entire R2 (all forwarding threads). This is because the counters are expected to be allocated and deallocated by the control threads - we never expect the forwarding thread to be dynamically allocating or deallocating counters. So the counter allocation/deallocation in control thread can be lock protected, and the control thread simply passes on the counter to the forwarding thread(s). And of course the counter itself is just a pointer to an 8 byte memory area, so modifying that doesnt need any atomics or locks - which automatically means that the counter cannot be shared across forwarding threads or else the contents will get garbled (we dont do any atomic ops on the counter). So typically every forwarding thread node will have its own copy of the counters - this is done when the control plan thread clones the node, a new copy of the counter is created inside clone() of each node.

The shared memory for counters is divided into three areas

## Shared memory areas

### Directory

The directory is just an array of entries that will point to the name of a counter, length of the name, the address of the actual counter, and number of counters. Note that we might contiguously  allocate more than one counter. So the address of the counter points to an array of one or more counters, and hence the length in there. The directory is mostly useful for external utilities to walk through all the counters, dump the counter name and the associated value(s).

### Name

As described before, this holds the name of each counter, there is maximum length to each name.

### Counters

This holds the actual counter values. Modules that uses counters will have addresses of these locations that hold values, and will write into these locations. External utilies that dump counters will read from these locations.

## Managing shm areas using 'Bins'

So the above three areas we mentioned (directory, names, counters) have pre-defined start and end addresses (ie they have a fixed size). And then we are allocating objects which are multiples of a fixed size from these areas.

For example, the directory entry is fixed in size - and we always allocate them one at a time. The name is either 32 or 64 bytes, ie its a multiple of 32. The counters themselves are either one 64 bit value or an array of upto 32 of such 64 bit values. So the general modus operandi here is that each of these areas have items that are of a "bin size" (sizeof directory, 32 bytes for name, 8 bytes for counter) and a "bin max" which is a multiple of bin size thats the max one object can ever be (1 for directory, 2 for names and 32 for counters).

So the bin module provides a rather simple way to allocate and free fixed size objects from an area, with possibility for more than one of these objects contiguous. For example referring to the area for counters, there are bins of size 1 through 32. The bin size 1 holds a  list of counters of just 8 bytes (64 bits. Bin size 2 holds a list of counters of size 16 bytes (2 contiguous 8 bytes) and so on uptill bin 32 that holds list counters of size contiguous 32*8 bytes. And if someone wants one counter of 8 bytes, they pop one from the bin 1 list. If someone wants a 32x8byte counter, they pop one from bin32. 

All bins start off as empty. And we keep track of an offset in the memory area from which we are free to allocate. So when someone wants to allocate from a bin thats empty, we allocate the object fresh from the memory area's free offset. Theres one catch though which is that we allocate in multiples of a "page size" - so if someone wants 32*8 byte object, we allocate page-size / 32x8 number of those objects and put them all in the list in bin32. When an object is freed, it just goes back to its bin - which means that there is no "compaction" and such complicated memory management done. Which means that if someone allocates a ton of 8byte counters and frees them all and then tries to allocate 32x8 byte counters, the latter allocation might fail since all the free ones are in bin1. So obviously the bin stuff is used just for counter allocations - counters are usually allocated in fixed patterns and we dont anticipate random malloc style counter allocations, so the simple bin mechanism should suffice

## Counter flavors

There are three types of counters. First being the simple 8 byte counter, which can be used for anything like counting number of packets or bytes or some kind of error etc.. The next is a packet & byte counter - for situations where we want to increment a packet count and also increment the byte count, so rather than have two seperate counters in seperate parts of memory, we just allow an allocation of contiguous 16 bytes of memory. The third is an array of 8byte counters. with max 32 elements in the array.

