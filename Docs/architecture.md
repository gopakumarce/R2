# Highlights

The below are aspects of R2 we want to spend time talking about, each of these will be explained in details in later sections.

## Source code

The source is organized as individual libraries, they are all in one repository at the moment, but can be easily pulled out to be seperate to "R2 infrastructure" and "R2 features". This will enable people who want to add R2 features (ie R2 "nodes") to work on a small relatively less changing library and add features of their own which can be loaded into the R2 repository. In the Architecture Details section, see the sub section on dynamic loading of nodes into R2.

## Forwarding Graph

Everyone who has worked on packet forwarding engines know that the packet always moves through a chain of functions, example below for basic IP routing of a packet

Packet Input-->Remove Ether Header-->Route Lookup-->Add Ether Header-->Packet Output

Based on whether the packet is an IPV4 packet or an IPV6 packet, the "Route Lookup" can further be split into an IPV4 Lookup or IPV6 Lookup

```rust
                 ------------>IPV4 Lkup--------+
                 |                             |
                 |                             V  
Input-->Remove Ether Header              Add Ether Header-->Output
                 |                             ^  
                 |                             |
                 ------------>IPV6 Lkup---------

```

Obviously there are a lot more "functions" that we have left out - packet scheduling being one of them. The scheduler usually comes into play before the Output happens. So we can see how the packet path is naturally a collection of functions organized as a graph. And that is how R2 organizes its packet path, as a collection of graph nodes where the nodes takes in a packet and gives out a packet doing whatever packet processing that node does. 

There are many advantages to the graph model - the obvious one being modularity. In a "spaghetti" architecture where there is no clean seperation of nodes, its not too long before ipv6 and ipv4 is cyclically dependent etc.. and the code quickly gets tangled beyond repair. In R2, the nodes are compiled as independent cargos (think library) that depend only on the core R2 library, so its architected not to get entangled. The other not so obvious advantages are that the nodes need not use the code provided with R2. If someone has a better implementation of IPV4 forwarding, when R2 starts up we can dynamically load the better version of IPV4 node and ignore the one provided by R2. R2 does not have code as of yet to do that, but the archicture allows it and thats the vision.

## Threading model

An R2 thread basically moves packets from one graph node to another. The packet comes in when the node for an interface reads a packet from the interface (via socket or via driver/ dpdk etc..), and that packet then runs through various nodes of the graph and reaches an output interface node where it gets sent out on the interface. The idea is that everything that is needed to be done for a packet happens within one thread. The user of R2 can run as many threads of the R2 as needed (hopefully no more than the number of cores). The highlight here is that each thread has an EXACT COPY of the graph, ie the graph nodes are NOT shared across threads. So if we take the graph of Picture 1 and run it in two threads, each thread will have an Input mode, Remove-Ether node, IPV4 node, IPV6 node, Add-Ether node, and Output node. Taking the IPV4 node as example, it does IPV4 route lookups. So obviously the nodes in both the threads need access to the same set of IPV4 route tables etc.. We will talk about how that is achieved in the next section.

The one question that can arise is about the interface Rx/Tx nodes themselves - even those nodes are duplicated on all threads. But at the same time, typically there can be no parallel access to hardware from different threads. So the way to deal with that is that only one of the threads will do the Rx/Tx (packet recieve/send) for an interface, the other threads Rx/Tx node will just be a NO-OP. The interface can decide which thread it wants to run on. And of course different interfaces can pin themselves to different threads.

But the problem is that even if only one thread handles Rx for interface, any thread would want to send packets out of any interface - routing that happens in any thread can end up choosing any interface to send the packet out of. There are two ways to handle that - the Tx node on each thread can take a lock and thus ensure that only one thread is doing Tx at any time. OR we  designate one thread for Tx for a particular interface, and everyone else just queues up the packets to that thread using a lock free MPSC queue. Again, its entirely upto the node to decide whether to lock or to handoff packets via MPSC queue to another thread, the preferred model is the latter. With multiple threads all fighting for a lock, the behaviour of the pipelines become unpredictable - the goal is to strive as much as possible to avoid pipelines locking each other out. The same consideration for the Tx driver also applies to the Tx packet scheduler - the packet scheduler is best done in one thread (whichever is associated with the Tx interface) that handles both enqueues to and dequeues from the scheduler.

