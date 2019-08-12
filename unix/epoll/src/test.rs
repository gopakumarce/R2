use super::*;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const HELLO_WORLD: &str = "Hello World";

fn pipe_read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        return libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
    }
}

fn pipe_write(fd: i32, s: &str) {
    unsafe {
        libc::write(fd, s.as_ptr() as *const libc::c_void, s.len());
    }
}

struct EpollTest {
    fd: i32,
    nevents: Arc<AtomicUsize>,
}

impl EpollClient for EpollTest {
    fn event(&mut self, fd: i32, _event: u32) {
        assert_eq!(fd, self.fd);
        self.nevents.fetch_add(1, Ordering::Relaxed);
        let mut buf: Vec<u8> = vec![0; HELLO_WORLD.len()];
        let sz = pipe_read(fd, &mut buf[0..]);
        assert_eq!(sz as usize, buf.len());
        let str = str::from_utf8(&buf[0..]).unwrap();
        assert_eq!(HELLO_WORLD, str);
    }
}

#[test]
fn epoll_test() {
    let mut pipefd = [-1, -1];
    unsafe {
        libc::pipe2(pipefd.as_mut_ptr(), libc::O_NONBLOCK);
        assert!(pipefd[0] > 0);
        assert!(pipefd[1] > 0);
    }
    let edata = Box::new(EpollTest {
        fd: pipefd[0],
        nevents: Arc::new(AtomicUsize::new(0)),
    });
    let nevents = edata.nevents.clone();
    let efd = Arc::new(Efd::new(0).unwrap());
    let mut epoll = match Epoll::new(efd, 4, -1, edata) {
        Ok(e) => e,
        Err(errno) => panic!("epoll create failed, errno {}", errno),
    };
    let ret = epoll.add(pipefd[0], EPOLLIN);
    assert_eq!(ret, 0);

    let wait = Arc::new(AtomicUsize::new(0));
    let done = wait.clone();
    let tname = "epoll".to_string();
    let handler = thread::Builder::new().name(tname).spawn(move || loop {
        epoll.wait();
        if nevents.load(Ordering::Relaxed) == 4 {
            epoll.del(pipefd[0]);
            done.fetch_add(1, Ordering::Relaxed);
            break;
        }
    });

    while wait.load(Ordering::Relaxed) == 0 {
        pipe_write(pipefd[1], HELLO_WORLD);
    }
    handler.unwrap().join().unwrap();
}
