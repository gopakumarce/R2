use counters::CountersRO;
use std::process;

fn main() {
    if let Ok(counters) = CountersRO::new(common::R2CNT_SHM) {
        for (name, cntr) in counters.hash.iter() {
            print!("{}: ", name);
            for i in 0..cntr.num_cntrs() {
                print!("{} ", cntr.read(i));
            }
            println!();
        }
    } else {
        println!("No shared memory r2 found");
        process::exit(1);
    }
}
