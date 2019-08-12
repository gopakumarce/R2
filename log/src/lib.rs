use common::time_nsecs;
use libc;
use shm;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::Write;
use std::mem::{size_of, size_of_val};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;

const MAX_DEBUGS: usize = 16;

// The assumption here is that a log entry consists of at most 16 different variables.
// The variables are logged as just a sequence of bytes, so each of the 16 entries here
// are recording the number of bytes consumed by each of those variables.
#[macro_export]
macro_rules! sizes_16 {
    () => {{
        [
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
            std::sync::atomic::AtomicU8::new(0),
        ]
    }};
}

// This macro logs one variable in the log entry by calling logger.copy() and then recursively
// calls itself to log the next variable in the log entry. Note that the recursive calls are
// all macro calls, so all the recursive calls expands out to logger.copy() one after the other
#[macro_export]
macro_rules! log_helper {
    ($logger:expr, $base:expr, $entry:expr, $bytes:expr, $index:expr, $last:expr, $e:expr) => {{
        ($logger.copy(&$e, $base, $entry, $bytes, $index, $last), $index + 1)
    }};

    ($logger:expr, $base:expr, $entry:expr, $bytes:expr, $index:expr, $last:expr, $e:expr, $($es:expr),+) => {{
        let (b, i) = $crate::log_helper! {$logger, $base, $entry, $bytes, $index, 0, $e};
        $crate::log_helper! {$logger, $base, $entry, b, i, 1, $( $es ),+}
    }};
}

// This is the macro that modules call to log data. This first creates a static entry that
// stores the format string and information about all the variables being logged and their
// sizes etc.. and then proceeds to log each of the variables. Obviously the static entry
// init is done just once. Also if the logger is stopped, nothing is logged.
#[macro_export]
macro_rules! log {
    ($logger:expr, $str:literal, $($es:expr),+) => {{
        static ENTRY: $crate::Entry = $crate::Entry {
                                index: std::sync::atomic::AtomicUsize::new(0),
                                format: $str,
                                inited: std::sync::atomic::AtomicUsize::new(0),
                                sizes: $crate::sizes_16!(),
                              };
        if $logger.stopped() == 0 {
            let base = $logger.get_entry(&ENTRY);
            $crate::log_helper! {$logger, base, &ENTRY, 0, 0, 1, $( $es ),+ };
        }
    }};
}

/// The logger is basically an area of memory divided into fixed size elements. Each log
/// entry consumes a fixed size memory regardless of whether it needs it or not. And the
/// logs can wrap around and go back to the beginning of the buffer. The Logger as of today
/// is shared with the control plane thread since the log dump is done by control plane
/// thread. So to be able to mutate fields in the shared object, we use the atomic vars.
/// We use Relaxed atomic ops, so there should be no side effects of using atomics. And
/// the control plane thread will ensure the logger is stopped before dumping the logs, so
/// Relaxed ops works just fine
pub struct Logger {
    /// Name of the logger
    name: String,
    /// The file descriptor if the logger memory can be backed up by shared memory/file etc..
    fd: i32,
    /// The start address of the logger memory
    base: u64,
    /// The (fixed) size of each log entry
    esz: usize,
    /// The size of the entire logger memory area
    emax: usize,
    /// Stop logging if value is non zero
    stop: AtomicUsize,
    /// The next entry in the logger that can be used to log data, this can wrap around
    enext: AtomicUsize,
    /// Give each logging point in the code a unique index, to store the log Entry meta data
    index: AtomicUsize,
    /// The index to log Entry meta data mapping
    hash: Mutex<HashMap<usize, &'static Entry>>,
}

/// Meta data for each logging point in the code. This structure is defined as STATIC by
/// the log! macro and hence the only way to mutate its fields like index, is by having it
/// atomic. We use Relaxed atomic ops and hence it should not have any side effects
/// of using atomics
pub struct Entry {
    /// A unique index
    pub index: AtomicUsize,
    /// The format string
    pub format: &'static str,
    /// Non zero if this log entry has been logged at least once
    pub inited: AtomicUsize,
    /// Sizes of the variables being logged at this logging point
    pub sizes: [AtomicU8; MAX_DEBUGS],
}

// We dint want to include the reasonably large serdes module just for a simple log entry
// translation to json. I dont anticipate this getting any more complex, if it does we can
// evaluate serdes
struct Serial {
    index: u32,
    format: &'static str,
    timestamp: u64,
    vals: Vec<u64>,
}

impl Serial {
    fn to_json(&self) -> String {
        let mut s = format!(
            "{{ \
             \"index\": {}, \
             \"format\": \"{}\", \
             \"timestamp\": {}, \
             \"vals\": [",
            self.index, self.format, self.timestamp
        );
        let mut first = true;
        for v in self.vals.iter() {
            if !first {
                s.push_str(",");
            }
            s.push_str(&format!("{}", v));
            first = false;
        }
        s.push_str("]}");
        s
    }
}