## Data Structure model

Data can either be "owned" by a particular thread OR it can be "shared" across threads. If a thread owns the data, then only that thread can do any read/write of the data. If data is shared across threads, then the data is immutable (ie cannot be written into) without a lock etc..

Obviously there is no one solution that fits all cases. But the general approach R2 takes for forwarding objects, is to have the data shared between control and forwarding threads. The control thread is the designated "creator" of the data which then is shared with forwarding threads. Let us given an example. So the ipv4 routing table is an example of shared data. If there are 16 threads of R2 for example, it does not make sense to "own" the routes on each thread - that will bloat up memory consumption 16 times. So we share the data. Same applies to stuff like interface configuration data etc.., they are also shared. But like we mentioned above, if data is shared and say the control thread wants to modify the data. One option is for the shared data to be lock protected, the control thread can take a lock and modify the data. But then the forwarding threads will be stalled during that time. So each time a route is added or deleted, there will be a micro stall of the forwarding planes. It can work sure, but we don't want that approach - we talked about "predictable" architectures in the introduction page.

### Control thread wants to modify forwarding data 

So when control thread wants to modify shared forwarding data, it makes a copy of the shared data, modifies the copy, and then messages the forwarding threads with a reference to the copy. The forwarding threads on getting the message, will start pointing to the new copy. So for example, if the routes are stored in a tree structure, we keep two exact copies of the tree at all times. Note that the routes themselves are not copies, only the tree and the tree nodes etc.. are copies. Typically the route is the tree leaf, and the route holds the largest amount of data, the tree nodes themselves are small. So now if we get a route add, we add the route to the copy, and send a message to all forwarding threads asking them to start using the copy. And once all forwarding threads have switched over to the new copy, we add the route to the old table also - thus old and new are always kept in sync.

The above is the general mechanism adopted for other pieces of data also - like interface config information. If interface config changes (like ip address or mtu etc..), we create a new copy and ask forwarding threads to point to the new one. That is our general philosophy.  

## External communication via APIs

R2 is purely a forwarding plane. That is, it will not run stuff like routing best path computations etc.. inside it. Nor will R2 worry about figuring out what is the IP address of an interface etc.. R2 has to be "told" such things by whatever external entity can do that, ie R2 has to be "programmed" with that information. The way that is done is using Apache Thrift APIs to communicate to R2.

### Why Apache Thrift

We wanted the below from our API library of choice

1. It should be intuitive to use and have basic minimal features (ie no need of complex documentation)
2. It should be light weight - the library should be as small in size as possible
3. It should have libraries in a variety of languages
4. Performance is important, but not a *huge* concern - we are not going to make a million API calls in a second unlike a web server

And of course remember we are working with Rust, so we want a Rust library. Now admittedly, the most popular one out there is google protobuf. But after scanning through some google protobuf libraries in Rust (and its not really just rust, any language has the same issue) I quickly decided against protobuf. Google protobuf libraries are humongous and with stuff like HTTP servers built in ! Maybe they are needed in the web world, but we are talking about the embedded world here. I cant have R2 loaded with an HTTP server inside, no way!

So then the only other option really was Apache Thrift. It is very unlikely one can read and understand what goes on inside a Google protobuf library in a couple of hours, but the Apache Thrift Rust code is a thing of beauty - in a couple of hours I got a hang of the implementation and got to a stage where I could answer questions from the code, without needing any documentation. And its a really light weight slim library, and satisfies all the stated requirements. So far, I have not once regretted the choice of using Thrift for R2.

So all communication from external programs to R2 will be via thrift APIs. R2 obviously provides the Thrift api definition files which can be compiled by the Thrift compiler into any language of choice - Java or Go or python etc.. For each API, R2 also provides a "command line" implementation example of those APIs - where one can configure R2 using a command line program corresponding to those APIs. This can serve as a guide for anyone who wants to use another language to write an API to talk to R2

