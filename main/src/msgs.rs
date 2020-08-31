use super::*;
use names::l2_eth_decap;
use names::l2_eth_encap;

pub fn ctrl2fwd_messages(
    thread: usize,
    epoll: &mut Epoll,
    receiver: &Receiver<R2Msg>,
    g: &mut Graph<R2Msg>,
) {
    while let Ok(msg) = receiver.try_recv() {
        match msg {
            R2Msg::GnodeAdd(gnode_add) => {
                g.add(gnode_add.node, gnode_add.init);
                g.finalize();
            }
            R2Msg::EpollAdd(epoll_add) => {
                if epoll_add.thread == thread {
                    if let Some(fd) = epoll_add.fd {
                        epoll.add(fd, EPOLLIN);
                    }
                }
            }
            R2Msg::IPv4TableAdd(_) => {
                g.control_msg(names::L3_IPV4_FWD, msg);
            }
            R2Msg::ModifyInterface(mod_intf) => {
                g.control_msg(
                    &l2_eth_decap(mod_intf.intf.ifindex),
                    R2Msg::ModifyInterface(mod_intf.clone()),
                );
                g.control_msg(
                    &l2_eth_encap(mod_intf.intf.ifindex),
                    R2Msg::ModifyInterface(mod_intf.clone()),
                );
                g.control_msg(
                    &rx_tx(mod_intf.intf.ifindex),
                    R2Msg::ModifyInterface(mod_intf),
                );
            }
            R2Msg::EthMacAdd(mac_add) => {
                g.control_msg(
                    &l2_eth_decap(mac_add.ifindex),
                    R2Msg::EthMacAdd(mac_add.clone()),
                );
                g.control_msg(&l2_eth_encap(mac_add.ifindex), R2Msg::EthMacAdd(mac_add));
            }
            R2Msg::ClassAdd(class) => {
                g.control_msg(&rx_tx(class.ifindex), R2Msg::ClassAdd(class));
            }
        }
    }
}

pub fn fwd2ctrl_messages(r2: Arc<Mutex<R2>>, receiver: Receiver<R2Msg>) {
    while let Ok(msg) = receiver.recv() {
        match msg {
            R2Msg::EthMacAdd(mac_add) => {
                let mut r2 = r2.lock().unwrap();
                r2.broadcast(R2Msg::EthMacAdd(mac_add));
            }
            _ => panic!("Unexpected message"),
        }
    }
}
