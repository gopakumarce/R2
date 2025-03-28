use counters::Counters;

pub struct Perf {}

impl Perf {
    pub fn new(name: &str, counters: &mut Counters) -> Self {
        Perf {}
    }

    pub fn start(&mut self) {}

    pub fn stop(&mut self) {}

    pub fn get_count(&self) -> u64 {
        0
    }

    pub fn get_avg(&self) -> u64 {
        0
    }
}
