use std::collections::{BTreeMap, BTreeSet};
use weekend_db::{
    CollectionId, Database, Error, Rows, Snapshot, Transaction, TransactionState, Validation,
};

type Records = BTreeMap<u8, i64>;
type Catalog = [Option<Records>; 2];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Access {
    collection: usize,
    key: Option<u8>, // None denotes the whole collection.
}

impl Access {
    fn overlaps(self, other: Self) -> bool {
        self.collection == other.collection
            && (self.key.is_none() || other.key.is_none() || self.key == other.key)
    }
}

struct Draft<'a> {
    tx: Transaction<'a>,
    mode: Validation,
    start: usize,
    base: Catalog,
    view: Catalog,
    observed: BTreeSet<Access>,
    edits: BTreeSet<(usize, u8)>,
}

impl<'a> Draft<'a> {
    fn begin(db: &'a Database, mode: Validation, head: &Catalog, start: usize) -> Self {
        Self {
            tx: db.begin_with_validation(mode),
            mode,
            start,
            base: head.clone(),
            view: head.clone(),
            observed: BTreeSet::new(),
            edits: BTreeSet::new(),
        }
    }

    fn commit(self, head: &mut Catalog, history: &mut Vec<Vec<Access>>, initial: u64) {
        let Self {
            mut tx,
            mode,
            start,
            view,
            observed,
            edits,
            ..
        } = self;
        let since_begin = &history[start..];
        let conflict = (mode == Validation::WholeDatabase && !since_begin.is_empty())
            || since_begin
                .iter()
                .flatten()
                .any(|change| observed.iter().any(|read| read.overlaps(*change)));
        let expected = if conflict {
            Err(Error::Conflict)
        } else {
            let mut changes = Vec::new();
            for (c, key) in edits {
                let current = head[c].as_mut().unwrap();
                match view[c].as_ref().unwrap().get(&key) {
                    Some(&value) => {
                        current.insert(key, value);
                    }
                    None => {
                        current.remove(&key);
                    }
                }
                changes.push(Access {
                    collection: c,
                    key: Some(key),
                });
            }
            history.push(changes);
            Ok(initial + history.len() as u64)
        };
        assert_eq!(tx.commit(), expected);
        assert_eq!(
            tx.state(),
            if conflict {
                TransactionState::Aborted
            } else {
                TransactionState::Committed
            }
        );
        assert_eq!(tx.commit(), Err(Error::Closed));
    }
}

fn rows(actual: Rows) -> Vec<(u8, i64)> {
    actual
        .into_iter()
        .map(|(key, value)| {
            assert_eq!(key.len(), 1);
            (key[0], value.as_i64().unwrap())
        })
        .collect()
}

const SEEDED: u64 = 3;
const START: u64 = 4;

fn check_snapshot(
    snapshot: &Snapshot<'_>,
    ids: [CollectionId; 2],
    model: &Catalog,
    revision: u64,
    data_revision: u64,
) {
    assert_eq!(snapshot.revision(), revision);
    assert_eq!(snapshot.data_revision(), data_revision);
    for (i, id) in ids.into_iter().enumerate() {
        match &model[i] {
            None => assert!(matches!(snapshot.scan(id), Err(Error::NotFound))),
            Some(records) => assert!(
                rows(snapshot.scan(id).unwrap())
                    .into_iter()
                    .eq(records.iter().map(|(&k, &v)| (k, v)))
            ),
        }
    }
}

fn check_head(db: &Database, ids: [CollectionId; 2], head: &Catalog, history: &[Vec<Access>]) {
    let data = history
        .iter()
        .rposition(|changes| !changes.is_empty())
        .map_or(SEEDED, |index| START + index as u64 + 1);
    check_snapshot(
        &db.snapshot(),
        ids,
        head,
        START + history.len() as u64,
        data,
    );
}

