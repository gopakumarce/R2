use efd::Efd;
use libc;
use std::sync::Arc;

pub const EPOLLIN: u32 = libc::EPOLLIN as u32;
pub const EPOLLOUT: u32 = libc::EPOLLOUT as u32;
pub const EPOLLHUP: u32 = libc::EPOLLHUP as u32;
pub const EPOLLERR: u32 = libc::EPOLLERR as u32;

pub trait EpollClient: Send {
    fn event(&mut self, fd: i32, event: u32);
}

pub struct Epoll {
    epoll: i32,
    nfds: i32,
    timeout: i32,
    wakeup: Arc<Efd>,
    client: Box<dyn EpollClient>,
    events: Vec<libc::epoll_event>,
}

impl Epoll {
    pub fn new(
        efd: Arc<Efd>,
        nfds: i32,
        timeout: i32,
        client: Box<dyn EpollClient>,
    ) -> Result<Epoll, i32> {
        let epoll: i32 = unsafe {
            let epoll = libc::epoll_create(nfds);
            if epoll < 0 {
                return Err(*(libc::__errno_location()));
            }
            epoll
        };
        let event = libc::epoll_event { events: 0, u64: 0 };
        let events: Vec<libc::epoll_event> = vec![event; nfds as usize];
        let epoll = Epoll {
            epoll,
            nfds,
            timeout,
            client,
            wakeup: efd,
            events,
        };
        epoll.add(epoll.wakeup.fd, EPOLLIN);
        Ok(epoll)
    }

    pub fn add(&self, fd: i32, flags: u32) -> i32 {
        unsafe {
            let mut f = libc::fcntl(fd, libc::F_GETFL);
            if f == -1 {
                let errno = *(libc::__errno_location());
                return -errno;
            }
            f |= libc::O_NONBLOCK;
            if libc::fcntl(fd, libc::F_SETFL, f) < 0 {
                let errno = *(libc::__errno_location());
                return -errno;
            }
            let mut event = libc::epoll_event {
                events: flags,
                u64: fd as u64,
            };
            let ret = libc::epoll_ctl(self.epoll, libc::EPOLL_CTL_ADD, fd, &mut event);
            if ret < 0 {
                let errno = *(libc::__errno_location());
                return -errno;
            }
        }
        0
    }

    pub fn del(&self, fd: i32) {
        unsafe {
            let mut event = libc::epoll_event { events: 0, u64: 0 };
            libc::epoll_ctl(self.epoll, libc::EPOLL_CTL_DEL, fd, &mut event);
        }
    }

    pub fn wait(&mut self) -> i32 {
        let ret = unsafe {
            let ret = libc::epoll_wait(
                self.epoll,
                self.events.as_mut_ptr(),
                self.nfds,
                self.timeout,
            );
            if ret == -1 {
                let errno = *(libc::__errno_location());
                if errno == libc::EINTR {
                    return 0;
                }
                return -errno;
            }
            ret
        };
        for e in self.events.iter().take(ret as usize) {
            let fd = e.u64 as i32;
            if fd == self.wakeup.fd {
                self.wakeup.read();
            }
            self.client.event(fd, e.events);
        }
        ret
    }
}

#[cfg(test)]
mod test;
