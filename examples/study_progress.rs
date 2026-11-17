//! Sync applies different resolution policies to two fields in one record.
#[path = "support/study.rs"]
mod study;

use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
use study::StudyProgress;
use weekend_db::{
    Database, Error, Result,
    crdt::{Policy, Replica},
};

fn sync(from: &Replica<'_, StudyProgress>, to: &Replica<'_, StudyProgress>) -> Result<()> {
    // Test-owned loopback endpoints. Applications supply authenticated streams.
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| Error::Io)?;
    let sender =
        TcpStream::connect(listener.local_addr().map_err(|_| Error::Io)?).map_err(|_| Error::Io)?;
    let (receiver, _) = listener.accept().map_err(|_| Error::Io)?;
    for stream in [&sender, &receiver] {
        let timeout = Some(Duration::from_secs(5));
        stream.set_read_timeout(timeout).map_err(|_| Error::Io)?;
        stream.set_write_timeout(timeout).map_err(|_| Error::Io)?;
    }
    thread::scope(|scope| {
        let served = scope.spawn(|| to.serve_authenticated(receiver));
        let ack = from.push_authenticated(sender);
        served.join().map_err(|_| Error::Panic)??;
        ack.map(|_| ())
    })
}

fn main() -> Result<()> {
    let db_a = Database::new()?;
    let db_b = Database::new()?;
    let policy = Policy {
        collection: [9; 16],
        generation: 7,
        actors: [[1; 16], [2; 16]],
    };
    let a = Replica::<StudyProgress>::enroll(
        &db_a,
        db_a.create_collection()?,
        policy,
        policy.actors[0],
    )?;
    let b = Replica::<StudyProgress>::enroll(
        &db_b,
        db_b.create_collection()?,
        policy,
        policy.actors[1],
    )?;

    a.update(|state, actor| {
        state.completed.insert(1)?;
        state.practice_minutes.increment(actor, 20)?;
        state.weekly_goal.assign(actor, 60)?;
        state.session_limit.assign(actor, 20)
    })?;
    b.update(|state, actor| {
        state.planned.insert(2, actor)?;
        state.weekly_goal.assign(actor, 90)?;
        state.session_limit.assign(actor, 30)
    })?;
    println!("Offline weekly goals: A = 60, B = 90 minutes");
    println!("Offline session limits: A = 20, B = 30 minutes");
    sync(&a, &b)?;
    sync(&b, &a)?;

    let synced = a.read()?;
    assert_eq!(synced, b.read()?);
    assert_eq!(synced.effective_weekly_goal(), Some(90));
    assert_eq!(synced.session_limit.values().collect::<Vec<_>>(), [20]);
    println!(
        "Committed weekly goal after sync: {:?} minutes",
        synced.weekly_goal.values().collect::<Vec<_>>()
    );
    println!(
        "Committed session limit after sync: {:?} minutes",
        synced.session_limit.values().collect::<Vec<_>>()
    );

    let revisions = (db_a.snapshot().revision(), db_b.snapshot().revision());
    sync(&a, &b)?;
    sync(&b, &a)?;
    assert_eq!(
        revisions,
        (db_a.snapshot().revision(), db_b.snapshot().revision())
    );
    println!(
        "Repeat sync kept A at revision {} and B at revision {}",
        revisions.0, revisions.1
    );

    let reviewed = synced.weekly_goal;
    a.update(|state, actor| state.resolve_goal(actor, reviewed, 75))?;
    sync(&a, &b)?;
    let edited = b.read()?;
    assert_eq!(a.read()?, edited);
    println!(
        "Later reviewed edit: {:?} minutes",
        edited.weekly_goal.values().collect::<Vec<_>>()
    );
    Ok(())
}
