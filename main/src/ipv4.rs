use super::*;
use apis_route::{RouteErr, RouteSyncHandler};
use fwd::ip_mask_decode;
use fwd::ipv4::IPv4Table;
use fwd::{adj::Adjacency, ipv4::IPv4Leaf, ipv4::IPv4TableMsg, Fwd};
use l3_ipv4_fwd::IPv4Fwd;
use l3_ipv4_parse::IPv4Parse;
use perf::Perf;
use std::fs::File;
use std::io::prelude::*;
use std::net::Ipv4Addr;
use std::str::FromStr;

pub struct RouteApis {
    r2: Arc<Mutex<R2>>,
}

impl RouteApis {
    pub fn new(r2: Arc<Mutex<R2>>) -> RouteApis {
        RouteApis { r2 }
    }

    fn handle_show_all(&self, filename: String) -> thrift::Result<String> {
        let mut file = match File::create(&filename) {
            Err(why) => {
                return Err(RouteErr::new(format!(
                    "couldn't create {}: {}",
                    filename,
                    why.to_string()
                )))
                .map_err(From::from);
            }
            Ok(file) => file,
        };
        file_write(&mut file, "{\n\"table1\":[\n");
        let r2 = self.r2.lock().unwrap();
        let iter = r2.ipv4.table1.root.iter();
        let mut first = true;
        for (prefix, masklen, leaf) in iter {
            if !first {
                file_write(&mut file, ",\n");
            }
            first = false;
            route_json_dump(&mut file, &r2, prefix, masklen, leaf);
        }
        file_write(&mut file, "\n],\n");

        file_write(&mut file, "\"table2\":[\n");
        let iter = r2.ipv4.table2.root.iter();
        let mut first = true;
        for (prefix, masklen, leaf) in iter {
            if !first {
                file_write(&mut file, ",\n");
            }
            first = false;
            route_json_dump(&mut file, &r2, prefix, masklen, leaf);
        }
        file_write(&mut file, "\n]\n}\n");

        Ok("".to_string())
    }

    fn handle_show_one(&self, r2: &R2, table: &IPv4Table, addr: Ipv4Addr) -> String {
        if let Some((prefix, mask, leaf)) = table.root.longest_match(addr) {
            if let ipv4::Fwd::Adjacency(adj) = &leaf.next {
                let ifname = if let Some(name) = r2.ifd.get_name(adj.ifindex as usize) {
                    name
                } else {
                    "Unknown_ifindex"
                };
                let mut s = "Destination\t\tNextHop\t\tInterface\n".to_string();
                s.push_str(&format!(
                    "{}/{}\t\t{}\t\t{}[{}]\n",
                    prefix, mask, adj.nhop, ifname, adj.ifindex
                ));
                s
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        }
    }
}

// We keep two route tables - ie two trie bitmaps - mirror image of each other. One is
// actively used by forwarding threads, the other is what gets updated when there is
// routing changes - and then we message forwarding threads to use the updated one, and
// we then update the old one also with the same route changes, thus keeping both the
// copies always in sync
pub enum V4Table {
    Table1,
    Table2,
}

pub struct IPv4Ctx {
    table1: Arc<IPv4Table>,
    table2: Arc<IPv4Table>,
    which: V4Table,
}

impl IPv4Ctx {
    pub fn new() -> IPv4Ctx {
        IPv4Ctx {
            table1: Arc::new(IPv4Table::new()),
            table2: Arc::new(IPv4Table::new()),
            which: V4Table::Table1,
        }
    }
}

pub fn create_ipv4_nodes(r2: &mut R2, g: &mut Graph<R2Msg>) {
    let ipv4_parse_node = IPv4Parse::new(&mut r2.counters);
    let init = GnodeInit {
        name: ipv4_parse_node.name(),
        next_names: ipv4_parse_node.next_names(),
        cntrs: GnodeCntrs::new(&ipv4_parse_node.name(), &mut r2.counters),
        perf: Perf::new(&ipv4_parse_node.name(), &mut r2.counters),
    };
    g.add(Box::new(ipv4_parse_node), init);

    let ipv4_fwd_node = IPv4Fwd::new(r2.ipv4.table1.clone(), &mut r2.counters);
    let init = GnodeInit {
        name: ipv4_fwd_node.name(),
        next_names: ipv4_fwd_node.next_names(),
        cntrs: GnodeCntrs::new(&ipv4_fwd_node.name(), &mut r2.counters),
        perf: Perf::new(&ipv4_fwd_node.name(), &mut r2.counters),
    };
    g.add(Box::new(ipv4_fwd_node), init);
}

fn file_write(f: &mut File, s: &str) {
    if let Err(why) = f.write(s.as_bytes()) {
        println!("Write failed {}", why.to_string());
    }
}

fn route_json_dump(f: &mut File, r2: &R2, prefix: Ipv4Addr, masklen: u32, leaf: &IPv4Leaf) {
    if let ipv4::Fwd::Adjacency(adj) = &leaf.next {
        let ifname = if let Some(name) = r2.ifd.get_name(adj.ifindex as usize) {
            name
        } else {
            "Unknown_ifindex"
        };
        let dump = format!(
            "{{ \
             \"prefix\": \"{}\", \
             \"masklen\": {}, \
             \"nhop\": \"{}\", \
             \"ifname\": \"{}\", \
             \"ifindex\": {}}}",
            prefix, masklen, adj.nhop, ifname, adj.ifindex,
        );
        file_write(f, &dump);
    }
}

impl RouteSyncHandler for RouteApis {
    fn handle_add_route(
        &self,
        ip_mask: String,
        nhop: String,
        ifname: String,
    ) -> thrift::Result<()> {
        let mut r2 = self.r2.lock().unwrap();

        let ip;
        let mask;
        let nhop_ip;
        let ifindex;
        if let Some((i, m)) = ip_mask_decode(&ip_mask) {
            ip = i;
            mask = m;
        } else {
            return Err(RouteErr::new("Unable to decode IP/MASK".to_string())).map_err(From::from);
        }
        if let Ok(n) = Ipv4Addr::from_str(&nhop) {
            nhop_ip = n;
        } else {
            return Err(RouteErr::new("Unable to decode NHOP".to_string())).map_err(From::from);
        }
        if let Some(intf) = r2.ifd.get(&ifname) {
            ifindex = intf.ifindex;
        } else {
            return Err(RouteErr::new(format!("Cannot find interface {}", ifname)))
                .map_err(From::from);
        }
        add_route(&mut r2, ip, mask, nhop_ip, ifindex);
        Ok(())
    }

