//! Bounded ownership model of retain_ref and release_ref in src/memory.rs.
//! Allocation and pointer validity remain obligations of the native/Miri tests.
use loom::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

mod ffi;

struct SharedModel {
    refs: AtomicUsize,
    release_order: Ordering,
    visits: [AtomicUsize; 2],
    destroyed: AtomicUsize,
}

impl SharedModel {
    fn clone_owner(&self) {
        assert!(self.refs.fetch_add(1, Ordering::Relaxed) < isize::MAX as usize);
    }

    fn drop_owner(&self) {
        if self.refs.fetch_sub(1, self.release_order) != 1 {
            return;
        }
        // Final destruction must observe both prior owners' accesses through
        // the release sequence on refs, without relying on thread joins.
        assert_eq!(
            self.visits[0].load(Ordering::Relaxed),
            1,
            "final owner must observe earlier owners' accesses"
        );
        assert_eq!(
            self.visits[1].load(Ordering::Relaxed),
            1,
            "final owner must observe earlier owners' accesses"
        );
        assert_eq!(self.destroyed.fetch_add(1, Ordering::Relaxed), 0);
    }
}

fn main() {
    let mode = std::env::args().nth(1);
    let control = match mode.as_deref() {
        Some("--ffi-handles") => Some(ffi::Control::None),
        Some("--broken-ffi-reservation") => Some(ffi::Control::LoadStore),
        Some("--broken-ffi-rollback") => Some(ffi::Control::SkipRollback),
        None | Some("--broken-shared-release") => None,
        Some(mode) => panic!("unknown ownership model mode: {mode}"),
    };
    if let Some(control) = control {
        ffi::run(control);
        return;
    }
    let release_order = if mode.as_deref() == Some("--broken-shared-release") {
        Ordering::Relaxed
    } else {
        Ordering::AcqRel
    };
    let mut model = loom::model::Builder::new();
    model.max_threads = 3;
    model.preemption_bound = Some(2);
    model.max_branches = 1000;
    model.max_permutations = None;
    model.max_duration = None;
    model.check(move || {
        let state = Arc::new(SharedModel {
            refs: AtomicUsize::new(3),
            release_order,
            visits: [AtomicUsize::new(0), AtomicUsize::new(0)],
            destroyed: AtomicUsize::new(0),
        });
        let threads: Vec<_> = (0..2)
            .map(|index| {
                let state = state.clone();
                loom::thread::spawn(move || {
                    state.clone_owner();
                    state.visits[index].store(1, Ordering::Relaxed);
                    state.drop_owner();
                    state.drop_owner();
                })
            })
            .collect();
        state.drop_owner();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(state.refs.load(Ordering::Relaxed), 0);
        assert_eq!(state.destroyed.load(Ordering::Relaxed), 1);
    });
    println!("Shared ownership model passed with three threads and preemption bound two");
}
