use super::*;

#[test]
fn test_init() {
    dpdk_init(128, 1);
    dpdk_buffer_init(1024*1024, 0, 2048);
}