## Unit Testing

Rust offers very easy convenient testing capabilities - "cargo test" - which can do both unit testing of a single library in the code base, and integration testing using multiple libraries in the code base. R2 uses the unit test capabilities extensively and each module is expected to write unit test cases before it can be accepted. And each commit also needs to pass "cargo test" before it can be accepted.

# Details

## Graph and Graph Nodes

### Anatomy of the Graph

The forwarding graph is *identical* in every thread. Even if a particular graph node is not interested in running in one particular thread, the node will still be there in the graph. A graph node object is expected to provide three methods - clone(), dispatch() and control_msg(). The graph nodes identify themselves by a well defined name (defined in a central module). There is no restriction as to which node can point to which etc.., ie it is also possible to arrange the nodes to loop back the packet (like say if we want to remove an outer mpls label and start again with the inner mpls label).

#### clone()

clone() is crucial to how the graphs are constructed. The usual method is to create a graph object, and add nodes one by one to the graph object - the nodes can be like ipv4 forwarding, ethernet encap / decap etc.. And once the entire graph is constructed, its ready for use by one thread. And then we "clone" the entire graph - ie make a copy of the entire graph - and pass it onto another thread, we do that as many times as there are threads. And each time the graph is copied, the graph in turn calls clone() on each node in the graph. 

So the basic mechanism is to construct the graph one time, and then clone it as many times as needed and pass it onto the forwarding threads. The other approach would have been to do the graph construction from scratch in each thread. But the thought is that its more logical and cleaner and potentially faster to be doing the construction once and then keep cloning as many times as needed. This method of constructing the graph will cover 90% or more of the different types of nodes that will ever be in the graph. There will be a small percentage of nodes for which this wont work.

##### Node add message

The "interfaces" in any packet forwarding system are dynamic in nature. They can come in and go away at any time. So the interface nodes cannot really be created at bootup time. So the above mechanism of "create the entire graph and clone" wont work because the interfaces might not all exist at that point in time. An interface can come in after all R2 threads have already started running. As mentioned earlier, R2 is a pure forwarding plane and is not in the business of "detecting" interfaces. An external entity will make an API call to R2 to add an interface - in response to that API call, R2 will create one interface node and clone() that node and send it as a message to all R2 threads. And the R2 threads on recieving the message with the node in it, will add the node to their own graphs.

#### dispatch()

The dispatch() method provides node with a bunch of packets in a vector and the node processes those packets and enqueues them to the next node/nodes.

#### control_msg()

This is a method to handle messages from control plane. The control plane messages are relayed to the nodes that express interest in the message. This is an **optional** method. If a node does not define one, then it automatically gets a NO-OP method.

The graph object provides a run() method which will iterate through all the nodes in the graph ONE TIME, and call dispatch() on each of them. It is up to the thread controlling the graph to call run() multiple times, potentially interleaving it with other activities the thread wants to do. The run() will provide the caller an indication of whether the graph has more work queued up (packets queued up) and the earliest timestamp in nano seconds from now that the graph expects service.

### Anatomy of a graph Node (Gnode)

The graph node object should have these basic things

1. The methods we described in the previous section
2. A name of its own, defined in the "names" module
3. An array of names of the nodes this node points to (ie nodes this node has graph edges to)

When a graph node processes a vector of packets given to it through dispatch() method and decides to send a packet to the next node, it calls a method on the dispatch object with an index into array in item 3) above. And given an index into array 3), the dispatch object (passed in as parameter to the dispatch() call) will know the actual node that the packet has to be queued to.

