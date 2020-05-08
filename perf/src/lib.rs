use counters::{
    flavors::{CounterArray, CounterType},
    Counters,
};
use std::arch::x86_64::_rdtsc;

pub struct Perf {
    // Three counters:
    // index 0 is the timestamp rdtsc()
    // inex 1 is the hit count
    // index 2 is the sum of rdtsc() delta, the average of which is what usually is of interest
    cntrs: CounterArray,
}

impl Perf {
    pub fn new(name: &str, counters: &mut Counters) -> Self {
        let mut cntrs = CounterArray::new(counters, "perf", CounterType::Info, name, 3);
        cntrs.set(0, 0);
        cntrs.set(1, 0);
        cntrs.set(2, 0);
        Perf { cntrs }
    }

    pub fn start(&mut self) {
        unsafe {
            self.cntrs.set(0, _rdtsc());
        }
    }

    pub fn stop(&mut self) {
        unsafe {
            let elapsed = _rdtsc() - self.cntrs.get(0);
            self.cntrs.add(1, 1);
            self.cntrs.add(2, elapsed);
        }
    }

    pub fn get_count(&self) -> u64 {
        self.cntrs.get(1)
    }

    pub fn get_avg(&self) -> u64 {
        if self.cntrs.get(1) != 0 {
            self.cntrs.get(2) / self.cntrs.get(1)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_perf() {
        let mut counters = Counters::new("perf_test").unwrap();
        let mut p = Perf::new("perf", &mut counters);
        p.start();
        let mut _i = 0;
        for _ in 0..100 {
            _i += 1;
        }
        p.stop();
        assert_eq!(p.get_count(), 1);
        let mut total = p.cntrs.get(2);
        assert!(p.get_avg() > 50);
        assert!(p.get_avg() == total);
        p.start();
        let mut _i = 0;
        for _ in 0..100 {
            _i += 1;
        }
        p.stop();
        assert_eq!(p.get_count(), 2);
        total = p.cntrs.get(2);
        assert!(total > p.get_avg());
        assert!(p.get_avg() > 50);
    }
}
