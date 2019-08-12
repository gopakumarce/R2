use libc;
use packet::BoxPkt;
use std::ffi::CString;
use std::mem;
use std::ptr;

pub struct RawSock {
    fd: i32,
}

impl RawSock {
    const PACKET_AUXDATA: i32 = 8;
    const ETH_P_ALL_BE: u16 = 0x0300; // htons(libc::ETH_P_ALL);

    fn sockaddr_ll_new(index: u32) -> libc::sockaddr_ll {
        libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as libc::c_ushort,
            sll_ifindex: index as libc::c_int,
            sll_protocol: RawSock::ETH_P_ALL_BE,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 0,
            sll_addr: [0; 8],
        }
    }

    pub fn fd(&self) -> i32 {
        self.fd
    }

    pub fn new(interface: &str, non_blocking: bool) -> Result<RawSock, i32> {
        unsafe {
            let fd = libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW,
                RawSock::ETH_P_ALL_BE as i32,
            );
            if fd < 0 {
                return Err(*(libc::__errno_location()));
            }
            let c_str = CString::new(interface).unwrap();
            let ifname = c_str.as_ptr() as *const i8;
            let index = libc::if_nametoindex(ifname);
            if index == 0 {
                return Err(*(libc::__errno_location()));
            }
            let mut sa = RawSock::sockaddr_ll_new(index);
            let ptr = &mut sa as *mut libc::sockaddr_ll as *mut libc::sockaddr;
            let sz = mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t;
            let ret = libc::bind(fd, ptr, sz);
            if ret < 0 {
                return Err(*(libc::__errno_location()));
            }
            let val: Vec<libc::c_int> = vec![1];
            let ret = libc::setsockopt(
                fd,
                libc::SOL_PACKET,
                RawSock::PACKET_AUXDATA,
                val.as_ptr() as *const libc::c_void,
                mem::size_of::<libc::c_int>() as u32,
            );
            if ret < 0 {
                return Err(*(libc::__errno_location()));
            }
            if non_blocking {
                let mut f = libc::fcntl(fd, libc::F_GETFL);
                if f == -1 {
                    return Err(*(libc::__errno_location()));
                }
                f |= libc::O_NONBLOCK;
                if libc::fcntl(fd, libc::F_SETFL, f) < 0 {
                    return Err(*(libc::__errno_location()));
                }
            }
            Ok(RawSock { fd })
        }
    }

    pub fn recvmsg(&self, pkt: &mut BoxPkt) {
        unsafe {
            let buf = pkt.data_raw();
            let mut iov: libc::iovec = mem::MaybeUninit::uninit().assume_init();
            let head = buf.as_ptr() as u64 + pkt.headroom() as u64;
            iov.iov_base = head as *mut libc::c_void;
            iov.iov_len = buf.len() - pkt.headroom();
            let mut cmsg: [u8; 32] = mem::MaybeUninit::uninit().assume_init();
            let mut mhdr: libc::msghdr = mem::MaybeUninit::uninit().assume_init();
            mhdr.msg_name = ptr::null_mut();
            mhdr.msg_namelen = 0 as libc::socklen_t;
            mhdr.msg_iov = &mut iov;
            mhdr.msg_iovlen = 1;
            mhdr.msg_control = cmsg.as_mut_ptr() as *mut libc::c_void;
            mhdr.msg_controllen = cmsg.len();
            mhdr.msg_flags = 0;
            let rv = libc::recvmsg(self.fd, &mut mhdr, libc::MSG_TRUNC);
            if rv > 0 {
                assert_eq!(pkt.move_tail(rv), rv);
            }
        }
    }

    pub fn sendmsg(&self, pkt: &BoxPkt) -> usize {
        unsafe {
            let slices = pkt.slices();
            let iov: libc::iovec = mem::MaybeUninit::uninit().assume_init();
            let mut iovec: Vec<libc::iovec> = vec![iov; slices.len()];
            for i in 0..slices.len() {
                iovec[i].iov_base = slices[i].0.as_ptr() as *mut libc::c_void;
                iovec[i].iov_len = slices[i].1;
            }
            let mut mhdr: libc::msghdr = mem::MaybeUninit::uninit().assume_init();
            mhdr.msg_name = ptr::null_mut();
            mhdr.msg_namelen = 0 as libc::socklen_t;
            mhdr.msg_iov = iovec.as_mut_ptr();
            mhdr.msg_iovlen = iovec.len();
            mhdr.msg_control = ptr::null_mut();
            mhdr.msg_controllen = 0;
            mhdr.msg_flags = 0;
            let rv = libc::sendmsg(self.fd, &mhdr, 0);
            if rv < 0 {
                return 0;
            }
            rv as usize
        }
    }
}

#[cfg(test)]
mod test;
