//! Bounded handle-count model for CDatabase. Close runs only after workers join.
use loom::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Control {
    None,
    LoadStore,
    SkipRollback,
}

fn reserve(handles: &AtomicUsize, control: Control) -> bool {
    if control == Control::LoadStore {
        // Negative control for losing a concurrent reservation or rollback.
        let prior = handles.load(Ordering::Relaxed);
        let Some(next) = prior.checked_add(1) else {
            return false;
        };
        handles.store(next, Ordering::Relaxed);
        return true;
    }
    // Loom's fetch_update models the production try_update CAS and orderings.
    handles
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .is_ok()
}

fn check(initial: usize, control: Control) {
    let mut model = loom::model::Builder::new();
    model.max_threads = 3;
    model.preemption_bound = Some(2);
    model.max_branches = 1000;
    model.max_permutations = None;
    model.max_duration = None;
    model.check(move || {
        let handles = Arc::new(AtomicUsize::new(initial));
        let refuser = handles.clone();
        let first = loom::thread::spawn(move || {
            // Allocator refusal drops the temporary Handle after reservation.
            if reserve(&refuser, control) && control != Control::SkipRollback {
                refuser.fetch_sub(1, Ordering::AcqRel);
            }
        });
        let acquirer = handles.clone();
        let second = loom::thread::spawn(move || reserve(&acquirer, control));
        first.join().unwrap();
        let acquired = second.join().unwrap();
        // After external quiescence close uses an Acquire load. The successful
        // worker's handle stays live and remains accounted for until release.
        assert_eq!(
            handles.load(Ordering::Acquire),
            initial + usize::from(acquired),
            "only successful handle reservations may survive quiescence"
        );
        if acquired {
            handles.fetch_sub(1, Ordering::AcqRel);
        }
        assert_eq!(handles.load(Ordering::Acquire), initial);
    });
}

pub(super) fn run(control: Control) {
    check(1, control);
    check(usize::MAX - 1, control);
    println!("FFI handle accounting passed: three threads, preemption bound two");
}
