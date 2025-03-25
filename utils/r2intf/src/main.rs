use api::api_client;
use apis_interface::{CurvesApi, InterfaceSyncClient, ScApi, TInterfaceSyncClient};
use clap::ArgMatches;
#[macro_use]
extern crate clap;
use clap::App;

fn interface_add(ifname: String, ifindex: i32, mac: String) {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::INTF_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => {
            println!("Client connection failed: {}", why);
            return;
        }
    };
    let mut client = InterfaceSyncClient::new(i_prot, o_prot);

    if let Err(e) = client.add_if(ifname, ifindex, mac) {
        println!("Add failed: {}", e);
    }
}

fn add_ip(ifname: String, ip_and_mask: String) {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::INTF_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => {
            println!("Client connection failed: {}", why);
            return;
        }
    };
    let mut client = InterfaceSyncClient::new(i_prot, o_prot);

    if let Err(e) = client.add_ip(ifname, ip_and_mask) {
        println!("Add failed: {}", e);
    }
}

fn class_add_del(
    del: bool,
    ifname: &str,
    class: &str,
    parent: &str,
    qlimit: i32,
    leaf: bool,
    curves: CurvesApi,
) {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::INTF_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => {
            println!("Client connection failed: {}", why);
            return;
        }
    };
    let mut client = InterfaceSyncClient::new(i_prot, o_prot);

    if !del {
        if let Err(e) = client.add_class(
            ifname.to_string(),
            class.to_string(),
            parent.to_string(),
            qlimit,
            leaf,
            curves,
        ) {
            println!("Add failed: {}", e);
        }
    }
}

fn class_parse_fsc(curves: &mut CurvesApi, matches: &ArgMatches) {
    if matches.is_present("fm1") {
        let m1 = value_t!(matches, "fm1", i32).unwrap_or_else(|e| e.exit());
        let d = value_t!(matches, "fd", i32).unwrap_or_else(|e| e.exit());
        let m2 = value_t!(matches, "fm2", i32).unwrap_or_else(|e| e.exit());
        curves.f_sc = Some(ScApi {
            m1: Some(m1),
            d: Some(d),
            m2: Some(m2),
        });
    }
}

fn class_parse_rsc(curves: &mut CurvesApi, matches: &ArgMatches) {
    if matches.is_present("rm1") {
        let m1 = value_t!(matches, "rm1", i32).unwrap_or_else(|e| e.exit());
        let d = value_t!(matches, "rd", i32).unwrap_or_else(|e| e.exit());
        let m2 = value_t!(matches, "rm2", i32).unwrap_or_else(|e| e.exit());
        curves.r_sc = Some(ScApi {
            m1: Some(m1),
            d: Some(d),
            m2: Some(m2),
        });
    }
}

fn class_parse_usc(curves: &mut CurvesApi, matches: &ArgMatches) {
    if matches.is_present("um1") {
        let m1 = value_t!(matches, "um1", i32).unwrap_or_else(|e| e.exit());
        let d = value_t!(matches, "ud", i32).unwrap_or_else(|e| e.exit());
        let m2 = value_t!(matches, "um2", i32).unwrap_or_else(|e| e.exit());
        curves.u_sc = Some(ScApi {
            m1: Some(m1),
            d: Some(d),
            m2: Some(m2),
        });
    }
}

fn class_subcmd(ifname: &str, matches: &ArgMatches) {
    let mut curves = CurvesApi {
        r_sc: None,
        u_sc: None,
        f_sc: None,
    };
    let mut del = false;
    let mut leaf = false;
    let mut qlimit = 0;
    let class = matches.value_of("CLASS").unwrap();
    let parent = matches.value_of("PARENT").unwrap();
    if matches.is_present("delete") {
        del = true;
    }
    if matches.is_present("leaf") {
        leaf = true;
    }
    if matches.is_present("qlimit") {
        qlimit = value_t!(matches, "qlimit", i32).unwrap_or_else(|e| e.exit());
    }
    class_parse_fsc(&mut curves, matches);
    class_parse_rsc(&mut curves, matches);
    class_parse_usc(&mut curves, matches);
    class_add_del(del, ifname, class, parent, qlimit, leaf, curves);
}

fn add_subcmd(ifname: &str, matches: &ArgMatches) {
    let ifindex = value_t!(matches, "IFINDEX", i32).unwrap_or_else(|e| e.exit());
    let mac = value_t!(matches, "MAC", String).unwrap_or_else(|e| e.exit());
    if fwd::str_to_mac(&mac).is_none() {
        println!("Bad Mac address {}", &mac);
        return;
    }
    interface_add(ifname.to_string(), ifindex, mac);
}

fn ip_subcmd(ifname: &str, matches: &ArgMatches) {
    let ip_and_mask = value_t!(matches, "IPMASK", String).unwrap_or_else(|e| e.exit());
    if fwd::ip_mask_decode(&ip_and_mask).is_none() {
        println!("Bad IP/MASK {}", &ip_and_mask);
        return;
    }
    add_ip(ifname.to_string(), ip_and_mask);
}

fn main() {
    let yaml = load_yaml!("./r2intf.yml");
    let matches = App::from(yaml).get_matches();

    let ifname = matches.value_of("IFNAME").unwrap();

    if let Some(matches) = matches.subcommand_matches("add") {
        add_subcmd(ifname, matches);
    } else if let Some(matches) = matches.subcommand_matches("class") {
        class_subcmd(ifname, matches);
    } else if let Some(matches) = matches.subcommand_matches("ip") {
        ip_subcmd(ifname, matches);
    }
}
