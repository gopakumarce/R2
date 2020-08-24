use super::*;

const NUM_PKTS: usize = 10;
const NUM_PART: usize = 20;
const PARTICLE_SZ: usize = 512;

fn packet_pool(test: &str) -> Box<dyn PacketPool> {
    let q = Arc::new(ArrayQueue::new(NUM_PKTS));
    let mut counters = Counters::new(test).unwrap();
    Box::new(PktsHeap::new(
        q,
        &mut counters,
        NUM_PKTS,
        NUM_PART,
        PARTICLE_SZ,
    ))
}

fn nparticles(pkt: &BoxPkt) -> usize {
    let mut cnt = 1;
    let mut p = pkt.particle.as_ref().unwrap();
    while let Some(next) = p.next.as_ref() {
        cnt += 1;
        p = next;
    }
    cnt
}

fn verify_pkt(pkt: &mut BoxPkt) {
    for i in 0..pkt.len() {
        let (d, _) = match pkt.data(i) {
            Some((d, s)) => (d, s),
            None => panic!("Cannot find offset"),
        };
        assert_eq!(d[0], (i % 256) as u8)
    }
}

fn push_pkt(pool: &mut dyn PacketPool, headroom: usize, v: &Vec<u8>, sz: usize, npart: usize) {
    let mut pkt = pool.pkt(headroom).unwrap();
    let mut i = 0;
    // Push one byte at a time
    while i < v.len() {
        if (i + sz) >= v.len() {
            assert!(pkt.append(pool, &v[i..]));
            break;
        } else {
            assert!(pkt.append(pool, &v[i..i + sz]));
        }
        i += sz;
    }
    assert_eq!(pkt.len(), v.len());
    assert_eq!(nparticles(&pkt), npart);
    verify_pkt(&mut pkt);
}

fn append_w_headroom(pool: &mut dyn PacketPool, headroom: usize, npart: usize) {
    let need = npart * PARTICLE_SZ - headroom;
    let v: Vec<u8> = (0..need).map(|x| (x % 256) as u8).collect();

    // push one byte at a time
    push_pkt(pool, headroom, &v, 1, npart);

    // push two bytes at a time
    push_pkt(pool, headroom, &v, 2, npart);

    // push three bytes at a time
    push_pkt(pool, headroom, &v, 3, npart);

    // push hundred bytes at a time
    push_pkt(pool, headroom, &v, 100, npart);

    // push PARTICLE_SZ+1 bytes at a time
    push_pkt(pool, headroom, &v, PARTICLE_SZ + 1, npart);
}

#[test]
fn append_test() {
    let mut pool = packet_pool("append_test");
    append_w_headroom(&mut *pool, 100, 3);
}

#[test]
fn prepend_test() {
    let mut pool = packet_pool("prepend_test");
    let mut pkt = pool.pkt(100).unwrap();
    assert_eq!(pkt.headroom(), 100);
    let v: Vec<u8> = (0..100).map(|x| (x % 256) as u8).collect();
    assert!(pkt.prepend(&mut *pool, &v[0..]));
    assert_eq!(pkt.len(), 100);
    assert_eq!(pkt.headroom(), 0);
    assert_eq!(nparticles(&pkt), 1);
    verify_pkt(&mut pkt);

    let mut pkt = pool.pkt(100).unwrap();
    assert_eq!(pkt.headroom(), 100);
    let v: Vec<u8> = (0..200).map(|x| (x % 256) as u8).collect();
    assert!(pkt.prepend(&mut *pool, &v[0..]));
    assert_eq!(pkt.len(), 200);
    assert_eq!(pkt.headroom(), PARTICLE_SZ - 100); // 100 in the first particle, 100 in next
    assert_eq!(nparticles(&pkt), 2);
    verify_pkt(&mut pkt);
}

fn check_last_part(pkt: &mut BoxPkt, tail: usize) {
    let p = pkt.particle.as_mut().unwrap().last_particle();
    assert_eq!(p.tail, tail);
}

