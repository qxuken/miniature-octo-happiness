//! Bounded model of Memory::admit, Charge::drop and Memory::usage.
//! It models reservations, not actual allocation, pointer or destructor safety.
use loom::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Budget {
    used: AtomicUsize,
    peak: AtomicUsize,
    broken_admission: bool,
    broken_sampling: bool,
}

impl Budget {
    const LIMIT: usize = 8;

    fn admit(&self, bytes: usize) -> bool {
        let permitted = |used: usize| used.checked_add(bytes).filter(|n| *n <= Self::LIMIT);
        let previous = if self.broken_admission {
            // Negative control: individually atomic load/store still loses
            // concurrent reservations. This mutates integers only.
            let previous = self.used.load(Ordering::Relaxed);
            let Some(next) = permitted(previous) else {
                return false;
            };
            self.used.store(next, Ordering::Relaxed);
            previous
        } else {
            // Loom's fetch_update implements the same successful/failed CAS
            // ordering as production try_update. Allocation follows admission.
            let Ok(previous) =
                self.used
                    .fetch_update(Ordering::AcqRel, Ordering::Relaxed, permitted)
            else {
                return false;
            };
            previous
        };
        self.peak.fetch_max(previous + bytes, Ordering::Relaxed);
        true
    }

    fn release(&self, bytes: usize) {
        let previous = self.used.fetch_sub(bytes, Ordering::AcqRel);
        assert!(
            previous >= bytes,
            "no released reservation may be forgotten"
        );
    }

    fn usage(&self) -> (usize, usize) {
        let live = self.used.load(Ordering::Acquire);
        let peak = if self.broken_sampling {
            self.peak.load(Ordering::Relaxed).max(live)
        } else {
            self.peak.fetch_max(live, Ordering::Relaxed).max(live)
        };
        assert!(live <= peak && peak <= Self::LIMIT);
        (live, peak)
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    assert!(matches!(
        mode.as_str(),
        "" | "--broken-admission" | "--broken-sampling"
    ));
    let mut model = loom::model::Builder::new();
    model.max_threads = 3;
    model.preemption_bound = Some(2);
    model.max_branches = 1000;
    model.max_permutations = None;
    model.max_duration = None;
    model.check(move || {
        // Worker B owns an initial reservation. A can reach the exact cap
        // while B releases it; queries can overlap the CAS-to-peak window.
        let budget = Arc::new(Budget {
            used: AtomicUsize::new(4),
            peak: AtomicUsize::new(4),
            broken_admission: mode == "--broken-admission",
            broken_sampling: mode == "--broken-sampling",
        });
        let a = budget.clone();
        let first = loom::thread::spawn(move || {
            if a.admit(4) {
                a.release(4); // Allocation success/drop or failure rollback.
            }
        });
        let b = budget.clone();
        let second = loom::thread::spawn(move || {
            b.release(4);
            if b.admit(5) {
                b.release(5);
            }
        });
        let first_peak = budget.usage().1;
        let second_peak = budget.usage().1;
        assert!(second_peak >= first_peak, "observed peak must not decrease");
        first.join().unwrap();
        second.join().unwrap();
        let (live, peak) = budget.usage();
        assert_eq!(live, 0);
        assert!(peak >= second_peak && peak >= 4);
    });
    println!("Reservation and telemetry model passed: three threads, preemption bound two");
}
