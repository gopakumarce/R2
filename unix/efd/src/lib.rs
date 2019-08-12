use libc;

pub struct Efd {
    pub fd: i32,
}

impl Efd {
    pub fn new(flags: i32) -> Result<Efd, i32> {
        unsafe {
            let fd = libc::eventfd(0, flags);
            if fd <= 0 {
                return Err(*(libc::__errno_location()));
            }
            Ok(Efd { fd })
        }
    }

    pub fn write(&self, val: u64) {
        unsafe {
            let data = [val; 1];
            libc::write(self.fd, data.as_ptr() as *const libc::c_void, 8);
        }
    }

    pub fn read(&self) -> u64 {
        unsafe {
            let data: [u64; 1] = [0; 1];
            libc::read(self.fd, data.as_ptr() as *mut libc::c_void, 8);
            data[0]
        }
    }
}