Each graph node has associated with it a (fixed size) vector to store packets. Say we call dispatch() for node1, node1 is provided this vector of packets - even though the vector is fixed size say 256, it might have just one packet, or it might have all 256. And say node1 processes packet at index 0 and decides to send it to node2. So packet at index 0 will call a method in the dispatch object (parameter to dispatch()) asking for the packet to be queued to node2 - if node2's dispatch vector happens to be full, the packet will get dropped - which generally will be a good indication that the code/feature in node2 has performance issues. But of course eventually the slow node will start back pressuring all the other nodes in the system and we will start seeing drops everywhere. The general packet forwarding design principle is that we want all the packet drops to happen at the entry point into the graph, and want to avoid drops in intermediate nodes - so a smart node which can quickly classify packets and decide to drop early on based on the priority of the packet, the performance of the system etc.. is what we need. For example the L3Parse node in the code base might be an ideal place to do that.

How about a node that is not "provided" any input packets, but rather is supposed to "generate" input packets - ie an Rx driver node that reads packets off the wire (or a socket) ? R2 organizes the Rx and Tx driver nodes to be bundled into one single node - an IfNode. So when the IfNode is called with a dispatch(), the vector of packets are the ones to be sent out (Tx). And after Tx (or even before, whatever the driver wants), the same node can poll its Rx side to see if there are input packets and process the input packets and throw them into the same dispatch vector. So obviously the driver node will need a larger dispatch Vector to hold both Tx and Rx packets. Note that we end up making two passes over the dispatch vector - one inside node1 and once node1 dispatch() is complete, to send it to the next nodes (their dispatch vectors). We could have chosen to somehow expose every nodes dispatch vector to everyone else and have node1 directly enqueue to node2's dispatch vector, we intentionally avoided that to not have nodes poke into one another. This will be potentially an item to revisit once we seriously start looking at performance aspects.

Each node also returns an indication of whether it has more pending work to do and if so what is the earliest time in nano seconds from now that it needs service. Usually the only nodes that have pending work will be the non-work-conserving (ie shapers) scheduler nodes which can have packets scheduled to a later point in time.

### Dynamically loading/extending the graph

There is no code to do this today, but the goal is to achieve this. So R2 eventually will come with a pre-defined set of functionalities, with nodes defining each functionality. Say at some point in time, someone who runs R2 wants to swap out ipv4 routing node with their own better performing or some proprietary v4 node. And the way the client should be able to achieve that is to build their ipv4 node and place it into a node library, and when R2 is launched it will know that someone has an ipv4 node in the library and hence it will use the one from the library instead of its own. The same will be the case when the client wants to add a new node to the graph thats not provided by R2. This first of all needs a good Rust ABI support which seems lacking at the moment, and more design to achieve this. But this is certainly the way to go to enable usage of R2 in flexible ways. 

And this will be made even more easier by the fact that the R2 source is organized such that nodes can be built outside of the main R2 repository, so people can choose to keep their nodes as closed source if they choose.

### A day in the life of a packet

Ok, now we know enough to be dangerous. Let us look into the graph for doing simple IPv4 packet forwarding. Let us expand on the graph in Picture 1

```rust
   +----->EtherDecap------>L3Parse-------+
   |                                     |
   |                                     V
IfNode                                IPv4Fwd
   ^                                     |
   |                                     |
   |                                     |
   +------EtherEncap<-----EncapMux<------+
```

The above is a basic ipv4 forwarding graph. The picture assumes that packet comes in from one interface and goes out on the same interface, ie there is only one interface in this example. And thing to note is that the EtherEncap and EtherDecap nodes are "per interface". Lets say there is an ethernet interface and lets say a serial interface in the system. The way the layer2 encaps and decaps is done is different for ethernet and serial, so encaps decaps is per-interface. Which also means that if there are two ethernet interfaces, there will also be two EtherEncaps and two EtherDecaps. Also each interface has an index called an "ifindex" (standard unix terminology), which is just a number corresponding to the interface. The arrows clearly indicate how packets flow, we clarify them further below.

1. IfNode has the Rx driver that reads packets in (assume ethernet packets), and forwards to the Ethernet Decapsulate node. The packet also has the input interface index stored in the packet

2. Ether Decaps Node removes the L2 headers and sends it to the L3Parse module

3. The L3Parse module tries to figure what is the layer3 protocol, in this case it identifies it as IPv4 and sends packet to IPv4Fwd

