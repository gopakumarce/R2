# Main

The code in main/ is logically organized into files based on their functionality

main.rs: deals with creating and initializing fundamental structures, like packet pools, counters, the graph itself.

ifd.rs: deals with interface management - adding new interfaces, adding/modifying ip addresses etc..

ipv4.rs: Deals with ipv4 routing, adding/deleting routes etc..

log.rs: Dealing with log display etc..

msgs.rs: Deals with  forwarding<-->control plane messaging

pkts.rs: Deals with packet pools, today this is empty, we just use the default in-heap pool provided by packet/ library.

## main.rs

The struct R2 holds the context of the entire program - it has data in it that used by the control threads, and some data thats shared across control and forwarding threads. It also has data that is unique to individual forwarding threads (logger being an example). main creates all these contexts like counters, packet pools and message channel to exchange messages between control and forwarding planes. The broadcast() method in R2 broadcasts the message to all the forwarding threads. broadcast() expects each message to implement a clone() - it works by sending a copy of the message to each thread.

Main proceeds to create all the graph nodes other than the interface related ones (EtherEncap, EtherDecap etc..) in create_nodes(). The interface related nodes as explained in the architecture, are "pluggable" - an external entity has to message R2 to add an interface. Main also creates an epoller - whenever the interface nodes are created, if the interfaces work using sockets, we need to wait till packets arrive on the socket. We register the socket file descriptor with the epoller.

The register_apis() ends up registering the callbacks of all the modules that have APIs that can be invoked from an external entity. And finally create_thread() launches as many forwarding threads as required, cloning the  graph for each thread. The forwarding threads are waiting on the epoller for an event, and as soon as there is event on any of the fds, it goes ahead and runs through the entire graph by calling graph.run(). Note that the epoller model will not be needed / it will need changes when we introduce polling based forwarding models like dpdk.

Main also launches an API handler thread which will listen to external API requests. The API handler threads all share a reference counted struct R2. The API handler threads get invoked when there is an external API call, in which case it takes actions like adding a route and doing a broadcast() of a message to all forwarding threads. And finally the main thread itself gets into a "wait for messages from forwarding thread" mode - like we explaiend earlier, the forwarding thread might want to send messages to control thread or to other forwarding threads. So this wait loop handles those messages.

## ifd.rs

The interface handling code in this file also includes API callbacks for adding interfaces, modifying interface parameters like ip addresses and interface QoS queues. Each interface in the system has an index called "ifindex" - pretty standard concept in any linux/unix. Note that the interfaces and ifindices etc.. have nothing whatsoever to do with linux interfaces and ifindices. R2 does not really care about linux interfaces or linux forwarding - R2 has its own internal ifindices different from linux and its own forwarding seperate from linux. 

When an external entity calls the API to add an interface, we end up calling create_interface_node() which basically creates a graph node and sends the graph node as a broadcast() message to all the forwarding threads. As we mentioned earlier, the broadcast() will clone() the message - and the graph nodes are designed to have clone() APIs, so it works well. And each forwarding thread on receiving the message adds the interface node to the graph and calls graph.finalize() to update the other nodes with indices of the newly added node. 

Similarly the handle_add_ip() handles the changes in interface parameters like ip address (and later other parameters like mtu or bandwidth etc.. can be added on). The parameters of the interface are used by the forwarding threads. Like we discussed in the architecture section, the goal here is to copy the parameters to a new interface structure and send the new structure as a message to the forwarding threads - and the forwarding threads will swap out their interface with the new one, in one simple light weight step. So the existing interface is cloned(), and the new parameters are set and we call broadcast() to send a message to all forwarding threads. Similar stuff happens when we call handle_add_class() to modify the QoS parameters of the interface.

## ipv4.rs

The API callback in this file gets invoked when there is a route add/del triggered externally. Like we had explained in the architecture section, ipv4 route table is organized as an active/backup copy. Again, the routes themselves are shared, only the table (the tree, tree nodes etc..) are duplicated. The add_or_del_route() API first modifies the current backup table (WhichTable defines which one is primary and backup), then does a broadcast() message to all forwarding threads to switch to the backup table, and then it waits till all the reference counts on the active table drops and the reference count becomes 1 - the Arc::get_mut() will succeed only if the reference count drops to 1. At that point the old active (right now backup) is also modified and that completes the sequence.

Note that the control thread waiting for forwarding threads to drop reference should not take long - the forwarding threads are in the business of packet forwarding, so they are not going to have a sleep-for-ten-minutes kind of things you expect from control threads. Even so, adding a control thread yield while spin looping for ref count drop to 1 might be a good TODO item.

## log.rs

Here we handle API callbacks to dump the log from each forwarding thread. Details are in logger.md. Also note that we dump the log files, but dont merge them, we expect an external utility to do that. Also as explained in logger.md, it might be a good idea to just stop the loggers in this API handler and let the external utility do the dumping also.