# Graph

This is the central piece of R2. The Graph structure consists of the following main items

1. Vector of Nodes
2. Vector of packets queues - queues waiting to be processed by each node (dispatch vectors)
3. A dictionary of indices for each node - an index into the vector in 1) above

The graph is basically a collection of nodes and each node specifies its list of next-nodes. As of today, there is only nodes that can get added to the graph, nothing gets deleted from it (we can extend it if need be, maybe just mark a node as deleted etc..). And since nodes are added to a vector in 1) above, the index corresponding to the node is simply an offset into that vector. And the list of next-nodes of each node is thus simply a list of indices.

## Gnode

The node structure has a Client object with a set of APIs corresponding which each client object should provide. One of the APIs is dispatch(), which is the API that gives clients the packets waiting to be processed. The other API is clone(), which is used to make copies of the graph (which involves copying each node). And the last API is control_msg() - if the control plane wants to send a message to the nodes (like add a route), then the client gets this callback.

When the client asks for a node to be created and inserted into the graph, it provides the client object as a parameter. It also provides a list of names of the next-nodes. Once all the nodes are inserted to the graph, the graph creator calls the finalize() API on the graph which will basically update each node with a next-node-name to next-node-index translation.

The graph run() API walks through every single node, and calls the dispatch() API on the client. The dispatch is called with a Dispatch structure, which basically contains the dispatch vector of every node in the graph - the client will take packets as inputs from its own dispatch vector and queue them to dispatch vectors of the other nodes. The Dispatch structure also provides the list of node-ids of the next-nodes of that particular node - remember we had mentioned earlier how each node gives the names of its next-nodes as a list and how we convert it to node-ids in finalize(), after the graph is all ready.

The node client is any structure that provides the APIs mentioned earlier. And the general structure of a node is as below

1. Client will have its own name - a "well known" name listed in the names module
2. Client will provide a list of names of next-nodes. Usually its done by having an enum Next which gives names to next-node array indices and then a NEXT_NAMES array which gives names of the next-nodes. There is no requirement to do it that way, although its nice to have a uniform way of doing things
3. Client will use the pop() method of the Dispatch object passed in to get its own input packets one by one, process the packet and send them to another node using push() method of the Dispatch object
4. Client does not know about the "actual" node indices etc.., the client always refers to its own local next-node array to refer to the node it wants to push the packet to, the Dispatch object will figure out how to convert that to the actual graph index
