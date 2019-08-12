use super::*;

#[test]
fn log_basic() {
    let logger = Logger::new("r2_log_test_basic", 32, 4).unwrap();
    log!(logger, "Foo bar", 1 as u8, 2 as u16, 3 as u32, 4 as u64, 5 as u8, 6 as u32, 7 as u8);

    let mut base = logger.base;
    unsafe {
        assert_eq!(*(base as *const u32), 1); // index
        base += 4;
        assert!(*(base as *const u64) != 0); // rdtsc
        base += 8;
        assert_eq!(*(base as *const u8), 1);
        base += 1;
        assert_eq!(*(base as *const u16), 2);
        base += 2;
        assert_eq!(*(base as *const u32), 3);
        base += 4;
        assert_eq!(*(base as *const u64), 4);
        base += 8;
        assert_eq!(*(base as *const u8), 5);
        base += 1;
        assert_eq!(*(base as *const u32), 6);
        base += 4;
        assert_eq!(*(base as *const u8), 0); // 7 did not get written
    }

    log!(
        logger,
        "Foo bar again",
        1 as u8,
        2 as u16,
        3 as u32,
        4 as u64,
        5 as u8,
        6 as u32,
        7 as u8
    );
    unsafe {
        assert_eq!(*(base as *const u32), 2); // index
        base += 4;
        assert!(*(base as *const u64) != 0); // rdtsc
        base += 8;
        assert_eq!(*(base as *const u8), 1);
        base += 1;
        assert_eq!(*(base as *const u16), 2);
        base += 2;
        assert_eq!(*(base as *const u32), 3);
        base += 4;
        assert_eq!(*(base as *const u64), 4);
        base += 8;
        assert_eq!(*(base as *const u8), 5);
        base += 1;
        assert_eq!(*(base as *const u32), 6);
        base += 4;
        assert_eq!(*(base as *const u8), 0); // 7 did not get written
    }
    log!(
        logger,
        "Foo bar yet again",
        1 as u8,
        2 as u16,
        3 as u32,
        4 as u64,
        5 as u8,
        6 as u32,
        7 as u8
    );
    log!(
        logger,
        "Foo bar, cant tolerate it %d %f !!",
        1 as u8,
        2 as u16,
        3 as u32,
        4 as u64,
        5 as u8,
        6 as u32,
        7 as u8
    );

    log!(
        logger,
        "Test wrap around",
        7 as u8,
        6 as u16,
        5 as u32,
        4 as u64,
        3 as u8,
        2 as u32,
        1 as u8
    );
    // wrap around to the first entry
    let mut base = logger.base;
    unsafe {
        assert_eq!(*(base as *const u32), 5); // index
        base += 4;
        assert!(*(base as *const u64) != 0); // rdtsc
        base += 8;
        assert_eq!(*(base as *const u8), 7);
        base += 1;
        assert_eq!(*(base as *const u16), 6);
        base += 2;
        assert_eq!(*(base as *const u32), 5);
        base += 4;
        assert_eq!(*(base as *const u64), 4);
        base += 8;
        assert_eq!(*(base as *const u8), 3);
        base += 1;
        assert_eq!(*(base as *const u32), 2);
        base += 4;
        assert_eq!(*(base as *const u32), 2); // This is the index number 2 (second entry)
    }

    let file = File::create("/tmp/r2_logs.json").unwrap();
    logger.serialize(file).unwrap();
}
