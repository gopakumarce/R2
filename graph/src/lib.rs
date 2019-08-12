use counters::flavors::{Counter, CounterType};
use counters::Counters;
use log::Logger;
use packet::BoxPkt;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

// We preallocate space for these many graph nodes, of course it can grow beyond that,
// but the goal is as much as possible to pre-allocate space
const GRAPH_INIT_SZ: usize = 1024;
/// The size of the packet queue to each graph node. Beyond this, packets to that node
/// will get dropped
pub const VEC_SIZE: usize = 256;

/// Every graph node feature/client needs to implement these methods/APIs
pub trait Gclient<T>: Send {
    /// Make a clone() of the node, usually to be used in another thread. It is upto the
    /// client to decide what should be cloned/copied and what should be shared. For example,
    /// counters are always per thread and cant be shared, a new set of counters need to be
    /// made per thread
    fn clone(&self, _counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<T>>;
    /// This API is called to hand over packets to the client for processing. Dispatch has
    /// pop() API to get packets destined for the node, and push() API to push packets to
    /// other graph nodes
    fn dispatch(&mut self, _thread: usize, _vectors: &mut Dispatch);
    /// This API is called when a node gets a message from control plane, like for example
    /// to modify the nodes forwarding tables etc..
    fn control_msg(&mut self, _thread: usize, _message: T) {}
}

/// This structure provides methods to get packets queued up for a node, and for
/// the node to queue up packets to other nodes
pub struct Dispatch<'d> {
    node: usize,
    vectors: &'d mut Vec<VecDeque<BoxPkt>>,
    counters: &'d mut Vec<GnodeCntrs>,
    nodes: &'d Vec<usize>,
    work: bool,
    wakeup: usize,
}

impl<'d> Dispatch<'d> {
    /// Get one of the packets queued up for a node
    pub fn pop(&mut self) -> Option<BoxPkt> {
        self.vectors[self.node].pop_front()
    }

    /// Queue one packet to another node
    pub fn push(&mut self, node: usize, pkt: BoxPkt) -> bool {
        let node = self.nodes[node];
        if self.vectors[node].capacity() >= 1 {
            self.vectors[node].push_back(pkt);
            if node <= self.node {
                self.work = true;
                self.wakeup = 0;
            }
            self.counters[node].enqed.incr();
            true
        } else {
            self.counters[node].drops.incr();
            false
        }
    }

    /// Specify the time when this node has work again/needs to be scheduled again
    /// wakeup of zero means it has work right now, non zero wakeup indicates time
    /// in nanoseconds from now when the node has work
    pub fn wakeup(&mut self, wakeup: usize) {
        if self.work {
            if wakeup < self.wakeup {
                self.wakeup = wakeup;
            }
        } else {
            self.work = true;
            self.wakeup = wakeup;
        }
    }
}

/// The parameters each feature/client node needs to specify if it wants to be added
/// to the graph
pub struct GnodeInit {
    /// A unique name for the node
    pub name: String,
    /// Names of all the nodes this node will have edges to (ie will send packets to)
    pub next_names: Vec<String>,
    /// A set of generic counters that tracks the node's enqueue/dequeue/drops etc..
    pub cntrs: GnodeCntrs,
}

impl GnodeInit {
    pub fn clone(&self, counters: &mut Counters) -> GnodeInit {
        GnodeInit {
            name: self.name.clone(),
            next_names: self.next_names.clone(),
            cntrs: GnodeCntrs::new(&self.name, counters),
        }
    }
}

pub struct GnodeCntrs {
    enqed: Counter,
    drops: Counter,
}

impl GnodeCntrs {
    pub fn new(name: &str, counters: &mut Counters) -> GnodeCntrs {
        let enqed = Counter::new(counters, name, CounterType::Pkts, "GraphEnq");
        let drops = Counter::new(counters, name, CounterType::Error, "GraphDrop");
        GnodeCntrs { enqed, drops }
    }
}

// The Gnode structure holds the exact node feature/client object and some metadata
// associated with the client
struct Gnode<T> {
    // The feature/client object
    client: Box<dyn Gclient<T>>,
    // Name of the feature/client
    name: String,
    // Names of all the nodes this node will have edges to (ie will send packets to)
    next_names: Vec<String>,
    // Node ids corresponding to the names in next_names
    next_nodes: Vec<usize>,
}

impl<T> Gnode<T> {
    fn new(client: Box<dyn Gclient<T>>, name: &str, next_names: Vec<String>) -> Gnode<T> {
        Gnode {
            client,
            name: name.to_string(),
            next_names,
            next_nodes: Vec::new(),
        }
    }

    fn clone(&self, counters: &mut Counters, log: Arc<Logger>) -> Gnode<T> {
        Gnode {
            client: self.client.clone(counters, log),
            name: self.name.clone(),
            next_names: self.next_names.clone(),
            next_nodes: self.next_nodes.clone(),
        }
    }
}

