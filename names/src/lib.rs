pub const DROP: &str = "drop";
pub const IFMUX: &str = "ifmux";
pub const ENCAPMUX: &str = "encapmux";
const RX_TX: &str = "rx_tx:";
pub const L2_ETH_DECAP: &str = "l2_eth_decap:";
pub const L2_ETH_ENCAP: &str = "l2_eth_encap:";
pub const L3_IPV4_PARSE: &str = "l3_ipv4_parse";
pub const L3_IPV4_FWD: &str = "l3_ipv4_fwd";

pub fn rx_tx(ifindex: usize) -> String {
    let mut name = RX_TX.to_string();
    name.push_str(&ifindex.to_string());
    name
}

pub fn l2_eth_decap(ifindex: usize) -> String {
    let mut name = L2_ETH_DECAP.to_string();
    name.push_str(&ifindex.to_string());
    name
}

pub fn l2_eth_encap(ifindex: usize) -> String {
    let mut name = L2_ETH_ENCAP.to_string();
    name.push_str(&ifindex.to_string());
    name
}