4. The IPv4Fwd node does a route lookup and finds an output adjancency which has information about the output interface and next-hop IP etc.. - and that information we store in the packet and sends it to the Interface node.

5. The EncapMux node is kind of a demultiplexer. Its only job is to forward the packets to the Ethernet Encaps node corresponding to the right interface. This requires a bit more explanation. Why cant IPv4Fwd just forward the packet directly to EtherEncap node corresponding to that EncapMux ? As we discussed earlier, each node maintains a list of ALL its next-nodes. And also as we discussed earlier, the interfaces in the system can come in and go away dynamically. Now how will IPv4Fwd node know all the interfaces that will be present in the system ? We dont want to be updating the IPv4Fwd node (and possibly many other nodes wanting to get their packets to EtherEncap) whenever an interface comes in or go away. Hence the "EncapMux" node sits in between to hide the actual interface information. The EncapMux node uses output ifindex to figure out the proper EtherEncap node to send the packet to.

6. The EtherEncap node adds the ethernet headers and sends the packets for output to IfNode.

Now lets say we have two interfaces in the example  - say ifindex 0 and 1 - and packet coming in on index 0 and going out on index 1. So there will be IfNode0 sending packet to EtherDecap0 and IPv4Fwd does a route lookup and decides the packet has to go out on ifindex1, so stores the ifindex1 in the packet and sends to EncapMux node. And EncapMux sends it to EtherEncap1 and EtherEncap1 sends it to Ifnode1.

#### Interfaces in different threads

In the above example, it is  possible that ifindex0 and ifindex1 interfaces are handled by thread0 and thread1 respectively, for example. So in this case, the Ifnode0-->EtherDecap0-->IPv4Fwd-->EncapMux-->EtherEncap-->Ifnode1 will all run on thread0, but the ifinded1 is actually handled by thread1 - ie thread1 driver is the only one that can send packets out on ifindex1. So what is Ifnode1 doing in thread0 ? The ifnode1 on thread0 will simply queue packets on a lockfree queue, to Ifnode1 on thread1. Ifnode1 on thread1 will dequeue packets from the lockfree queue and use its drivers to actually send it out on the interface.

#### How does ARP work

So one thing we did not talk about above is how exactly does the EtherEncap node know what mac address to slap onto the packet. The EtherDecap node is the one that can "learn" mac addresses when it sees incoming packets. But then the EtherEncap node needs to learn it too. The EtherDecap node on learning a new Mac will inform the control plane via message channel, and control plane will broadcast the mac to all nodes and thus EtherEncap gets it too. R2 right now is designed to be a router and does not expect a ton of mac addresses. If it does at some point, the design around messaging and its frequency etc.. will have to be tuned to scale to a large number of mac addresses.

##### ARP request, response

```rust
   EtherDecap------>IfNode
```

EtherDecap node gets arp request, constructs an arp response packet and sends it to IfNode as shown above. In case of R2 generating ARP request, the packet reaches all the way to EtherEncap node which might find the arp entry missing. So EtherEncap constructs an ARP request and sends it to Ifnode.

## Threading, messaging and data models

### The control threads

The main() thread does all the graph creation, launches other forwarding threads etc.. and also launches the API handler thread which handles external requests to add interfaces, add/del routes etc.. Once all the creations are over, the main thread is just listening for messages from forwarding threads (like mac learning messages). It can potentially run some periodic tasks also in future. The main thread might want to modify R2's program context - for example add a new mac entry to some global display table etc.. the API handler thread might also want to modify R2's program context obviously because it might want to add / del interfaces and routes etc. So when we refer to "control threads" anywhere in R2, it refers to either the main() thread or the API handler thread - and remember both of them would want to modify the R2 program context.

### Clone Vs Copy

This is more of a Rust terminology - copy means something that can be just mem-copied - ie just copy the bits as is and you get a new copy of the original object. Whereas clone is something you cant just replicate by copying bits - for example if the original data had a reference counted object, we cant just bit-copy that, we need to increment the reference count before copying it. So for cloning, we need some function which will intelligently replicate element by element from the initial object and provide us with a replica of the original one. 99% of the times when we say "copy" anywhere in this document, it actually is a replica made not by bit-copying but by cloning.

