use super::*;

#[test]
fn test_init() {
    dpdk_init(128, 1);
    dpdk_buffer_init(10, 2048);
}
