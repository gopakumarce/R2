use std::time::{SystemTime, UNIX_EPOCH};

pub const API_SVR: &str = "127.0.0.1:5555";
pub const LOG_APIS: &str = "log";
pub const INTF_APIS: &str = "interface";
pub const ROUTE_APIS: &str = "route";
pub const R2CNT_SHM: &str = "r2cnt";
pub const R2LOG_SHM: &str = "r2log";

#[macro_export]
macro_rules! KB {
    ($name:expr) => {
        $name * 1024
    };
}

#[macro_export]
macro_rules! MB {
    ($name:expr) => {
        $name * 1024 * 1024
    };
}

pub fn pow2_u32(val: u32) -> u32 {
    let mut v = val - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v += 1;
    v
}

pub fn time_nsecs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

pub fn time_usecs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

pub fn time_msecs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