### Data Models

Each forwarding thread needs a bunch of data, including the graph itself. Who "creates" the data and who all "needs" the data determines a lot about the properties of the data object. So the multiple varieties possible are 

1. Each thread creates the data they need, on their own
2. One thread creates the data, uses it for a while and passes "ownership" of the data to another thread
3. One thread creates the data, and makes "copies" of the data and passes that to other threads
4. One thread creates the data, and shares that with other threads via reference counts
5. One thread creates the data, and shares that via &references

The above sound like a lot of categories, in fact its not. Its just a nice way of slotting data into sensible categories. In C, the language does not really care, and it is up to a good systems designer / programmer to think about his/her data in such detail and be "aware" what category each of his/her data falls into. And without enough thought (which is plenty of times), many objects are unsure what category they fall into, often they fall into multiple categories. But in Rust, the languages forces us to think in what category we want the data to be in and why so.

As mentioned in the Architecture Highlights section, for forwarding plane objects (like route tables and interface configs etc..), the goal is to use option 4), where the "creator" thread is the control thread. But for other pieces of data not directly related to forwarding (like counters and logs), we are not advocating any strict "one model fits all", the model that fits the situation best should be chosen - *after* ensuring that we think through the above categories and pick what fits best with full awareness of the pros and cons.

Below we look at the different data models and mention the R2 objects that falls into that category. As we can see, the models we use fall into three of the five possible categories above.

#### Copied/Cloned Data

The R2 objects in the "copied/cloned" category are

1. Graph - created in main(), and cloned to every forwarding thread. One can ask why not "share" the graph instead of cloning it ? The answer is that graph holds thread local data like the packet dispatch vector per node, which is obviously specific to each thread.

2. Graph nodes (Gnodes) - created in main() or API thread, and cloned to every forwarding thread

3. The message object R2Msg - when main() or API thread wants to broadcast a message to all forwarding threads, it creates one R2Msg object and clones it and sends it to all forwarding threads.

#### Shared Data (Reference counted)

Note that reference counted data in Rust is a simple Rc<Object> or Arc<Object>, and unlike in C, the use does not have to deal with the headaches of tracking reference counts and dropping the data etc.. Frivolous use of ref counting can of course lead to cycling refcounts and data leaks. The R2 objects in this category are

1. The R2 object - the R2 object holds the entire program context. Its shared between the control threads only. Since there can be more than one control thread which wants to modify the program context (add routes, add mac addresses etc..), its shared between control threads with a mutex lock protecting it.

2. Counter object - this handles allocation of individual counters from shared memory. The individual counters are allocated during node creation by the control threads, the forwarding threads dont need the counter object, they just need the individual counters themselves. So the counter object is shared between the control threads only.

3. The route table IPv4Table - this is created in main(), and shared between control and forwarding threads. One can ask why share, why not copy this ? The answer is that there is really nothing thread-specific in a route table, its exactly same across all threads.

4. The per thread logs - created in main(), and shared between the control and forwarding threads. If someone makes an API call to dump all the logs, the API handler can use the shared object and dump the logs from each forwarding thread. Instead if the logs were created and owned by each thread, a log dum would have to be done my messaging the forwarding threads and asking to dump the logs themselves!

5. The packet pool - created in main(), and shared between all forwarding threads(). Control thread can also potentially want to peek into it to display number of free packets etc..

##### Mutating (modifying) shared data - "interior mutability"

In the above list, the log and the packet pool are cases where the data is shared between two or more threads, and one or more thread wants to mutate the data. Now that violates the principle of "sharing". How can one thread have a handle to the shared data and expect the other thread to be able to modify it under its feet ? C will let one do whatever he/she wants. And people do whatever they want and run into bugs - one of the MOST COMMON class of bugs in a system written in C. Here again Rust comes to the rescue. The language will prevent from mutating shared data. But how do we go about it then ? We NEED to modify the data in this case. The forwarding thread that has the per-thread-log needs to write into the log, and maybe the control thread needs to read it to dump it. 

