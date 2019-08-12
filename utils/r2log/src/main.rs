use api::api_client;
use apis_log::{LogSyncClient, TLogSyncClient};
#[macro_use]
extern crate clap;
use clap::App;

fn show_logging(filename: &str) {
    let (i_prot, o_prot) = match api_client(common::API_SVR, common::LOG_APIS) {
        Ok((i, o)) => (i, o),
        Err(why) => {
            println!("Client connection failed: {}", why);
            return;
        }
    };
    let mut client = LogSyncClient::new(i_prot, o_prot);

    match client.show(filename.to_string()) {
        Ok(result) => result,
        Err(why) => println!("Command failed: {}", why),
    }
}

fn main() {
    let yaml = load_yaml!("./r2log.yml");
    let matches = App::from(yaml).get_matches();

    if let Some(name) = matches.value_of("FILENAME") {
        show_logging(name);
    } else {
        println!("Writing logs to /tmp/r2_logs.json");
        show_logging("/tmp/r2_logs.json");
    }
}
