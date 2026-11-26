//! Fixed deadlines for the two single-writer fixtures.
use std::{
    ops::RangeInclusive,
    thread,
    time::{Duration, Instant},
};

pub const WRITE_PERIOD_US: RangeInclusive<u64> = 1_000..=25_000;

pub struct Schedule {
    start: Instant,
    period_us: u64,
}

impl Schedule {
    pub fn new(period_us: u64) -> Self {
        Self {
            start: Instant::now(),
            period_us,
        }
    }

    pub fn begin(&self, epoch: u64) -> u64 {
        let due = Duration::from_micros(self.period_us * (epoch - 1));
        loop {
            let elapsed = self.start.elapsed();
            match due.checked_sub(elapsed) {
                Some(delay) if !delay.is_zero() => thread::sleep(delay),
                _ => return nanos(elapsed),
            }
        }
    }

    pub fn finish(&self) -> u64 {
        nanos(self.start.elapsed())
    }
}

fn nanos(duration: Duration) -> u64 {
    duration
        .as_nanos()
        .try_into()
        .expect("bounded run duration")
}
