use weekend_db::{
    Database,
    crdt::{GrowCounter, GrowSet, MaxRegister, Policy, Record, Replica, Resolution},
};
use weekend_db_typed::{Model, Store};

#[derive(Model)]
#[weekend(type_id = 912, version = 1)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    score: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Record, Resolution)]
#[record(
    key = b"progress",
    format = b"PKGTEST1",
    fields(done, count, goal),
    resolve = true
)]
struct Progress {
    done: GrowSet,
    count: GrowCounter,
    #[resolve]
    goal: MaxRegister,
}

// Only test-owned loopback streams are trusted here; deployments authenticate
// their transports before invoking the replica's authenticated-stream API.
fn sync(from: &Replica<'_, Progress>, to: &Replica<'_, Progress>) -> weekend_db::Result<()> {
    use std::{
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| weekend_db::Error::Io)?;
    let sender = TcpStream::connect(listener.local_addr().map_err(|_| weekend_db::Error::Io)?)
        .map_err(|_| weekend_db::Error::Io)?;
    let (receiver, _) = listener.accept().map_err(|_| weekend_db::Error::Io)?;
    for stream in [&sender, &receiver] {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|_| weekend_db::Error::Io)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|_| weekend_db::Error::Io)?;
    }
    thread::scope(|scope| {
        let serving = scope.spawn(|| to.serve_authenticated(receiver));
        let result = from.push_authenticated(sender);
        serving.join().map_err(|_| weekend_db::Error::Panic)??;
        result.map(|_| ())
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        weekend_db::version::SOURCE_ID,
        std::env::args().nth(1).expect("expected source ID")
    );
    let mut store = Store::memory()?;
    let tasks = store.table::<Task>("tasks")?;
    store.write(|w| {
        w.insert(
            &tasks,
            1,
            &Task {
                slug: "source-package".into(),
                score: 17,
            },
        )
    })?;
    store.write(|w| {
        w.update(&tasks, 1, |task| {
            task.score += 1;
            Ok(())
        })
    })?;
    store.read(|r| {
        assert_eq!(
            r.find(&tasks, Task::SLUG, "source-package")?.unwrap().score,
            18
        );
        Ok(())
    })?;

    let a = Database::new()?;
    let b = Database::new()?;
    let policy = Policy {
        collection: [9; 16],
        generation: 1,
        actors: [[1; 16], [2; 16]],
    };
    let left = Replica::<Progress>::enroll(&a, a.create_collection()?, policy, policy.actors[0])?;
    let right = Replica::<Progress>::enroll(&b, b.create_collection()?, policy, policy.actors[1])?;
    left.update(|record, actor| {
        record.done.insert(1)?;
        record.count.increment(actor, 3)?;
        record.goal.assign(actor, 10)
    })?;
    right.update(|record, actor| {
        record.done.insert(2)?;
        record.count.increment(actor, 4)?;
        record.goal.assign(actor, 20)
    })?;
    let (original_a, original_b) = (left.read()?, right.read()?);
    let (mut merged_a, mut merged_b) = (original_a, original_b);
    merged_a.merge(original_b);
    merged_b.merge(original_a);
    assert_eq!(merged_a, merged_b);
    assert_eq!(merged_a.count.total(), 7);
    assert!(merged_a.done.contains(1) && merged_a.done.contains(2));
    sync(&left, &right)?;
    sync(&right, &left)?;
    let committed = left.read()?;
    assert_eq!(committed, right.read()?);
    assert_eq!(committed.goal.values().collect::<Vec<_>>(), [20]);
    assert_eq!(committed.count.total(), 7);
    assert!(committed.done.contains(1) && committed.done.contains(2));
    let revisions = (a.snapshot().revision(), b.snapshot().revision());
    sync(&left, &right)?;
    sync(&right, &left)?;
    assert_eq!(
        revisions,
        (a.snapshot().revision(), b.snapshot().revision())
    );
    let mut wire = vec![0; Progress::SIZE];
    committed.encode(&mut wire)?;
    assert_eq!(Progress::decode(&wire)?, committed);
    println!(
        "Private copied-source Rust consumer: typed Model/index/update, CRDT Record/Resolution committed sync, repeat-sync and wire roundtrip passed"
    );
    Ok(())
}