    fn handle_del_route(
        &self,
        ip_mask: String,
        nhop: String,
        ifname: String,
    ) -> thrift::Result<()> {
        let mut r2 = self.r2.lock().unwrap();

        let ip;
        let mask;
        let nhop_ip;
        let ifindex;
        if let Some((i, m)) = ip_mask_decode(&ip_mask) {
            ip = i;
            mask = m;
        } else {
            return Err(RouteErr::new("Unable to decode IP/MASK".to_string())).map_err(From::from);
        }
        if let Ok(n) = Ipv4Addr::from_str(&nhop) {
            nhop_ip = n;
        } else {
            return Err(RouteErr::new("Unable to decode NHOP".to_string())).map_err(From::from);
        }
        if let Some(intf) = r2.ifd.get(&ifname) {
            ifindex = intf.ifindex;
        } else {
            return Err(RouteErr::new(format!("Cannot find interface {}", ifname)))
                .map_err(From::from);
        }
        del_route(&mut r2, ip, mask, nhop_ip, ifindex);
        Ok(())
    }

    fn handle_show(&self, prefix: String, filename: String) -> thrift::Result<String> {
        if let Ok(ipaddr) = Ipv4Addr::from_str(&prefix) {
            let r2 = self.r2.lock().unwrap();
            let mut s = "Table1:\n".to_string();
            s.push_str(&self.handle_show_one(&r2, &r2.ipv4.table1, ipaddr));
            s.push_str("Table2:\n");
            s.push_str(&self.handle_show_one(&r2, &r2.ipv4.table2, ipaddr));
            Ok(s)
        } else if prefix == "all" {
            self.handle_show_all(filename)
        } else {
            Err(RouteErr::new(format!(
                "Option should be ip address or keyword 'all': {}",
                prefix
            )))
            .map_err(From::from)
        }
    }
}

// First add to the  table thats not currently in use, then broadcast
// that table for all pipelines to use/switch to. And then add the same
// route onto the old table, looping till all the old table references
// fall off (when all dataplanes switch to the new one)
fn add_or_del_route(
    r2: &mut R2,
    ip: Ipv4Addr,
    masklen: u32,
    nhop: Ipv4Addr,
    ifindex: usize,
    add: bool,
) {
    let next = Fwd::Adjacency(Arc::new(Adjacency::new(nhop, ifindex)));
    let leaf = Arc::new(IPv4Leaf::new(next));
    match r2.ipv4.which {
        V4Table::Table1 => {
            let table = Arc::get_mut(&mut r2.ipv4.table2).unwrap();
            if add {
                table.add(ip, masklen, leaf.clone());
            } else {
                table.del(ip, masklen);
            }
            r2.ipv4.which = V4Table::Table2;
            let msg = IPv4TableMsg::new(r2.ipv4.table2.clone());
            r2.broadcast(R2Msg::IPv4TableAdd(msg));
            loop {
                // The Arc::get_mut() will succeed only when the table has a refcount of 1.
                // Ie when all forwarding plane threads drop their reference and only the
                // control plane thread has the reference.
                if let Some(table) = Arc::get_mut(&mut r2.ipv4.table1) {
                    if add {
                        table.add(ip, masklen, leaf);
                    } else {
                        table.del(ip, masklen);
                    }
                    break;
                }
            }
        }
        V4Table::Table2 => {
            let table = Arc::get_mut(&mut r2.ipv4.table1).unwrap();
            if add {
                table.add(ip, masklen, leaf.clone());
            } else {
                table.del(ip, masklen);
            }
            r2.ipv4.which = V4Table::Table1;
            let msg = IPv4TableMsg::new(r2.ipv4.table1.clone());
            r2.broadcast(R2Msg::IPv4TableAdd(msg));
            loop {
                // The Arc::get_mut() will succeed only when the table has a refcount of 1.
                // Ie when all forwarding plane threads drop their reference and only the
                // control plane thread has the reference.
                if let Some(table) = Arc::get_mut(&mut r2.ipv4.table2) {
                    if add {
                        table.add(ip, masklen, leaf);
                    } else {
                        table.del(ip, masklen);
                    }
                    break;
                }
            }
        }
    }
}

pub fn add_route(r2: &mut R2, ip: Ipv4Addr, masklen: u32, nhop: Ipv4Addr, ifindex: usize) {
    add_or_del_route(r2, ip, masklen, nhop, ifindex, true);
}

pub fn del_route(r2: &mut R2, ip: Ipv4Addr, masklen: u32, nhop: Ipv4Addr, ifindex: usize) {
    add_or_del_route(r2, ip, masklen, nhop, ifindex, false);
}
