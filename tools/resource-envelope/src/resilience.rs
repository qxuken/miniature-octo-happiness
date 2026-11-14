//! Finite compound maintenance/recovery drills; no new engine surface.
use super::*;
use std::{
    io::{BufRead, BufReader, Read},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use storage::{CompactionEvent, CompactionStep as S};

const STEPS: [S; 17] = [
    S::SourceLength,
    S::SourceRead,
    S::SourceWrite,
    S::SourceSync,
    S::TargetLength,
    S::TargetRead,
    S::TargetWrite,
    S::TargetSync,
    S::CopyMetadata,
    S::SyncMetadata,
    S::CheckNames,
    S::Exchange,
    S::SyncDirectory,
    S::SyncReplacement,
    S::RemoveOld,
    S::SyncCleanupDirectory,
    S::SyncCleanupFile,
];
const COPY: usize = 256 * MIB;
fn command(dir: &Path, image: &Path, mode: &str) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.arg(dir).arg(image).arg(mode);
    cmd
}
fn fresh_check(dir: &Path, image: &Path, phase: &str, revision: u64, changed: bool) {
    assert!(
        command(dir, image, "resilience-check")
            .arg(phase)
            .arg(revision.to_string())
            .arg(changed.to_string())
            .status()
            .unwrap()
            .success()
    );
}
pub(super) fn check(image: &Path, phase: &str, revision: u64, changed: bool) {
    let db = Database::open_file(image, limits(CORE)).unwrap();
    event(
        phase,
        &db,
        revision,
        ROUNDS,
        changed,
        0,
        "{\"fresh_process\":true}",
        true,
    );
}
pub(super) fn child(image: &Path, step: usize, occurrence: usize, after: bool) {
    let mut hits = 0;
    storage::compact_file_with_hook(image, limits(COPY), move |e| {
        if e.step == STEPS[step] && e.after == after {
            if hits == occurrence {
                println!("READY {step} {occurrence} {after}");
                std::io::stdout().flush().unwrap();
                // Parent SIGKILLs this process; neither Rust drop nor normal
                // cleanup can hide an orphan or release authority early.
                loop {
                    thread::park();
                }
            }
            hits += 1;
        }
        Ok(())
    })
    .unwrap();
    panic!("requested compaction boundary never reached");
}
pub(super) fn maintenance(dir: &Path, source: &Path, publish: bool) {
    fs::create_dir(dir).unwrap();
    let original = fs::read(source).unwrap();
    let control = dir.join("control.wdb");
    fs::write(&control, &original).unwrap();
    let counts = Arc::new(Mutex::new([0usize; 17]));
    let seen = counts.clone();
    storage::compact_file_with_hook(&control, limits(COPY), move |e: CompactionEvent| {
        if !e.after {
            let i = STEPS.iter().position(|s| *s == e.step).unwrap();
            seen.lock().unwrap()[i] += 1;
        }
        Ok(())
    })
    .unwrap();
    let compacted = fs::read(&control).unwrap();
    assert_eq!(original.len(), 66_154_496);
    assert_eq!(compacted.len(), 63_074_304);
    fresh_check(dir, &control, "control", FINAL, false);
    let counts = *counts.lock().unwrap();
    assert!(counts.iter().all(|n| *n > 0));
    let mut cuts: Vec<_> = (0..17)
        .filter(|s| (*s >= 9) == publish)
        .map(|s| (s, 0))
        .collect();
    if !publish {
        for step in [1, 5, 6] {
            assert!(counts[step] > 2);
            cuts.push((step, counts[step] / 2));
            cuts.push((step, counts[step] - 1));
        }
    }
    for (step, occurrence) in cuts {
        for after in [false, true] {
            let name = format!(
                "s{step}_n{occurrence}_{}",
                if after { "after" } else { "before" }
            );
            let case = dir.join(&name);
            fs::create_dir(&case).unwrap();
            let image = case.join("store.wdb");
            fs::write(&image, &original).unwrap();
            let mut child = command(&case, &image, "resilience-child")
                .arg(step.to_string())
                .arg(occurrence.to_string())
                .arg(after.to_string())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut ready = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .take(256)
                .read_line(&mut ready)
                .unwrap();
            assert_eq!(ready.trim(), format!("READY {step} {occurrence} {after}"));
            child.kill().unwrap();
            let status = child.wait().unwrap();
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(9));
            let exchanged = step > 11 || (step == 11 && after);
            assert_eq!(
                fs::read(&image).unwrap(),
                if exchanged { &compacted } else { &original }.as_slice()
            );
            let orphans = fs::read_dir(&case).unwrap().count() - 1;
            println!(
                "{{\"boundary\":\"{name}\",\"step\":{step},\"occurrence\":{occurrence},\"count\":{},\"after\":{after},\"signal\":9,\"exchanged\":{exchanged},\"orphans\":{orphans}}}",
                counts[step]
            );
            fresh_check(&case, &image, &format!("killed_{name}"), FINAL, false);
            Database::compact_file(&image, limits(COPY)).unwrap();
            assert_eq!(fs::read(&image).unwrap(), compacted);
            fresh_check(&case, &image, &format!("retried_{name}"), FINAL, false);
            // Only this fresh, private case directory is retired, including
            // any interrupted scratch inode. Engine cleanup is not inferred.
            fs::remove_dir_all(&case).unwrap();
        }
    }
    assert_eq!(fs::read(source).unwrap(), original);
    println!(
        "{{\"cleanup\":true,\"cases\":{},\"source_unchanged\":true}}",
        if publish { 16 } else { 30 }
    );
    complete(if publish {
        "maintenance-publish"
    } else {
        "maintenance-copy"
    });
}
fn first(image: &[u8], initial: usize, syncs: &[usize]) -> SimulatedDisk {
    let disk = SimulatedDisk::from_image(image).unwrap();
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    let (fault, error) = match initial {
        0 => (
            Some((syncs[0], Fault::SyncError(Crash::DropAll))),
            Some(Error::Io),
        ),
        1 => (
            Some((syncs[0], Fault::SyncError(Crash::PersistAll))),
            Some(Error::Io),
        ),
        2 => (
            Some((syncs[1], Fault::SyncError(Crash::DropAll))),
            Some(Error::OutcomeUnknown),
        ),
        3 => (
            Some((syncs[1], Fault::SyncError(Crash::PersistAll))),
            Some(Error::OutcomeUnknown),
        ),
        4 => (
            Some((syncs[2], Fault::SyncError(Crash::DropAll))),
            Some(Error::OutcomeUnknown),
        ),
        5 => (None, None),
        _ => unreachable!(),
    };
    let mut tx = probe(&db);
    if let Some((step, fault)) = fault {
        disk.arm(step, fault);
    }
    assert_eq!(tx.commit(), error.map_or(Ok(FINAL + 1), Err));
    assert_eq!(db.snapshot().revision(), FINAL + u64::from(error.is_none()));
    assert_eq!(db.requires_recovery(), error.is_some());
    drop(tx);
    drop(db);
    disk.clear_fault();
    // Deliberately do not crash: rootVolatile must remain visible to the next
    // verifier while lacking a durable root and a corresponding witness.
    disk
}
pub(super) fn recovery(dir: &Path, source: &Path, initial: usize) {
    fs::create_dir(dir).unwrap();
    let image = fs::read(source).unwrap();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    probe(&db).commit().unwrap();
    let syncs: Vec<_> = disk
        .trace()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (e.kind == IoKind::Sync).then_some(i))
        .collect();
    assert_eq!(syncs.len(), 3);
    drop(db);
    drop(disk);
    let disk = first(&image, initial, &syncs);
    disk.arm(usize::MAX, Fault::Error);
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    let selected = FINAL + u64::from(initial >= 2);
    verify(&db, selected, ROUNDS, initial >= 2, true);
    drop(db);
    let trace = disk.trace().unwrap();
    drop(disk);
    let barriers: Vec<_> = trace
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e.kind, IoKind::Write | IoKind::Sync).then_some((i, e.kind)))
        .collect();
    assert_eq!(barriers.len(), 3);
    assert_eq!(
        [barriers[0].1, barriers[1].1, barriers[2].1],
        [IoKind::Sync, IoKind::Write, IoKind::Sync]
    );
    let read_first = trace.iter().position(|e| e.kind == IoKind::Read).unwrap();
    let read_last = trace.iter().rposition(|e| e.kind == IoKind::Read).unwrap();
    for second in 0..9 {
        for persist in [false, true] {
            let disk = first(&image, initial, &syncs);
            let (step, fault) = match second {
                0 => (read_first, Fault::Error),
                1 => (read_last, Fault::Error),
                2 => (barriers[0].0, Fault::SyncError(Crash::DropAll)),
                3 => (barriers[0].0, Fault::SyncError(Crash::PersistAll)),
                4 => (
                    barriers[1].0,
                    Fault::Tear {
                        prefix: 63,
                        durable: false,
                    },
                ),
                5 => (
                    barriers[1].0,
                    Fault::Tear {
                        prefix: 63,
                        durable: true,
                    },
                ),
                6 => (barriers[2].0, Fault::SyncError(Crash::DropAll)),
                7 => (barriers[2].0, Fault::SyncError(Crash::PersistAll)),
                8 => (usize::MAX, Fault::Error),
                _ => unreachable!(),
            };
            disk.arm(step, fault);
            let result = Database::open_on(
                disk.clone(),
                limits(if second == 8 { 40 * MIB } else { CORE }),
            );
            assert!(
                matches!(result, Err(e) if e == if second == 8 { Error::Limit } else { Error::Io })
            );
            drop(result);
            if second != 8 {
                assert_eq!(disk.trace().unwrap(), trace[..=step]);
            }
            disk.clear_fault();
            disk.crash(if persist {
                Crash::PersistAll
            } else {
                Crash::DropAll
            })
            .unwrap();
            let survives = initial >= 3 || (initial == 2 && (persist || matches!(second, 3..=7)));
            let revision = FINAL + u64::from(survives);
            // A further failed open must not leak its exclusive lease or lose
            // the now-durable selected root, including an acknowledged head.
            let control = SimulatedDisk::from_image(&disk.durable_image().unwrap()).unwrap();
            control.arm(usize::MAX, Fault::Error);
            let reference = Database::open_on(control.clone(), limits(CORE)).unwrap();
            verify(&reference, revision, ROUNDS, survives, true);
            drop(reference);
            let again = control
                .trace()
                .unwrap()
                .iter()
                .rposition(|e| e.kind == IoKind::Sync)
                .unwrap();
            drop(control);
            disk.arm(again, Fault::SyncError(Crash::DropAll));
            assert!(matches!(
                Database::open_on(disk.clone(), limits(CORE)),
                Err(Error::Io)
            ));
            disk.clear_fault();
            disk.crash(Crash::DropAll).unwrap();
            let recovered = Database::open_on(disk.clone(), limits(CORE)).unwrap();
            let name = format!(
                "i{initial}_f{second}_{}",
                if persist { "persist" } else { "drop" }
            );
            event(
                &format!("recovered_{name}"),
                &recovered,
                revision,
                ROUNDS,
                survives,
                0,
                &format!(
                    "{{\"initial\":{initial},\"second\":{second},\"persist\":{persist},\"recovery_error\":\"{}\",\"repeat_error\":\"Io\",\"acknowledged_floor\":{}}}",
                    if second == 8 { "Limit" } else { "Io" },
                    FINAL + u64::from(initial == 5)
                ),
                true,
            );
            if survives {
                recovered.begin().commit().unwrap();
            } else {
                probe(&recovered).commit().unwrap();
            }
            drop(recovered);
            disk.crash(Crash::DropAll).unwrap();
            let checkpoint = dir.join("checkpoint.wdb");
            fs::write(&checkpoint, disk.durable_image().unwrap()).unwrap();
            drop(disk);
            fresh_check(
                dir,
                &checkpoint,
                &format!("retried_{name}"),
                revision + 1,
                true,
            );
            fs::remove_file(&checkpoint).unwrap();
        }
    }
    assert_eq!(fs::read(source).unwrap(), image);
    println!("{{\"cleanup\":true,\"cases\":18,\"source_unchanged\":true}}");
    complete(&format!("recovery-{initial}"));
}
