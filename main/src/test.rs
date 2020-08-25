use super::ipv4::add_route;
use super::*;
use fwd::EthMacAddMsg;
use fwd::EthMacRaw;
use graph::Driver;
use packet::{BoxPkt, PacketPool, PktsHeap};
use socket::RawSock;
use std::net::Ipv4Addr;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const ETH_HDR_IPV4: [u8; 14] = [
    0xaa, 0xbb, 0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0, 0, 0, 0x08, 0x00,
];

// r2_eth1 will be used as a graph node with ifindex 0, r2_eth2 will be used for
// injecting and inspecting packets from outside the graph
const EXTERNAL_OUTPUT: &str = "r2_eout";
const GRAPH_INPUT: &str = "r2_gin";
const GRAPH_OUTPUT: &str = "r2_gout";
const EXTERNAL_INPUT: &str = "r2_ein";
const OUTPUT_IFINDEX: usize = 0;
const INPUT_IFINDEX: usize = 1;
const DATA_LEN: usize = 256;
const MAC_INPUT: &str = "aa:bb:de:ad:be:ef";
const MAC_OUTPUT: &str = "aa:bb:ca:fe:ba:be";
const NUM_PKTS: usize = 10;
const NUM_PART: usize = 20;

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
            DEF_PARTICLE_SZ,
        )),
        q,
    )
}

fn delete_veth() {
    let args = [
        "link",
        "del",
        EXTERNAL_OUTPUT,
        "type",
        "veth",
        "peer",
        "name",
        GRAPH_INPUT,
    ];
    Command::new("ip")
        .args(&args)
        .spawn()
        .expect("veth failed")
        .wait()
        .unwrap();
    let args = [
        "link",
        "del",
        EXTERNAL_INPUT,
        "type",
        "veth",
        "peer",
        "name",
        GRAPH_OUTPUT,
    ];
    Command::new("ip")
        .args(&args)
        .spawn()
        .expect("veth failed")
        .wait()
        .unwrap();
}

// We get random packets if ipv6 is enabled, we want only our own packets
fn disable_ipv6(eth: &str) -> String {
    let mut name = "net.ipv6.conf.".to_string();
    name.push_str(eth);
    name.push_str(".disable_ipv6=1");
    name
}

fn create_veth() {
    let args = [
        "link",
        "add",
        EXTERNAL_OUTPUT,
        "type",
        "veth",
        "peer",
        "name",
        GRAPH_INPUT,
    ];
    Command::new("ip")
        .args(&args)
        .spawn()
        .expect("veth failed")
        .wait()
        .unwrap();
    let args = [
        "link",
        "add",
        EXTERNAL_INPUT,
        "type",
        "veth",
        "peer",
        "name",
        GRAPH_OUTPUT,
    ];
    Command::new("ip")
        .args(&args)
        .spawn()
        .expect("veth failed")
        .wait()
        .unwrap();

    let args = [EXTERNAL_OUTPUT, "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig up fail")
        .wait()
        .unwrap();
    let args = ["-w", &disable_ipv6(EXTERNAL_OUTPUT)];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();

    let args = [GRAPH_INPUT, "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig up fail")
        .wait()
        .unwrap();
    let args = ["-w", &disable_ipv6(GRAPH_INPUT)];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();

    let args = [EXTERNAL_INPUT, "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig up fail")
        .wait()
        .unwrap();
    let args = ["-w", &disable_ipv6(EXTERNAL_INPUT)];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();

    let args = [GRAPH_OUTPUT, "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig up fail")
        .wait()
        .unwrap();
    let args = ["-w", &disable_ipv6(GRAPH_OUTPUT)];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();
}

fn packet_send(done: Arc<AtomicUsize>) -> std::thread::JoinHandle<()> {
    thread::Builder::new()
        .name("tx".to_string())
        .spawn(move || {
            let raw = match RawSock::new(EXTERNAL_OUTPUT, false) {
                Ok(sock) => sock,
                Err(errno) => panic!("Cant open packet socket, errno {}", errno),
            };
            let (mut pool, queue) = packet_pool("main_pkt_send");
            while done.load(Ordering::Relaxed) == 0 {
                let mut pkt = pool.pkt(0).unwrap();
                assert!(pkt.append(&mut *pool, &ETH_HDR_IPV4));
                let data: Vec<u8> = vec![0; DATA_LEN - 14];
                assert!(pkt.append(&mut *pool, &data));
                assert_eq!(raw.sendmsg(pkt), DATA_LEN);
                while let Ok(p) = queue.pop() {
                    pool.free(p);
                }
            }
        })
        .unwrap()
}