// The Graph object, basically a collection of graph nodes and edges from node to node
// Usually there is one Graph per thread, the graphs in each thread are copies of each other
pub struct Graph<T> {
    // The thread this graph belongs to
    thread: usize,
    // The graph nodes
    nodes: Vec<Gnode<T>>,
    // A per node packet queue, to hold packets from other nodes to this node
    vectors: Vec<VecDeque<BoxPkt>>,
    // Generic enq/deq/drop counters per node
    counters: Vec<GnodeCntrs>,
    // Each graph node has an index which is an offset into the nodes Vec in this structure.
    // This hashmap provides a mapping from a graph node name to its index
    indices: HashMap<String, usize>,
}

impl<T> Graph<T> {
    /// A new graph is created with just one node in it, a Drop Node that just drops any packet
    /// it receives.
    pub fn new(thread: usize, counters: &mut Counters) -> Graph<T> {
        let mut g = Graph {
            thread,
            nodes: Vec::with_capacity(GRAPH_INIT_SZ),
            vectors: Vec::with_capacity(GRAPH_INIT_SZ),
            counters: Vec::with_capacity(GRAPH_INIT_SZ),
            indices: HashMap::with_capacity(GRAPH_INIT_SZ),
        };
        let init = GnodeInit {
            name: names::DROP.to_string(),
            next_names: vec![],
            cntrs: GnodeCntrs::new(names::DROP, counters),
        };
        let count = Counter::new(counters, names::DROP, CounterType::Pkts, "count");
        g.add(Box::new(DropNode { count }), init);
        g
    }

    /// Clone the entire graph. That relies on each graph node feature/client providing
    /// an ability to clone() itself
    pub fn clone(&self, thread: usize, counters: &mut Counters, log: Arc<Logger>) -> Graph<T> {
        let mut nodes = Vec::with_capacity(GRAPH_INIT_SZ);
        let mut vectors = Vec::with_capacity(GRAPH_INIT_SZ);
        let mut cntrs = Vec::with_capacity(GRAPH_INIT_SZ);
        for n in self.nodes.iter() {
            nodes.push(n.clone(counters, log.clone()));
            vectors.push(VecDeque::with_capacity(VEC_SIZE));
            cntrs.push(GnodeCntrs::new(&n.name, counters));
        }
        Graph {
            thread,
            nodes,
            vectors,
            counters: cntrs,
            indices: self.indices.clone(),
        }
    }

    /// Add a new feature/client node to the graph.
    pub fn add(&mut self, client: Box<dyn Gclient<T>>, init: GnodeInit) {
        let index = self.index(&init.name);
        if index != 0 {
            return; // Gclient already registered
        }

        self.nodes
            .push(Gnode::new(client, &init.name, init.next_names));
        self.vectors.push(VecDeque::with_capacity(VEC_SIZE));
        self.counters.push(init.cntrs);
        let index = self.nodes.len() - 1; // 0 based index
        self.indices.insert(init.name, index);
    }

    fn index(&self, name: &str) -> usize {
        if let Some(&index) = self.indices.get(name) {
            index
        } else {
            0
        }
    }

    /// Any time a new node is added to the graph, there might be other nodes that have
    /// specified this new node as their next node - so we have to resolve those names
    /// to a proper node index. The finalize() will walk through all nodes and resolve
    /// next_name to node index. This is typically called after a new node is added
    pub fn finalize(&mut self) {
        for n in 0..self.nodes.len() {
            let node = &self.nodes[n];
            for l in 0..node.next_names.len() {
                let node = &self.nodes[n];
                let index = self.index(&node.next_names[l]);
                let node = &mut self.nodes[n];
                if node.next_nodes.len() <= l {
                    node.next_nodes.resize(l + 1, 0);
                }
                node.next_nodes[l] = index;
            }
        }
    }

    // Run through all the nodes one single time, do whatever work is possible in that
    // iteration, and return values which say if more work is pending and at what time
    // the work has to be done
    pub fn run(&mut self) -> (bool, usize) {
        let mut nsecs = std::usize::MAX;
        let mut work = false;
        for n in 0..self.nodes.len() {
            let node = &mut self.nodes[n];
            let client = &mut node.client;
            let mut d = Dispatch {
                node: n,
                vectors: &mut self.vectors,
                counters: &mut self.counters,
                nodes: &node.next_nodes,
                work: false,
                wakeup: std::usize::MAX,
            };
            client.dispatch(self.thread, &mut d);
            // Does client have more work pending, and when does it need to do that work ?
            if d.work {
                work = true;
                if d.wakeup < nsecs {
                    nsecs = d.wakeup;
                }
            }
        }
        (work, nsecs)
    }

    pub fn control_msg(&mut self, name: &str, message: T) -> bool {
        let index = self.index(name);
        if index == 0 {
            false
        } else {
            self.nodes[index].client.control_msg(self.thread, message);
            true
        }
    }
}

struct DropNode {
    count: Counter,
}

impl<T> Gclient<T> for DropNode {
    fn clone(&self, counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<T>> {
        let count = Counter::new(counters, names::DROP, CounterType::Pkts, "count");
        Box::new(DropNode { count })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(_) = vectors.pop() {
            self.count.incr();
        }
    }
}

#[cfg(test)]
mod test;
