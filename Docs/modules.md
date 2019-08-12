# Modules

R2 project is organized as a workspace which is a collection of independent Cargo-es (libraries). What typically happens in large software projects is that its hard to define the module dependencies, to know what module depends on what (often ending up with circular dependencies). We strive to define the dependency model here upfront.

## The major modules

1. names: this module defines the names of all the graph nodes, so modules can refer to the names of other modules in a consistent way

2. fwd: Like we explained in the architecture section, a packet can traverse multiple forwarding objects. This module defines ALL the forwarding objects used in the system (refer to section "Forwarding objects" in architecture details)

3. counters: shared memory counters used by a lot of other modules

4. log: forwarding path fast logging library

5. common: miscellaneous common utilities

6. packet: The basic packet defenition and packet manipulation libraries, foundational library used by all forwarding nodes.

7. graph: The library that deals with creating the forwarding graph and adding nodes to it etc..

8. gnodes: All the features that plug in to the graph as nodes

9. api: the library to let external utilities (in Rust) to make API calls to R2

10. apis: The thrift api defenitions of various modules.

11. msg: The control to data plane (and vice versa) message definitions

### Dependency

We are not trying to list here the dependency graph - that obviously can be derived from the Cargo.toml of the various modules. The goal here is to provide the dependency expectations between the major modules listed above.

#### The first tier

names, apis, common, counters, log and fwd are the "top tier" modules - almost anyone and everyone will depend on them directly or indirectly. names, api, common etc.. is obvious. counters and logging is very fundamental infrastructure, so its not surprising everyone depends on it. 

fwd needs some explanation. fwd defines the forwarading path objects, and the Packet structure might contain references to some of those objects. When the packet traverses from node to node, it might capture the forwarding object information in the packet itself. So packet has to depend on fwd. And packet is itself a foundational/fundamental module in the system, hence fwd becomes even more basic/fundamental. So every object in fwd should be composed of Rust standard lib objects or other objects in fwd itself.

#### The second tier

packet library depends on fwd. Packet is used by almost all of R2

#### The third tier

graph, msg are the next tier of modules - they depend on the modules above

#### The last tier

All the graph nodes (modules in gnodes/) depend on everything above. 
