use super::*;
use flavors::{Counter, CounterArray, CounterType, PktsBytes};

#[test]
fn basic_test() {
    let mut counters = Counters::new("basic_test").unwrap();

    let mut vec = vec![];
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = Counter::new(&mut counters, "test", CounterType::Error, &name);
        c.add(123_456 + i);
        vec.push(c);
    }

    let counters_ro = CountersRO::new("basic_test").unwrap();
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        assert_eq!(c.read(0), 123_456 + i);
    }

    while let Some(v) = vec.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn free_test() {
    let mut counters = Counters::new("free_test").unwrap();

    let mut vec1 = vec![];
    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = Counter::new(&mut counters, "test", CounterType::Error, &name);
        c.add(123_456 + i);
        vec1.push(c);
    }
    let mut vec2 = vec![];
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = Counter::new(&mut counters, "test", CounterType::Error, &name);
        c.add(123_456 + i);
        vec2.push(c);
    }

    while let Some(v) = vec1.pop() {
        v.free(&mut counters);
    }

    let counters_ro = CountersRO::new("free_test").unwrap();

    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name);
        assert!(c.is_none());
    }
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        assert_eq!(c.read(0), 123_456 + i);
    }

    while let Some(v) = vec2.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn basic_pkts_bytes_test() {
    let mut counters = Counters::new("basic_pkts_bytes_test").unwrap();

    let mut vec = vec![];
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = PktsBytes::new(&mut counters, "test", CounterType::Error, &name);
        c.add(i, 123_456 + i);
        vec.push(c);
    }

    let counters_ro = CountersRO::new("basic_pkts_bytes_test").unwrap();
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        let (pkts, bytes) = (c.read(0), c.read(1));
        assert_eq!(pkts, i);
        assert_eq!(bytes, 123_456 + i);
    }

    while let Some(v) = vec.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn free_pkts_bytes_test() {
    let mut counters = Counters::new("free_pkts_bytes_test").unwrap();

    let mut vec1 = vec![];
    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = PktsBytes::new(&mut counters, "test", CounterType::Error, &name);
        c.add(i, 123_456 + i);
        vec1.push(c);
    }
    let mut vec2 = vec![];
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = PktsBytes::new(&mut counters, "test", CounterType::Error, &name);
        c.add(i, 123_456 + i);
        vec2.push(c);
    }

    while let Some(v) = vec1.pop() {
        v.free(&mut counters);
    }

    let counters_ro = CountersRO::new("free_pkts_bytes_test").unwrap();

    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name);
        assert!(c.is_none());
    }
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        let (pkts, bytes) = (c.read(0), c.read(1));
        assert_eq!(pkts, i);
        assert_eq!(bytes, 123_456 + i);
    }

    while let Some(v) = vec2.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn basic_vec_test() {
    let mut counters = Counters::new("basic_vec_test").unwrap();

    let nvec = VEC.binmax as usize;
    let mut vec = vec![];
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = CounterArray::new(&mut counters, "test", CounterType::Error, &name, nvec);
        for v in 0..nvec {
            c.add(v, (123_456 + v + i) as u64);
        }
        vec.push(c);
    }

    let counters_ro = CountersRO::new("basic_vec_test").unwrap();
    for i in 0..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        for v in 0..nvec {
            assert_eq!(c.read(v), (123_456 + v + i) as u64);
        }
    }

    while let Some(v) = vec.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn free_vec_test() {
    let mut counters = Counters::new("free_vec_test").unwrap();

    let nvec = VEC.binmax as usize;
    let mut vec1 = vec![];
    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = CounterArray::new(&mut counters, "test", CounterType::Error, &name, nvec);
        for v in 0..nvec {
            c.add(v, (123_456 + v + i) as u64);
        }
        vec1.push(c);
    }

    let mut vec2 = vec![];
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let mut c = CounterArray::new(&mut counters, "test", CounterType::Error, &name, nvec);
        for v in 0..nvec {
            c.add(v, (123_456 + v + i) as u64);
        }
        vec2.push(c);
    }

    while let Some(v) = vec1.pop() {
        v.free(&mut counters);
    }

    let counters_ro = CountersRO::new("free_vec_test").unwrap();

    for i in 0..50 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name);
        assert!(c.is_none());
    }
    for i in 50..100 {
        let mut name = "counter".to_string();
        name.push_str(&i.to_string());
        let c = CounterRO::search(&counters_ro, "test", CounterType::Error, &name).unwrap();
        for v in 0..nvec {
            assert_eq!(c.read(v), (123_456 + v + i) as u64);
        }
    }

    while let Some(v) = vec2.pop() {
        v.free(&mut counters);
    }
}

#[test]
fn combined_test() {
    let mut counters = Counters::new("combined_test").unwrap();

    let mut basic = Counter::new(&mut counters, "test", CounterType::Error, "basic");
    basic.add(123_456);

    let mut pb = PktsBytes::new(&mut counters, "test", CounterType::Error, "pktsbytes");
    pb.add(100, 123_456);

    let nvec = VEC.binmax as usize;
    let mut array = CounterArray::new(&mut counters, "test", CounterType::Error, "array", nvec);
    for v in 0..nvec {
        array.add(v, 123_456 + v as u64);
    }

    let counters_ro = CountersRO::new("combined_test").unwrap();

    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "basic").unwrap();
    assert_eq!(c.read(0), 123_456);

    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "pktsbytes").unwrap();
    let (pkts, bytes) = (c.read(0), c.read(1));
    assert_eq!(pkts, 100);
    assert_eq!(bytes, 123_456);

    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "array").unwrap();
    for v in 0..nvec {
        assert_eq!(c.read(v), (123_456 + v) as u64);
    }

    drop(counters_ro);
    array.free(&mut counters);

    let counters_ro = CountersRO::new("combined_test").unwrap();
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "array");
    assert!(c.is_none());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "pktsbytes");
    assert!(c.is_some());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "basic");
    assert!(c.is_some());

    drop(counters_ro);
    pb.free(&mut counters);

    let counters_ro = CountersRO::new("combined_test").unwrap();
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "array");
    assert!(c.is_none());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "pktsbytes");
    assert!(c.is_none());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "basic");
    assert!(c.is_some());

    drop(counters_ro);
    basic.free(&mut counters);

    let counters_ro = CountersRO::new("combined_test").unwrap();
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "array");
    assert!(c.is_none());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "pktsbytes");
    assert!(c.is_none());
    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "basic");
    assert!(c.is_none());
}

#[test]
fn max_limits_test() {
    let mut counters = Counters::new("max_limits_test").unwrap();

    let name = "A_VERY_LONG_NAME_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let _basic = Counter::new(&mut counters, "test", CounterType::Error, name);

    let nvec = VEC.binmax as usize;
    let _array = CounterArray::new(&mut counters, "test", CounterType::Error, "array", nvec + 1);

    let counters_ro = CountersRO::new("max_limits_test").unwrap();

    let _c = CounterRO::search(&counters_ro, "test", CounterType::Error, name).unwrap();

    let c = CounterRO::search(&counters_ro, "test", CounterType::Error, "array").unwrap();
    c.read(nvec - 1);
}
