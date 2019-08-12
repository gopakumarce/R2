use api::api_client;
use apis_route::{RouteSyncClient, TRouteSyncClient};
#[macro_use]
extern crate clap;
use clap::App;
use clap::ArgMatches;
use fwd::ip_mask_decode;
use std::net::Ipv4Addr;
use std::str::FromStr;

fn add_del_ip(ip_and_mask: &str, nhop: &str, ifname: &str, del: bool) {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::ROUTE_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => {
            println!("Client connection failed: {}", why);
            return;
        }
    };
    let mut client = RouteSyncClient::new(i_prot, o_prot);

    let ret = if del {
        client.del_route(
            ip_and_mask.to_string(),
            nhop.to_string(),
            ifname.to_string(),
        )
    } else {
        client.add_route(
            ip_and_mask.to_string(),
            nhop.to_string(),
            ifname.to_string(),
        )
    };
    if let Err(e) = ret {
        println!("Add failed: {}", e);
        return;
    }
}

fn show(prefix: &str, filename: &str) -> String {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::ROUTE_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => panic!("Client connection failed: {}", why),
    };
    let mut client = RouteSyncClient::new(i_prot, o_prot);
    let ret = client.show(prefix.to_string(), filename.to_string());
    if let Err(e) = ret {
        format!("Show failed: {}", e)
    } else {
        ret.unwrap()
    }
}

fn add_del_subcmd(matches: &ArgMatches) {
    let ip_mask = matches.value_of("IPMASK").unwrap();
    let nhop = matches.value_of("NHOP").unwrap();
    let ifname = matches.value_of("IFNAME").unwrap();
    let del = matches.is_present("delete");

    if ip_mask_decode(ip_mask).is_none() {
        println!("IP/Mask invalid");
        return;
    }
    if Ipv4Addr::from_str(nhop).is_err() {
        println!("Nhop invalid");
        return;
    }
    add_del_ip(ip_mask, nhop, ifname, del);
}

fn show_subcmd(matches: &ArgMatches) -> String {
    let prefix = matches.value_of("PREFIX").unwrap();
    if prefix != "all" {
        if let Err(_n) = Ipv4Addr::from_str(&prefix) {
            return "Prefix should be a valid ip address or keyword 'all'".to_string();
        }
    }
    if let Some(name) = matches.value_of("FILENAME") {
        show(prefix, name);
        String::new()
    } else {
        if prefix == "all" {
            println!("Writing routes to file /tmp/r2_routes.json");
        }
        show(prefix, "/tmp/r2_routes.json")
    }
}

fn main() {
    let yaml = load_yaml!("./r2rt.yml");
    let matches = App::from(yaml).get_matches();

    if let Some(matches) = matches.subcommand_matches("route") {
        add_del_subcmd(&matches);
    } else if let Some(matches) = matches.subcommand_matches("show") {
        let show = show_subcmd(&matches);
        println!("{}", show);
    }
}