fn packet_rcv(done: Arc<AtomicUsize>) -> std::thread::JoinHandle<()> {
    thread::Builder::new()
        .name("rx".to_string())
        .spawn(move || {
            let raw = match RawSock::new(EXTERNAL_INPUT, false) {
                Ok(sock) => sock,
                Err(errno) => panic!("Cant open packet socket, errno {}", errno),
            };
            let (mut pool, queue) = packet_pool("main_pkt_recv");
            let mut rcv = 0;
            while rcv < 10 {
                let pkt = raw.recvmsg(&mut *pool, 0).unwrap();
                if pkt.len() != 0 {
                    assert_eq!(pkt.len(), DATA_LEN);
                    rcv += 1;
                }
                while let Ok(p) = queue.pop() {
                    pool.free(p);
                }
            }
            done.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap()
}

fn create_interfaces(r2: &mut R2) {
    let mac_in = fwd::str_to_mac(MAC_INPUT).unwrap();
    let mac_out = fwd::str_to_mac(MAC_OUTPUT).unwrap();

    match ifd::create_interface_node(r2, GRAPH_INPUT, INPUT_IFINDEX, mac_in.clone()) {
        Ok(_) => {}
        Err(errno) => panic!("Could not create input intf, errno {}", errno),
    }
    let mac_add = EthMacAddMsg {
        ifindex: INPUT_IFINDEX,
        ip: Ipv4Addr::new(0, 0, 0, 0),
        mac: EthMacRaw {
            bytes: Arc::new(mac_out.clone()),
        },
    };
    r2.broadcast(R2Msg::EthMacAdd(mac_add));

    match ifd::create_interface_node(r2, GRAPH_OUTPUT, OUTPUT_IFINDEX, mac_out.clone()) {
        Ok(_) => {}
        Err(errno) => panic!("Could not create output intf, errno {}", errno),
    }
    let mac_add = EthMacAddMsg {
        ifindex: OUTPUT_IFINDEX,
        ip: Ipv4Addr::new(0, 0, 0, 0),
        mac: EthMacRaw {
            bytes: Arc::new(mac_in),
        },
    };
    r2.broadcast(R2Msg::EthMacAdd(mac_add));
}

fn launch_test_threads(r2: &mut R2, done: Arc<AtomicUsize>, mut g: Graph<R2Msg>) {
    let (sender, receiver) = channel();
    r2.threads[0].ctrl2fwd = Some(sender);
    let efd = r2.threads[0].efd.clone();
    let mut epoll = Epoll::new(efd, MAX_FDS, -1, Box::new(R2Epoll {})).unwrap();

    let d = done.clone();
    thread::Builder::new()
        .name("r2-0".to_string())
        .spawn(move || loop {
            while d.load(Ordering::Relaxed) == 0 {
                g.run();
                ctrl2fwd_messages(0, &mut epoll, &receiver, &mut g);
            }
        })
        .unwrap();
}

#[test]
fn integ_test() {
    delete_veth();
    create_veth();

    let (sender, _receiver) = channel();
    let r2_rc = Arc::new(Mutex::new(R2::new(
        "integ_test",
        "r2_logs",
        32,
        1000,
        sender,
        1,
    )));
    let mut r2 = r2_rc.lock().unwrap();
    let (pool, queue) = packet_pool("main_graph");
    let mut graph = Graph::<R2Msg>::new(0, pool, queue, &mut r2.counters);
    create_nodes(&mut r2, &mut graph);
    let done = Arc::new(AtomicUsize::new(0));
    launch_test_threads(&mut r2, done.clone(), graph);
    create_interfaces(&mut r2);

    // Add a default route
    add_route(
        &mut r2,
        Ipv4Addr::new(0, 0, 0, 0),
        0,
        Ipv4Addr::new(0, 0, 0, 0),
        OUTPUT_IFINDEX,
    );

    let sender = packet_send(done.clone());
    let receiver = packet_rcv(done.clone());

    sender.join().unwrap();
    receiver.join().unwrap();

    delete_veth();
}
