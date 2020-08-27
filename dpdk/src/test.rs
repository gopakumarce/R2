use super::*;

use counters::Counters;
use crossbeam_queue::ArrayQueue;
use packet::PacketPool;
use socket::RawSock;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time;

const NUM_PKTS: usize = 10;
const NUM_PART: usize = 20;
const PART_SZ: usize = 3072;
const MAX_PACKET: usize = 1500;

fn packet_free(q: Arc<ArrayQueue<BoxPkt>>, pool: &mut dyn PacketPool) {
    while let Ok(p) = q.pop() {
        pool.free(p);
    }
}

fn packet_pool(test: &str, q: Arc<ArrayQueue<BoxPkt>>) -> Box<PktsDpdk> {
    let mut counters = Counters::new(test).unwrap();
    Box::new(PktsDpdk::new(
        test,
        q,
        &mut counters,
        NUM_PKTS,
        NUM_PART,
        PART_SZ,
    ))
}

fn delete_veth() {
    let args = [
        "link", "del", "r2_eth1", "type", "veth", "peer", "name", "r2_eth2",
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
        "link", "add", "r2_eth1", "type", "veth", "peer", "name", "r2_eth2",
    ];
    Command::new("ip")
        .args(&args)
        .spawn()
        .expect("veth failed")
        .wait()
        .unwrap();

    let args = ["r2_eth1", "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig eth1 fail")
        .wait()
        .unwrap();
    let args = ["-w", &disable_ipv6("r2_eth1")];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();

    let args = ["-w", &disable_ipv6("r2_eth2")];
    Command::new("sysctl")
        .args(&args)
        .spawn()
        .expect("ipv6 disable fail")
        .wait()
        .unwrap();
    let args = ["r2_eth2", "up"];
    Command::new("ifconfig")
        .args(&args)
        .spawn()
        .expect("ifconfig eth2 fail")
        .wait()
        .unwrap();
}

struct DpdkThread {
    pool_rx: Box<PktsDpdk>,
    pool_tx: Box<PktsDpdk>,
    q_rx: Arc<ArrayQueue<BoxPkt>>,
    q_tx: Arc<ArrayQueue<BoxPkt>>,
    dpdk_rx: Dpdk,
    dpdk_tx: Dpdk,
    done: Arc<AtomicUsize>,
}

extern "C" fn dpdk_eal_thread(arg: *mut core::ffi::c_void) -> i32 {
    unsafe {
        let params: Box<DpdkThread> = Box::from_raw(arg as *mut DpdkThread);
        dpdk_thread(params);
        0
    }
}

fn dpdk_thread(mut params: Box<DpdkThread>) {
    let data: Vec<u8> = (0..MAX_PACKET).map(|x| (x % 256) as u8).collect();
    loop {
        packet_free(params.q_rx.clone(), &mut *params.pool_rx);
        packet_free(params.q_tx.clone(), &mut *params.pool_tx);
        let pkt = params.pool_tx.pkt(0);
        if pkt.is_none() {
            continue;
        }
        let mut pkt = pkt.unwrap();
        assert!(pkt.append(&mut *params.pool_tx, &data[0..]));
        assert_eq!(params.dpdk_tx.sendmsg(pkt), MAX_PACKET);

        let pkt = params.dpdk_rx.recvmsg(&mut *params.pool_rx, 0);
        if pkt.is_none() {
            continue;
        }
        let pkt = pkt.unwrap();
        let pktlen = pkt.len();
        assert_eq!(MAX_PACKET, pktlen);
        let (buf, len) = match pkt.data(0) {
            Some((d, s)) => (d, s),
            None => panic!("Cant get offset 0"),
        };
        assert_eq!(len, pktlen);
        for i in 0..MAX_PACKET {
            assert_eq!(buf[i], i as u8);
        }
        params.done.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn read_write() {
    delete_veth();
    create_veth();

    let mut glob = DpdkGlobal::new(128, 1);

    let q_tx = Arc::new(ArrayQueue::new(NUM_PKTS));
    let pool_tx = packet_pool("dpdk_read_write_tx", q_tx.clone());
    let params = Params {
        name: "r2_eth1",
        hw: DpdkHw::AfPacket,
        pool: pool_tx.dpdk_pool,
    };
    let dpdk_tx = match glob.add(params) {
        Ok(dpdk) => dpdk,
        Err(err) => panic!("Error {:?} creating dpdk port", err),
    };

    let q_rx = Arc::new(ArrayQueue::new(NUM_PKTS));
    let pool_rx = packet_pool("dpdk_read_write_rx", q_rx.clone());
    let params = Params {
        name: "r2_eth2",
        hw: DpdkHw::AfPacket,
        pool: pool_rx.dpdk_pool,
    };
    let dpdk_rx = match glob.add(params) {
        Ok(dpdk) => dpdk,
        Err(err) => panic!("Error {:?} creating dpdk port", err),
    };

    let wait = Arc::new(AtomicUsize::new(0));
    let done = wait.clone();

    let params = Box::new(DpdkThread {
        pool_rx,
        pool_tx,
        q_rx,
        q_tx,
        dpdk_rx,
        dpdk_tx,
        done,
    });

    dpdk_launch(
        Some(dpdk_eal_thread),
        Box::into_raw(params) as *mut core::ffi::c_void,
    );

    while wait.load(Ordering::Relaxed) == 0 {
        let wait = time::Duration::from_millis(1);
        thread::sleep(wait)
    }

    delete_veth();
}
