use super::*;
use counters::Counters;
use crossbeam_queue::ArrayQueue;
use packet::{PacketPool, PktsHeap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const PKTSZ: usize = 256;
const DATA: [u8; PKTSZ] = [0; PKTSZ];
// Time in nano seconds to send one packet
const PKT_TIME: u64 = PKTSZ as u64 * 8 * 1_000_000_000 / (1024 * 1024);
const NUM_PKTS: usize = 4096;
const NUM_PART: usize = 2 * 4096;

fn packet_pool(test: &str) -> Box<dyn PacketPool> {
    let q = Arc::new(ArrayQueue::<BoxPkt>::new(NUM_PKTS));
    let mut counters = Counters::new(test).unwrap();
    Box::new(PktsHeap::new(
        "PKTS_HEAP",
        q,
        &mut counters,
        NUM_PKTS,
        NUM_PART,
        PKTSZ,
    ))
}

// Create two classes with bandwidth ratio 1:10
#[test]
fn one_level_linkshare() {
    let mut pool = packet_pool("hfsc_1lvl_lshare");
    let f_sc_10mb = Sc {
        m1: 0,
        d: 0,
        m2: 10_000_000,
    };
    let f_sc_1mb = Sc {
        m1: 0,
        d: 0,
        m2: 1_000_000,
    };
    let mut hfsc = Hfsc::new(100_000_000);

    hfsc.create_class(
        "class1".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let class1 = hfsc.class_index("class1".to_string()).unwrap();

    hfsc.create_class(
        "class2".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let class2 = hfsc.class_index("class2".to_string()).unwrap();
    assert_eq!(hfsc.classes[1].children.len(), 0);

    for _ in 0..512 {
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class1;
        hfsc.enqueue(class1, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class2;
        hfsc.enqueue(class2, pkt);
    }
    assert_eq!(hfsc.classes[hfsc.root].children.len(), 2);
    assert_eq!(hfsc.eligible.len(), 0);
    assert_eq!(hfsc.classes[class1].children.len(), 0);
    assert_eq!(hfsc.classes[class2].children.len(), 0);

    let mut class1_pkts = 0;
    let mut class2_pkts = 0;
    for _ in 0..(512 * 2) {
        let pkt = hfsc.dequeue().unwrap();
        if pkt.out_ifindex == class1 {
            class1_pkts += 1;
        } else if pkt.out_ifindex == class2 {
            class2_pkts += 1;
        }
        // The packets dequeued ratio should be approx 10, with some tolerance added
        if !(class1_pkts + 1 >= class2_pkts) {
            println!("class1 pkts {}, class2 pkts {}", class1_pkts, class2_pkts);
            assert!(false);
        }
        if class1_pkts < 512 && class2_pkts != 0 && class1_pkts / class2_pkts > 11 {
            println!("Ratio {}", class1_pkts / class2_pkts);
            assert!(false);
        }

        if class1_pkts == 512 && class2_pkts < 512 {
            assert_eq!(hfsc.classes[hfsc.root].children.len(), 1);
        }
    }
    assert_eq!(hfsc.classes[hfsc.root].children.len(), 0);
    assert_eq!(hfsc.eligible.len(), 0);
    assert_eq!(class1_pkts, 512);
    assert_eq!(class2_pkts, 512);
}

// Level 1 has classes c1 and c2
// Level2 has classes c1 and c2 under Level 1 and Level 2 each
// l1_c1:l1_c2 is in ratio 10:1
// l1_c1_l2_c1:l1_c1_l2_c2 is ratio 1:10
// l1_c2_l2_c1:l1_c2_l2_c2 is ratio 10:1
#[test]
fn two_level_linkshare() {
    let mut pool = packet_pool("hfsc_2lvl_lshare");
    let f_sc_10mb = Sc {
        m1: 0,
        d: 0,
        m2: 10_000_000,
    };
    let f_sc_1mb = Sc {
        m1: 0,
        d: 0,
        m2: 1_000_000,
    };
    let mut hfsc = Hfsc::new(100_000_000);

    hfsc.create_class(
        "l1_c1".to_string(),
        "root".to_string(),
        0,
        false,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let l1_c1 = hfsc.class_index("l1_c1".to_string()).unwrap();
    hfsc.create_class(
        "l1_c2".to_string(),
        "root".to_string(),
        0,
        false,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let l1_c2 = hfsc.class_index("l1_c2".to_string()).unwrap();
    hfsc.create_class(
        "l1_c1_l2_c1".to_string(),
        "l1_c1".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let l1_c1_l2_c1 = hfsc.class_index("l1_c1_l2_c1".to_string()).unwrap();
    hfsc.create_class(
        "l1_c1_l2_c2".to_string(),
        "l1_c1".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let l1_c1_l2_c2 = hfsc.class_index("l1_c1_l2_c2".to_string()).unwrap();
    hfsc.create_class(
        "l1_c2_l2_c1".to_string(),
        "l1_c2".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let l1_c2_l2_c1 = hfsc.class_index("l1_c2_l2_c1".to_string()).unwrap();
    hfsc.create_class(
        "l1_c2_l2_c2".to_string(),
        "l1_c2".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let l1_c2_l2_c2 = hfsc.class_index("l1_c2_l2_c2".to_string()).unwrap();

    for _ in 0..512 {
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = l1_c1_l2_c1;
        hfsc.enqueue(l1_c1_l2_c1, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = l1_c1_l2_c2;
        hfsc.enqueue(l1_c1_l2_c2, pkt);

        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = l1_c2_l2_c1;
        hfsc.enqueue(l1_c2_l2_c1, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = l1_c2_l2_c2;
        hfsc.enqueue(l1_c2_l2_c2, pkt);
    }

    assert_eq!(hfsc.eligible.len(), 0);
    assert_eq!(hfsc.classes[hfsc.root].children.len(), 2);
    assert_eq!(hfsc.classes[l1_c1].children.len(), 2);
    assert_eq!(hfsc.classes[l1_c2].children.len(), 2);
    assert_eq!(hfsc.classes[l1_c1_l2_c1].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c1_l2_c2].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c2_l2_c1].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c2_l2_c2].children.len(), 0);

    let mut l1_c1_pkts = 0;
    let mut l1_c2_pkts = 0;
    let mut l1_c1_l2_c1_pkts = 0;
    let mut l1_c1_l2_c2_pkts = 0;
    let mut l1_c2_l2_c1_pkts = 0;
    let mut l1_c2_l2_c2_pkts = 0;
    for _ in 0..(512 * 4) {
        let pkt = hfsc.dequeue().unwrap();
        if pkt.out_ifindex == l1_c1_l2_c1 {
            l1_c1_l2_c1_pkts += 1;
            l1_c1_pkts += 1;
        } else if pkt.out_ifindex == l1_c1_l2_c2 {
            l1_c1_l2_c2_pkts += 1;
            l1_c1_pkts += 1;
        } else if pkt.out_ifindex == l1_c2_l2_c1 {
            l1_c2_l2_c1_pkts += 1;
            l1_c2_pkts += 1;
        } else if pkt.out_ifindex == l1_c2_l2_c2 {
            l1_c2_l2_c2_pkts += 1;
            l1_c2_pkts += 1;
        }

        if !(l1_c1_pkts + 1 >= l1_c2_pkts) {
            println!("l1_c1_pkts {}, l1_c2_pkts {}", l1_c1_pkts, l1_c2_pkts);
            assert!(false);
        }
        if l1_c1_pkts < (512 * 2) && l1_c2_pkts != 0 && l1_c1_pkts / l1_c2_pkts > 11 {
            println!("L1 ratio {}", l1_c1_pkts / l1_c2_pkts);
            assert!(false);
        }

        if !(l1_c1_l2_c2_pkts + 1 >= l1_c1_l2_c1_pkts) {
            println!(
                "l1_c1_l2_c2_pkts {}, l1_c1_l2_c1_pkts{}",
                l1_c1_l2_c2_pkts, l1_c1_l2_c1_pkts
            );
            assert!(false);
        }
        if l1_c1_l2_c2_pkts < 512
            && l1_c1_l2_c1_pkts != 0
            && l1_c1_l2_c2_pkts / l1_c1_l2_c1_pkts > 11
        {
            println!("L1 ratio {}", l1_c1_l2_c2_pkts / l1_c1_l2_c1_pkts);
            assert!(false);
        }

        if !(l1_c2_l2_c1_pkts + 1 >= l1_c2_l2_c2_pkts) {
            println!(
                "l1_c2_l2_c1_pkts {}, l1_c2_l2_c2_pkts {}",
                l1_c2_l2_c1_pkts, l1_c2_l2_c2_pkts
            );
            assert!(false);
        }
        if l1_c2_l2_c1_pkts < 512
            && l1_c2_l2_c2_pkts != 0
            && l1_c2_l2_c1_pkts / l1_c2_l2_c2_pkts > 11
        {
            println!("L1 ratio {}", l1_c2_l2_c1_pkts / l1_c2_l2_c2_pkts);
            assert!(false);
        }
    }

    assert_eq!(l1_c1_pkts, 512 * 2);
    assert_eq!(l1_c1_l2_c1_pkts, 512);
    assert_eq!(l1_c1_l2_c2_pkts, 512);
    assert_eq!(l1_c2_pkts, 512 * 2);
    assert_eq!(l1_c2_l2_c1_pkts, 512);
    assert_eq!(l1_c2_l2_c2_pkts, 512);

    assert_eq!(hfsc.classes[hfsc.root].children.len(), 0);
    assert_eq!(hfsc.eligible.len(), 0);
    assert_eq!(hfsc.classes[l1_c1].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c2].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c1_l2_c1].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c1_l2_c2].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c2_l2_c1].children.len(), 0);
    assert_eq!(hfsc.classes[l1_c2_l2_c2].children.len(), 0);
}

static TIMER: AtomicU64 = AtomicU64::new(0);

fn test_get_time_ns() -> u64 {
    TIMER.fetch_add(PKT_TIME + 1, Ordering::Relaxed);
    TIMER.load(Ordering::Relaxed)
}

// The two realtime classes have to be drained before the linkshare ones,
// because we spoof the time using test_get_time_ns() to make it look as
// if the realtime sessions are lagging behind in time
#[test]
fn single_level_realtime() {
    let mut pool = packet_pool("hfsc_1lvl_rt");
    let f_sc_10mb = Sc {
        m1: 0,
        d: 0,
        m2: 10_000_000,
    };
    let f_sc_1mb = Sc {
        m1: 0,
        d: 0,
        m2: 1_000_000,
    };
    let r_sc_1mb = Sc {
        m1: 0,
        d: 0,
        m2: 1_000_000,
    };
    let r_sc_10mb = Sc {
        m1: 0,
        d: 0,
        m2: 10_000_000,
    };
    let mut hfsc = Hfsc::new(100_000_000);
    hfsc.get_time_ns = test_get_time_ns;

    hfsc.create_class(
        "class1".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let class1 = hfsc.class_index("class1".to_string()).unwrap();
    hfsc.create_class(
        "class2".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: None,
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let class2 = hfsc.class_index("class2".to_string()).unwrap();
    hfsc.create_class(
        "class3".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: Some(r_sc_1mb),
            u_sc: None,
            f_sc: f_sc_10mb,
        },
    )
    .unwrap();
    let class3 = hfsc.class_index("class3".to_string()).unwrap();
    hfsc.create_class(
        "class4".to_string(),
        "root".to_string(),
        0,
        true,
        Curves {
            r_sc: Some(r_sc_10mb),
            u_sc: None,
            f_sc: f_sc_1mb,
        },
    )
    .unwrap();
    let class4 = hfsc.class_index("class4".to_string()).unwrap();
    assert_eq!(hfsc.classes[1].children.len(), 0);
    assert_eq!(hfsc.eligible.len(), 0);

    for _ in 0..512 {
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class1;
        hfsc.enqueue(class1, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class2;
        hfsc.enqueue(class2, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class3;
        hfsc.enqueue(class3, pkt);
        let mut pkt = pool.pkt(0).unwrap();
        assert!(pkt.append(&mut *pool, &DATA));
        pkt.out_ifindex = class4;
        hfsc.enqueue(class4, pkt);
    }
    assert_eq!(hfsc.classes[hfsc.root].children.len(), 4);
    assert_eq!(hfsc.eligible.len(), 2);
    assert_eq!(hfsc.classes[class1].children.len(), 0);
    assert_eq!(hfsc.classes[class2].children.len(), 0);

    let mut class1_pkts = 0;
    let mut class2_pkts = 0;
    let mut class3_pkts = 0;
    let mut class4_pkts = 0;
    for _ in 0..(512 * 4) {
        let pkt = hfsc.dequeue().unwrap();
        if pkt.out_ifindex == class1_pkts {
            class1_pkts += 1;
        } else if pkt.out_ifindex == class2_pkts {
            class2_pkts += 1;
        } else if pkt.out_ifindex == class3_pkts {
            class3_pkts += 1;
        } else if pkt.out_ifindex == class4_pkts {
            class4_pkts += 1;
        }
        if class3_pkts < 512 {
            assert_eq!(class1_pkts, 0);
            assert_eq!(class2_pkts, 0);
            assert!(class3_pkts + 1 > class4_pkts);
            if class4_pkts != 0 {
                assert!(class3_pkts / class4_pkts <= 11);
            }
        }

        if class3_pkts == 512 && class4_pkts == 12 {
            assert_eq!(hfsc.classes[hfsc.root].children.len(), 4);
            assert_eq!(hfsc.eligible.len(), 0);
            assert!(class1_pkts + 1 > class2_pkts);
            if class2_pkts != 0 {
                assert!(class1_pkts / class2_pkts <= 11);
            }
        }
    }
    assert_eq!(hfsc.classes[hfsc.root].children.len(), 0);
    assert_eq!(hfsc.eligible.len(), 0);
}