pub fn txn_schedule(input: &[u8]) {
    if input.len() < 3 {
        return;
    }
    let db = Database::new().unwrap();
    let ids = std::array::from_fn(|_| db.create_collection().unwrap());
    let seeded = [
        Some(Records::from([(0, 10), (1, 11)])),
        Some(Records::new()),
    ];
    let mut setup = db.begin();
    for (&key, &value) in seeded[0].as_ref().unwrap() {
        setup
            .put(ids[0], &[key], &db.integer(value).unwrap())
            .unwrap();
    }
    assert_eq!(setup.commit(), Ok(SEEDED));
    let pinned = db.snapshot();
    let retained = db.retain_snapshot(&pinned).unwrap();
    let mut head = seeded.clone();
    let mut history = Vec::new();
    let mut drafts: [Option<Draft<'_>>; 3] = [None, None, None];

    for &[control, key, value] in input.as_chunks::<3>().0.iter().take(32) {
        let index = usize::from(control & 3) % drafts.len();
        let c = usize::from((control >> 2) & 1);
        let id = ids[c];
        let key = key % 4;
        let slot = &mut drafts[index];
        match (control >> 4) & 7 {
            0 if slot.is_none() => {
                let mode = if control & 8 != 0 {
                    Validation::WholeDatabase
                } else {
                    Validation::Records
                };
                *slot = Some(Draft::begin(&db, mode, &head, history.len()));
            }
            op @ 1..=4 => {
                let Some(draft) = slot else { continue };
                let tx = &mut draft.tx;
                let base = &draft.base[c];
                let Some(view) = &mut draft.view[c] else {
                    assert_eq!(tx.get(id, &[key]), Err(Error::NotFound));
                    continue;
                };
                let point = op != 4 && base.as_ref().unwrap().contains_key(&key);
                draft.observed.insert(Access {
                    collection: c,
                    key: point.then_some(key),
                });
                match op {
                    1 => assert_eq!(
                        tx.get(id, &[key]).unwrap().map(|v| v.as_i64().unwrap()),
                        view.get(&key).copied()
                    ),
                    2 => {
                        let value = i64::from(value as i8);
                        tx.put(id, &[key], &db.integer(value).unwrap()).unwrap();
                        view.insert(key, value);
                        draft.edits.insert((c, key));
                    }
                    3 => {
                        tx.delete(id, &[key]).unwrap();
                        view.remove(&key);
                        draft.edits.insert((c, key));
                    }
                    _ => assert!(
                        rows(tx.scan(id).unwrap())
                            .into_iter()
                            .eq(view.iter().map(|(&k, &v)| (k, v)))
                    ),
                }
            }
            5 => {
                if let Some(draft) = slot.take() {
                    draft.commit(&mut head, &mut history, START);
                }
            }
            6 => {
                if head[c].take().is_some() {
                    db.drop_collection(id).unwrap();
                    history.push(vec![Access {
                        collection: c,
                        key: None,
                    }]);
                } else {
                    assert_eq!(db.drop_collection(id), Err(Error::NotFound));
                }
            }
            7 => {
                if let Some(mut draft) = slot.take() {
                    assert_eq!(draft.tx.abort(), Ok(()));
                    assert_eq!(draft.tx.commit(), Err(Error::Closed));
                }
            }
            _ => {}
        }
        check_head(&db, ids, &head, &history);
    }
    for draft in drafts.into_iter().flatten() {
        draft.commit(&mut head, &mut history, START);
        check_head(&db, ids, &head, &history);
    }
    check_snapshot(&pinned, ids, &seeded, SEEDED, SEEDED);
    check_snapshot(
        &db.retained_snapshot(retained).unwrap(),
        ids,
        &seeded,
        SEEDED,
        SEEDED,
    );
}

#[test]
fn checked_in_schedules_replay() {
    for seed in [
        include_bytes!("../seeds/txn_schedule/independent-edits"),
        include_bytes!("../seeds/txn_schedule/present-delete-conflict"),
        include_bytes!("../seeds/txn_schedule/absence-noop-conflict"),
        include_bytes!("../seeds/txn_schedule/empty-scan-drop"),
        include_bytes!("../seeds/txn_schedule/whole-root-empty-commit"),
        include_bytes!("../seeds/txn_schedule/own-writes-scan"),
    ] {
        txn_schedule(seed);
    }
}