impl Logger {
    /// Create a new loger, specify the fixed log entry size and the total log memory size
    pub fn new(name: &str, esz: usize, emax: usize) -> Result<Logger, i32> {
        let shm_sz = esz * emax;
        let (fd, base) = shm::shm_open_rw(name, shm_sz);
        if fd < 0 {
            unsafe {
                return Err(*(libc::__errno_location()));
            }
        }

        Ok(Logger {
            name: name.to_string(),
            fd,
            base,
            esz,
            emax,
            stop: AtomicUsize::new(0),
            enext: AtomicUsize::new(0),
            index: AtomicUsize::new(1),
            hash: Mutex::new(HashMap::new()),
        })
    }

    pub fn stopped(&self) -> usize {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.stop.store(1, Ordering::Relaxed);
    }

    /// Get a log entry, wrap around if needed
    pub fn get_entry(&self, entry: &'static Entry) -> u64 {
        // Fill up the index of the entry and the timestamp
        let cur = self.enext.load(Ordering::Relaxed);
        self.enext.store(cur + 1, Ordering::Relaxed);
        let cur = cur % self.emax;
        let base = self.base + (self.esz * cur) as u64;
        unsafe {
            *(base as *mut u32) = self.entry_index(entry);
        }
        unsafe {
            *((base + size_of::<u32>() as u64) as *mut u64) = time_nsecs();
        }
        base
    }

    // A new logging point is getting logged, get a unique index for it
    fn entry_index(&self, entry: &'static Entry) -> u32 {
        let ret;
        let index = entry.index.load(Ordering::Relaxed);
        if index != 0 {
            ret = index
        } else {
            let mut hash = self.hash.lock().unwrap();
            // Some other thread might have allocated index already
            let index = entry.index.load(Ordering::Relaxed);
            if index != 0 {
                ret = index
            } else {
                let val = self.index.load(Ordering::Relaxed);
                self.index.store(val + 1, Ordering::Relaxed);
                entry.index.store(val, Ordering::Relaxed);
                hash.insert(val, entry);
                ret = val
            }
            drop(hash);
        }
        ret as u32
    }

    /// Copy a variable to an offset into the log entry
    pub fn copy<T>(
        &self,
        val: &T,
        dst: u64,
        entry: &Entry,
        bytes: u64,
        index: usize,
        last: usize,
    ) -> u64 {
        let overheads = (size_of::<u32>() + size_of::<u64>()) as u64;
        let sz = size_of_val(val);
        // Store the size of this variable in the static log entry. We cant initialize
        // entry.sizes in the log! macro, that needs impl {} const fn support to be available in rust
        if entry.inited.load(Ordering::Relaxed) == 0 {
            if index < MAX_DEBUGS {
                entry.sizes[index].store(sz as u8, Ordering::Relaxed);
            }
            if last == 1 {
                entry.inited.store(1, Ordering::Relaxed);
            }
        }
        let d = dst + overheads + bytes;
        if (d + sz as u64) <= dst + self.esz as u64 {
            unsafe {
                let src = val as *const T as *const core::ffi::c_void;
                libc::memcpy(d as *mut libc::c_void, src, sz);
            }
        }
        bytes + sz as u64
    }

    /// Dump the logs to a file, formatted as json
    pub fn serialize(&self, mut file: File) -> std::io::Result<()> {
        file.write_all(b"{\"logs\":[\n")?;
        let start = self.enext.load(Ordering::Relaxed) % self.emax;
        let mut e = start;
        loop {
            let mut base = self.base + (e * self.esz) as u64;
            let end = base + self.esz as u64;
            let index = unsafe { *(base as *mut u32) };
            let prev = if index != 0 {
                base += 4;
                let timestamp = unsafe { *(base as *mut u64) };
                base += 8;

                let hash = self.hash.lock().unwrap();
                let entry = *hash.get(&(index as usize)).unwrap();
                drop(hash);
                let mut vals = Vec::new();
                for i in 0..MAX_DEBUGS {
                    let sz = entry.sizes[i].load(Ordering::Relaxed);
                    if sz == 0 {
                        break;
                    }
                    if base + sz as u64 > end {
                        break;
                    }
                    unsafe {
                        vals.push(match sz {
                            1 => *(base as *mut u8) as u64,
                            2 => *(base as *mut u16) as u64,
                            4 => *(base as *mut u32) as u64,
                            8 => *(base as *mut u64) as u64,
                            _ => panic!("Bad entry size {}", sz),
                        })
                    }
                    base += sz as u64;
                }
                let serial = Serial {
                    index,
                    format: entry.format,
                    timestamp,
                    vals,
                };
                let serialized = serial.to_json();
                file.write_all(serialized.as_bytes())?;
                true
            } else {
                false
            };
            e = (e + 1) % self.emax;
            if e == start {
                break;
            }
            if prev {
                file.write_all(b",\n")?;
            }
        }

        file.write_all(b"\n]}")?;
        Ok(())
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        shm::shm_close(self.fd);
        shm::shm_unlink(&self.name);
    }
}

#[cfg(test)]
mod test;