The forwarding threads all want to allocate packets from the packet pool! Welcome to the terminology of "interior mutability" in Rust. It's just a fancy term to say that "ok, this data is shared, but give me some way to modify it". And again, this is what a careful C programmer would do - he/she would hold a lock before modifying the data. And thats exactly what Rust allows us to do too. We can have an Arc<Mutex<Logger>> - which says its a "reference counted mutex protected logger". So if the fowarding thread wants to write into it, it holds the lock. If the control thread wants to dump the log, it holds the lock. Same can be done for the packet pool also - we can have an Arc<Mutex<PktPool>>. Simple concept - nothing different from C.

The above is perfectly fine, nothing wrong about it. Again as C system programmers are familiar, often times we opt for "lock free" algorithms to prevent threads waiting on each other for a lock and getting scheduled out and taking the hit of being rescheduled in etc.. And network programmers are very familiar with lockfree packet pools, especially those who have used the open source DPDK package. So the two options for keeping the packet pool data structure "sane" in a multi threaded environment is either use locks before accessing the pool or use lock free algorithms. And thats exactly what Rust allows us as the second option for "interior mutability". We can still have an Arc<PktPool> without the mutex - as long as we convince rust that the fields inside PktPool are all accessed and modified with Atomic operations. So rust will allow modifying Arc<PktPool> as long as PktPool fields being modified are done using Atomic Ops.

Now, just because fields are modified via Atomic Ops does not mean that eventually the data structure will be sane - a bunch of atomic ops does not constitute a sane lock free algorithm. But thats as much as a language can guarantee, it cant verify the sanity of a lock free algorithm (that would have been nice ;-), often no one including the author or reviewers can verify the sanity of lock free algorithms!). So in our case we intend to have the PktPool be designed using lock free algorithms. Another issue with lock free algorithms is the number of atomic operations they end up doing. An Atomic operation depending on the memory ordering can completely trash cache lines across CPU cores and thus affect performance. A Mutex also has an atomic op, but it has the added disadvantage that if the lock is locked, it can get scheduled out and scheduled back in by the OS - which is very expensive operation. Now if we design a lock free algorithm which has like half a dozen strongly ordered atomic ops, that wont perform well either. So Atomic Op is not a free lunch.

Coming to logger - logger is the case of a single writer (one forwarding thread that owns that log) and single reader (control thread) problem, and again we don't want to take locks for logging data from forwarding thread - when 99% of the time there is no lock contention, except for the time when the control thread wants to dump the log data. So even for the Logger structure, we modify the fields that need to be modified, using Atomic Ops and that keeps Rust happy. For the logger, keeping in mind that it's a single reader single writer problem, we do atomic operations using relaxed memory ordering - which on Intel CPUs is as good as a non atomic operation. So there is zero impact of having atomic op in the logger.

#### Owned Data (created in each thread)

There are some cases where it really is no logic for that data to be outside the thread - like the control plane thread doesn't give a damn about whether it can access that data or not.

1. Epoll object - with socket based drivers, we use epoll to figure out when we have packets and hence when to run the graph. The epoll object is allocated locally by each thread, no one else wants to be able to read/modify epoll data of another thread

2. The mac address table in layer2/eth/encap and layer2/eth/decap nodes - the mac adress information for an interface is local to the thread that the IfNode is pinned to. If the control thread wants a copy for display or other purposes, it can keep a copy of the mac address table from each node, it can be easily achieved by snooping into mac address broadcast messages - see section "How does ARP work"

### Messaging: Control plane to Forwarding plane 

Two examples where we want to send a message from control plane to forwarding plane - one instructing all forwarding planes to "use a new routing table" and another instructing to "use a new interface config data". These message handlers in the forwarding plane are handled inline with packets, and hence have to be short and quick - for example in these cases they just swap the references to objects.

### Messaging: Forwarding plane to Control plane 

