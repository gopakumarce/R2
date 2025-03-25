use std::ffi::CString;
use std::ptr;

pub fn shm_open_rw(name: &str, size: usize) -> (i32, u64) {
    unsafe {
        let c_name = CString::new(name).unwrap();
        let c_name = c_name.as_ptr();
        let flags = libc::O_CREAT | libc::O_TRUNC | libc::O_RDWR;
        let fd = libc::shm_open(c_name, flags, libc::S_IRUSR | libc::S_IWUSR);
        if fd == -1 {
            return (*(libc::__errno_location()), 0);
        }
        if libc::ftruncate(fd, size as i64) == -1 {
            return (*(libc::__errno_location()), 0);
        }
        let flags = libc::PROT_READ | libc::PROT_WRITE;
        let base = libc::mmap(ptr::null_mut(), size, flags, libc::MAP_SHARED, fd, 0);
        if base.is_null() {
            libc::close(fd);
            return (*(libc::__errno_location()), 0);
        }

        (fd, base as u64)
    }
}

pub fn shm_open_ro(name: &str, size: usize) -> (i32, u64) {
    unsafe {
        let c_name = CString::new(name).unwrap();
        let c_name = c_name.as_ptr();
        let flags = libc::O_RDONLY;
        let fd = libc::shm_open(c_name, flags, libc::S_IRUSR | libc::S_IWUSR);
        if fd == -1 {
            return (*(libc::__errno_location()), 0);
        }
        let flags = libc::PROT_READ;
        let base = libc::mmap(ptr::null_mut(), size, flags, libc::MAP_SHARED, fd, 0);
        if base.is_null() {
            libc::close(fd);
            return (*(libc::__errno_location()), 0);
        }

        (fd, base as u64)
    }
}

pub fn shm_close(fd: i32) {
    unsafe {
        libc::close(fd);
    }
}

pub fn shm_unlink(name: &str) {
    let c_name = CString::new(name).unwrap();
    let c_name = c_name.as_ptr();
    unsafe {
        libc::shm_unlink(c_name);
    }
}