#[test]
fn move_tail_test() {
    let mut pool = packet_pool("move_tail_test");
    // One particle test
    let headroom = 100;
    let available = PARTICLE_SZ - headroom;
    let mut pkt = pool.pkt(headroom).unwrap();
    let bytes = vec![0 as u8; available - 10];
    assert!(pkt.append(&mut *pool, &bytes[0..]));
    assert_eq!(pkt.len(), available - 10);
    check_last_part(&mut pkt, headroom + available - 10);
    assert_eq!(pkt.move_tail(10), 10);
    assert_eq!(pkt.len(), available);
    check_last_part(&mut pkt, headroom + available);
    // Cant go forward any further
    assert_eq!(pkt.move_tail(1), 0);
    assert_eq!(pkt.len(), available);
    check_last_part(&mut pkt, headroom + available);
    // Now go back
    let back = 0 - available as isize;
    assert_eq!(pkt.move_tail(back), back);
    assert_eq!(pkt.len(), 0);
    check_last_part(&mut pkt, headroom);
    // Cant go back any further
    assert_eq!(pkt.move_tail(-1), 0);
    assert_eq!(pkt.len(), 0);
    check_last_part(&mut pkt, headroom);

    // Two particle test
    let headroom = 100;
    let available = 2 * PARTICLE_SZ - headroom;
    let mut pkt = pool.pkt(headroom).unwrap();
    let bytes = vec![0 as u8; available - 10];
    assert!(pkt.append(&mut *pool, &bytes[0..]));
    assert_eq!(pkt.len(), available - 10);
    check_last_part(&mut pkt, PARTICLE_SZ - 10);
    assert_eq!(pkt.move_tail(10), 10);
    assert_eq!(pkt.len(), available);
    check_last_part(&mut pkt, PARTICLE_SZ);
    // Cant go forward any further
    assert_eq!(pkt.move_tail(1), 0);
    assert_eq!(pkt.len(), available);
    check_last_part(&mut pkt, PARTICLE_SZ);
    // Now go back
    let back = 0 - PARTICLE_SZ as isize;
    assert_eq!(pkt.move_tail(back), back);
    assert_eq!(pkt.len(), available - PARTICLE_SZ);
    check_last_part(&mut pkt, 0);
    // Cant go back any further
    assert_eq!(pkt.move_tail(-1), 0);
    assert_eq!(pkt.len(), available - PARTICLE_SZ);
    check_last_part(&mut pkt, 0);
}

fn check_first_part(pkt: &BoxPkt, head: usize) {
    let p = &pkt.particle;
    assert_eq!(p.as_ref().unwrap().head, head);
}

#[test]
fn slice_test() {
    let mut pool = packet_pool("slice_test");
    let headroom = 100;
    let mut pkt = pool.pkt(headroom).unwrap();
    let bytes = vec![0 as u8; 2 * PARTICLE_SZ];
    assert!(pkt.append(&mut *pool, &bytes[0..]));
    let slices = pkt.slices();
    assert_eq!(slices.len(), 3);
    let (s, l) = slices[0];
    let p = pkt.particle.as_ref().unwrap();
    assert_eq!(s, &p.raw.as_ref().unwrap()[headroom..p.tail]);
    assert_eq!(l, PARTICLE_SZ - headroom);
    let (s, l) = slices[1];
    let p = p.next.as_ref().unwrap();
    assert_eq!(s, &p.raw.as_ref().unwrap()[0..p.tail]);
    assert_eq!(l, PARTICLE_SZ);
    let (s, l) = slices[2];
    let p = p.next.as_ref().unwrap();
    assert_eq!(s, &p.raw.as_ref().unwrap()[0..p.tail]);
    assert_eq!(l, headroom);
}

