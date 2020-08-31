use super::*;
use crossbeam_queue::ArrayQueue;
use log::log;
use packet::{PacketPool, PktsHeap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const NUM_PKTS: usize = 10;
const NUM_PART: usize = 20;
const PARTICLE_SZ: usize = 256;

fn packet_pool(test: &str) -> (Box<dyn PacketPool>, Arc<ArrayQueue<BoxPkt>>) {
    let q = Arc::new(ArrayQueue::<BoxPkt>::new(NUM_PKTS));
    let mut counters = Counters::new(test).unwrap();
    (
        Box::new(PktsHeap::new(
            "PKTS_HEAP",
            q.clone(),
            &mut counters,
            NUM_PKTS,
            NUM_PART,
            PARTICLE_SZ,
        )),
        q,
    )
}

// Just add a sequence number as the data in the packet
fn new_pkt(pool: &mut dyn PacketPool, count: usize) -> BoxPkt {
    let cnt = count as u32;
    let mut pkt = pool.pkt(0).unwrap();
    let c = cnt.to_be_bytes();
    let v = c.to_vec();
    assert!(pkt.append(pool, &v));
    pkt
}

// Read the packet data and ensure it has the sequence number we expect
fn validate_pkt(pkt: &mut BoxPkt, count: usize) {
    let (data, size) = match pkt.data(0) {
        Some((d, s)) => (d, s),
        None => panic!("Empty data in PrintNode"),
    };
    assert_eq!(size, 4);
    let v = [data[0], data[1], data[2], data[3]];
    let val = u32::from_be_bytes(v);
    assert_eq!(val, count as u32);
}

#[derive(Copy, Clone)]
enum Next {
    RX = 0,
    PRINT,
    TX,
}

const NEXT_NAMES: &[Next] = &[Next::RX, Next::PRINT, Next::TX];

fn next_name(next: Next, thread: usize) -> String {
    match next {
        Next::RX => {
            let mut rx = "RX".to_string();
            rx.push_str(&thread.to_string());
            rx
        }
        Next::PRINT => "PRINT".to_string(),
        Next::TX => "TX".to_string(),
    }
}

struct RxNode {
    affinity: usize,
    count: usize,
    total_count: Arc<AtomicUsize>,
}

impl RxNode {
    fn new(affinity: usize) -> RxNode {
        RxNode {
            affinity,
            count: 0,
            total_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn name(&self) -> String {
        "RX".to_string()
    }

    fn next_names(&self, thread: usize) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            // The values have to be ordered as 0, 1, 2 ..
            assert_eq!(*n as usize, v.len());
            v.push(next_name(*n, thread));
        }
        v
    }
}

struct TestMsg {}

impl Gclient<TestMsg> for RxNode {
    fn clone(&self, _counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<TestMsg>> {
        Box::new(RxNode {
            affinity: self.affinity,
            count: 0,
            total_count: self.total_count.clone(),
        })
    }

    fn dispatch(&mut self, thread: usize, vectors: &mut Dispatch) {
        if self.affinity != thread {
            return;
        }
        let pkt = new_pkt(vectors.pool, self.count);
        vectors.push(Next::PRINT as usize, pkt);
        self.count += 1;
        self.total_count.fetch_add(1, Ordering::Relaxed);
    }
}

struct TxNode {
    count: usize,
    total_count: Arc<AtomicUsize>,
}

impl TxNode {
    fn new() -> TxNode {
        TxNode {
            count: 0,
            total_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn name(&self) -> String {
        "TX".to_string()
    }

    fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            // The values have to be ordered as 0, 1, 2 ..
            assert_eq!(*n as usize, v.len());
            v.push(next_name(*n, 0));
        }
        v
    }
}

impl Gclient<TestMsg> for TxNode {
    fn clone(&self, _counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<TestMsg>> {
        Box::new(TxNode {
            count: 0,
            total_count: self.total_count.clone(),
        })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(mut pkt) = vectors.pop() {
            validate_pkt(&mut pkt, self.count);
            self.count += 1;
            self.total_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct PrintNode {
    count: usize,
    total_count: Arc<AtomicUsize>,
}

impl PrintNode {
    fn new() -> PrintNode {
        PrintNode {
            count: 0,
            total_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn name(&self) -> String {
        "PRINT".to_string()
    }

    fn next_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in NEXT_NAMES {
            // The values have to be ordered as 0, 1, 2 ..
            assert_eq!(*n as usize, v.len());
            v.push(next_name(*n, 0));
        }
        v
    }
}

impl Gclient<TestMsg> for PrintNode {
    fn clone(&self, _counters: &mut Counters, _log: Arc<Logger>) -> Box<dyn Gclient<TestMsg>> {
        Box::new(PrintNode {
            count: 0,
            total_count: self.total_count.clone(),
        })
    }

    fn dispatch(&mut self, _thread: usize, vectors: &mut Dispatch) {
        while let Some(mut pkt) = vectors.pop() {
            validate_pkt(&mut pkt, self.count);
            self.count += 1;
            self.total_count.fetch_add(1, Ordering::Relaxed);
            vectors.push(Next::TX as usize, pkt);
        }
    }
}

#[test]
// Single thread: rx, print, tx all runs in the same thread. Note that each node has
// Rc + atomic self.total_count because once a node is cloned, its non-shared self.count
// will not reflect what is in the node inside the graph
fn single_thread() {
    let mut counters = match Counters::new("r2_graph_single") {
        Ok(c) => c,
        Err(errno) => panic!("Unable to create counters, errno {}", errno),
    };
    let log = Arc::new(Logger::new("r2_logs", 32, 1000).unwrap());
    let (pool, queue) = packet_pool("single_thread");
    let mut graph = Graph::new(0, pool, queue, &mut counters);
    let rx = Box::new(RxNode::new(0));
    let tx = Box::new(TxNode::new());
    let print = Box::new(PrintNode::new());

    let init = GnodeInit {
        name: rx.name(),
        next_names: rx.next_names(0),
        cntrs: GnodeCntrs::new(&rx.name(), &mut counters),
        perf: Perf::new(&rx.name(), &mut counters),
    };
    graph.add(rx.clone(&mut counters, log.clone()), init);

    let init = GnodeInit {
        name: tx.name(),
        next_names: tx.next_names(),
        cntrs: GnodeCntrs::new(&tx.name(), &mut counters),
        perf: Perf::new(&tx.name(), &mut counters),
    };
    graph.add(tx.clone(&mut counters, log.clone()), init);

    let init = GnodeInit {
        name: print.name(),
        next_names: print.next_names(),
        cntrs: GnodeCntrs::new(&print.name(), &mut counters),
        perf: Perf::new(&print.name(), &mut counters),
    };
    graph.add(print.clone(&mut counters, log), init);

    graph.finalize();

    let test_count = 10;
    for _ in 0..test_count {
        graph.run();
    }

    let rcnt = rx.total_count.load(Ordering::Relaxed);
    let tcnt = tx.total_count.load(Ordering::Relaxed);
    let pcnt = print.total_count.load(Ordering::Relaxed);
    assert_eq!(rcnt, test_count);
    assert_eq!(tcnt, rcnt - 1);
    assert_eq!(pcnt, rcnt);
}

#[test]
// 8 threads, there are 8 rx nodes, all of them are added to the graph and the same graph
// runs on all the threads, but we use ensure that each rx node "runs" only on one thread
// - this is like the case of an rx driver node, where we pin one interface/port to one
// thread, and another to another thread etc..
fn multi_thread() {
    let mut counters = match Counters::new("r2_graph_multi") {
        Ok(c) => c,
        Err(errno) => panic!("Unable to create counters, errno {}", errno),
    };
    let (pool, queue) = packet_pool("multi_thread0");
    let log = Arc::new(Logger::new("r2_logs", 32, 1000).unwrap());
    let mut graph = Graph::new(0, pool, queue, &mut counters);
    let tx = Box::new(TxNode::new());
    let print = Box::new(PrintNode::new());

    let test_threads = 8;
    let mut rx_vec = Vec::new();
    for i in 1..=test_threads {
        let rx = Box::new(RxNode::new(i));
        let init = GnodeInit {
            name: rx.name(),
            next_names: rx.next_names(0),
            cntrs: GnodeCntrs::new(&rx.name(), &mut counters),
            perf: Perf::new(&rx.name(), &mut counters),
        };
        graph.add(rx.clone(&mut counters, log.clone()), init);
        rx_vec.push(rx);
    }

    let init = GnodeInit {
        name: tx.name(),
        next_names: tx.next_names(),
        cntrs: GnodeCntrs::new(&tx.name(), &mut counters),
        perf: Perf::new(&tx.name(), &mut counters),
    };
    graph.add(tx.clone(&mut counters, log.clone()), init);

    let init = GnodeInit {
        name: print.name(),
        next_names: print.next_names(),
        cntrs: GnodeCntrs::new(&print.name(), &mut counters),
        perf: Perf::new(&print.name(), &mut counters),
    };
    graph.add(print.clone(&mut counters, log.clone()), init);

    graph.finalize();

    let mut handlers = Vec::new();
    for i in 1..=test_threads {
        let pool_name = format!("multi_thread{}", i);
        let (pool, queue) = packet_pool(&pool_name);
        let mut g = graph.clone(i, pool, queue, &mut counters, log.clone());
        let name = format!("r2{}", i);
        let handler = thread::Builder::new().name(name).spawn(move || {
            let test_count = 10;
            for _ in 0..test_count {
                g.run();
            }
        });
        handlers.push(handler);
    }
    for handler in handlers {
        handler.unwrap().join().unwrap();
    }

    log!(log, "Done! %d", 0);

    let mut rcnt = 0;
    for rx in rx_vec {
        rcnt += rx.total_count.load(Ordering::Relaxed);
    }
    let tcnt = tx.total_count.load(Ordering::Relaxed);
    let pcnt = print.total_count.load(Ordering::Relaxed);
    println!("RX COUNT {}", rcnt);
    assert_eq!(tcnt, rcnt - 1);
    assert_eq!(pcnt, rcnt);
}