It can happen that the packet input happens on one thread whereas output happens in another. Let's say an interface decapsulated an ethernet packet in thread0 and it is going to get encapsulated and sent out in thread1. So the decap node learnt a mac address which the encap node is interested in learning too. In this case the thread0 decap node will send a message to the control thread saying "I learnt a new mac" and the control thread will relay it to the encap node on thread1 which will add the mac address to its mac table.  

From a node's perspective this is not any different from getting a control to forwarding plane message. Except that its originated by another forwarding node and relayed by the control thread (as opposed to originated by control thread itself). The forwarding to control plane messages above are expected to be far fewer in number compared to the control to forwarding plane messages. The former is driven by a packet coming into the system and latter is driven by external API calls.

## Forwarding objects

The packet path can be looked at as a collection of nodes - which is what we discussed so far - OR it can be looked at as a collection of "forwarding" objects. For example in our "life of a packet" example, we can look at the packet path as having come from an "interface object", going into an "ethernet object" into an "ipv4 object" etc.. The "collection of nodes" view is basically a "collection of functions" view of the packet path. The "collection of objects" is a more data oriented view of the same thing. So one might think that there is probably a 1:1 correspondence between the two - which is not necessarily the case. 

For example, inside the "ipv4 node", we do a route lookup on the ipv4 packet, we find an "adjacency object" and send it to the EtherEncap node which adds an ethernet header using the information in adjancency object. But it can also happen for example that the route lookup gives us not one but two or more adjacencies - ie we have an option to send the packet out of multiple interfaces. In routing parlance thats called a "load balance object". Now does that mean we send the packet to a "load balance node" ? Not necessarily - the ipv4 node itself can make a decision to select one of the adjancencies from the loadbalance object and send it to the EtherEncap node. That is, the ipv4 node (and similarly each node) can end up "traversing" multiple forwarding objects before it sends the packet to the next node.

Can we just have one view and say that each forwarding object is a node - isnt that simple ? Well sure, that can certainly be achieved. But the problem is that transferring the packet from one node to the other is not cheap - it involves moving the packet from one nodes dispatch vector to the next node's dispatch vector, as we saw earlier. So if we have a large number of nodes, we have a large number of such transfers and that will slow the performance, and it will also make the graph node scheduling more complicated. So the nodes and objects organizaton is somewhat of a tradeoff - a node can be thought of as a collection of one or more objects, sort of an additional hierarchy in forwarding objects.

## Apache Thrift

There is a single thread launched by main(), that listens to API requests. As of writing this, the API requests are primarily for configuring interfaces, adding/deleting routes, adding/deleting scheduler queues etc.. But eventually it will expand to include collecting statistics etc.. At this point, I don't expect scaling of the API requests to be a problem - so a thread should suffice.

The APIs are all in the apis/ cargo module, today its part of the entire R2 source code, but it can be easily separated out to an independent library which can be used by third party modules to build their API calls, using their language of choice. R2 provides sample usages for each of the APIs in the utils/ directory - there are utilities using the APIs for all the configuration tasks and displaying / showing various things (like the logs). The APIs are compiled OUTSIDE the R2 cargo build system (into Rust) and dumped into the R2 code base as of today - we have to think of a way to integrate that with R2 build and generate the Rust bindings and probably other language bindings too.

On the topic of configuration, the configuration is a collection of independent unix utilities. There is no Cisco/Juniper like integrated CLI modules with R2. The Cisco/Juniper parsers are beasts by themselves and the whole config/display activities are tightly integrated into whole parsing scheme, I doubt if R2 will ever go that route. Some other open source projects like Cumulus linux has taken a different approach where they take the standalone linux utilities and provide a light weight skin on top that gives it a look and feel of an integrated Cisco/Juniper like parsing, that probably is a more attractive option for future.

## Code formatting

Code format is without exception a topic on which no two people will agree. And hence the decision R2 has taken is to use the auto formatter "cargo fmt" (ie rustfmt). It's just that simple - write code without worrying about tabs and spaces and newlines and what not, and just run "cargo fmt" before you commit. I use Visual Studio with the rust RLS extension, which automatically does the formatting each time I save the file.