#[test]
fn move_head_test() {
    let mut pool = packet_pool("move_head_test");
    // One particle test
    let headroom = 100;
    let available = PARTICLE_SZ - headroom;
    let mut pkt = pool.pkt(headroom).unwrap();
    let bytes = vec![0 as u8; available];
    assert!(pkt.append(&mut *pool, &bytes[0..]));
    assert_eq!(pkt.len(), available);
    // Go front 10
    assert_eq!(pkt.move_head(10), 10);
    assert_eq!(pkt.len(), available - 10);
    check_first_part(&pkt, headroom + 10);
    // Come back 10
    assert_eq!(pkt.move_head(-10), -10);
    assert_eq!(pkt.len(), available);
    check_first_part(&pkt, headroom);
    // Try to go front available+1, it shud fail
    assert_eq!(pkt.move_head((available + 1) as isize), 0);
    assert_eq!(pkt.len(), available);
    // Try to go front available
    assert_eq!(pkt.move_head(available as isize), available as isize);
    check_first_part(&pkt, PARTICLE_SZ);
    assert_eq!(pkt.len(), 0);
    // Try to go back to the beginning of buffer - 1, it shud fail
    let l = -1 - PARTICLE_SZ as isize;
    assert_eq!(pkt.move_head(l), 0);
    check_first_part(&pkt, PARTICLE_SZ);
    assert_eq!(pkt.len(), 0);
    // Try to go back to the beginning of buffer
    let l = 0 - PARTICLE_SZ as isize;
    assert_eq!(pkt.move_head(l), l);
    check_first_part(&pkt, 0);
    assert_eq!(pkt.len(), PARTICLE_SZ);
}

#[test]
fn l2_test() {
    let mut pool = packet_pool("l2_test");
    let mac: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6];
    // One particle test
    let headroom = 100;
    let mut pkt = pool.pkt(headroom).unwrap();
    assert!(pkt.append(&mut *pool, &mac[0..]));
    assert_eq!(pkt.pull_l2(mac.len()), mac.len());
    let (l2, l2len) = pkt.get_l2();
    assert_eq!(l2len, mac.len());
    assert_eq!(mac.iter().zip(l2).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), 0);
    assert!(pkt.push_l2(&mut *pool, &mac));
    let (l2, l2len) = pkt.get_l2();
    assert_eq!(l2len, mac.len());
    assert_eq!(mac.iter().zip(l2).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), mac.len());

    let mut pkt = pool.pkt(headroom).unwrap();
    assert!(pkt.append(&mut *pool, &mac[0..]));
    assert!(pkt.set_l2(mac.len()));
    let (l2, l2len) = pkt.get_l2();
    assert_eq!(l2len, mac.len());
    assert_eq!(mac.iter().zip(l2).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), mac.len());
}

#[test]
fn l3_test() {
    let mut pool = packet_pool("l3_test");
    let ip: Vec<u8> = vec![1, 2, 3, 4, 1, 2, 3, 4];
    // One particle test
    let headroom = 100;
    let mut pkt = pool.pkt(headroom).unwrap();
    assert!(pkt.append(&mut *pool, &ip[0..]));
    assert_eq!(pkt.pull_l3(ip.len()), ip.len());
    let (l3, l3len) = pkt.get_l3();
    assert_eq!(l3len, ip.len());
    assert_eq!(ip.iter().zip(l3).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), 0);
    assert!(pkt.push_l3(&mut *pool, &ip));
    let (l3, l3len) = pkt.get_l3();
    assert_eq!(l3len, ip.len());
    assert_eq!(ip.iter().zip(l3).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), ip.len());
    assert!(pkt.set_l3(ip.len()));
    let (l3, l3len) = pkt.get_l3();
    assert_eq!(l3len, ip.len());
    assert_eq!(ip.iter().zip(l3).all(|(a, b)| a == b), true);
    assert_eq!(pkt.len(), ip.len());
    assert!(!pkt.set_l3(ip.len() + 1));
}
