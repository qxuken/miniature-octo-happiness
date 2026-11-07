use weekend_db_typed::{Document, Error, Model, Result, Store, Table, engine};

#[derive(Model, Debug)]
#[weekend(type_id = 0x7461736b, version = 1)]
pub struct LegacyTask {
    #[weekend(id = 1, unique)]
    pub slug: String,
    #[weekend(id = 2)]
    pub title: String,
    #[weekend(id = 3)]
    pub done: bool,
    #[weekend(id = 4)]
    pub tags: Vec<String>,
}
#[derive(Model, Debug)]
#[weekend(type_id = 0x7461736b, version = 2)]
pub struct Task {
    #[weekend(id = 1, unique)]
    pub slug: String,
    #[weekend(id = 2)]
    pub title: String,
    #[weekend(id = 3)]
    pub done: bool,
    #[weekend(id = 4)]
    pub tags: Vec<String>,
    #[weekend(id = 5)]
    pub note: Option<String>,
    #[weekend(id = 6, default = 0)]
    pub priority: u8,
}
#[derive(Model)]
#[weekend(type_id = 0x7461736c, version = 1)]
struct Counter {
    #[weekend(id = 1)]
    next: u64,
}
pub struct Tasks {
    pub table: Table<Task>,
    counter: Table<Counter>,
}
impl Tasks {
    pub fn attach(store: &mut Store) -> Result<Self> {
        store.table::<LegacyTask>("tasks")?;
        let table = store.table::<Task>("tasks")?;
        let counter = store.table::<Counter>("task_ids")?;
        if store.read(|r| Ok(r.get(&counter, 1)?.is_none()))? {
            store.write(|w| {
                if w.get(&counter, 1)?.is_none() {
                    let next = w
                        .list(&table)?
                        .iter()
                        .map(Document::id)
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or(Error::Engine(engine::Error::Exhausted))?;
                    w.insert(&counter, 1, &Counter { next })?;
                }
                Ok(())
            })?;
        }
        Ok(Self { table, counter })
    }
    pub fn add(&self, store: &Store, slug: String, title: String) -> Result<u64> {
        if slug.trim().is_empty() || title.trim().is_empty() {
            return Err(Error::Schema(engine::schema::Error::InvalidValue));
        }
        store.write(|w| {
            let id = w.get(&self.counter, 1)?.ok_or(Error::InvalidCatalog)?.next;
            let next = id
                .checked_add(1)
                .ok_or(Error::Engine(engine::Error::Exhausted))?;
            w.update(&self.counter, 1, |counter| {
                counter.next = next;
                Ok(())
            })?;
            w.insert(
                &self.table,
                id,
                &Task {
                    slug,
                    title,
                    done: false,
                    tags: Vec::new(),
                    note: None,
                    priority: 0,
                },
            )?;
            Ok(id)
        })
    }
    pub fn update(
        &self,
        store: &Store,
        slug: &str,
        action: impl FnOnce(&mut Task) -> Result<()>,
    ) -> Result<()> {
        store.write(|w| {
            let id = w
                .find(&self.table, Task::SLUG, slug)?
                .ok_or(Error::Engine(engine::Error::NotFound))?
                .id();
            w.update(&self.table, id, action)
        })
    }
    pub fn remove(&self, store: &Store, slug: &str) -> Result<()> {
        store.write(|w| {
            let id = w
                .find(&self.table, Task::SLUG, slug)?
                .ok_or(Error::Engine(engine::Error::NotFound))?
                .id();
            w.delete(&self.table, id)?;
            Ok(())
        })
    }
    pub fn undo(&self, store: &Store, id: u64) -> Result<u64> {
        // Restore task data; keep the allocator advancing across delete/undo.
        store.read_retained(id, |r| r.list(&self.table))?;
        let backup = store.checkpoint()?;
        store.restore_table(&self.table, id)?;
        Ok(backup)
    }
}
pub fn demo() -> Result<()> {
    let mut store = Store::memory()?;
    let old = store.table::<LegacyTask>("tasks")?;
    store.write(|w| {
        w.insert(
            &old,
            1,
            &LegacyTask {
                slug: "learn".into(),
                title: "Read the database design".into(),
                done: false,
                tags: vec!["reading".into()],
            },
        )
    })?;
    let original = store.checkpoint()?;
    let tasks = Tasks::attach(&mut store)?;
    assert_eq!(
        store.write(|w| w.migrate(&tasks.table, |_, task| {
            task.priority = 2;
            Ok(())
        }))?,
        1
    );
    tasks.update(&store, "learn", |task| {
        task.done = true;
        task.note = Some("Unknown fields survive older models".into());
        Ok(())
    })?;
    store.write(|w| {
        w.update(&old, 1, |task| {
            task.title = "Read and build".into();
            Ok(())
        })
    })?;
    store.read(|r| {
        let task = r.get(&tasks.table, 1)?.unwrap();
        assert!(task.done);
        assert_eq!(task.priority, 2);
        assert!(task.note.is_some());
        Ok(())
    })?;
    let redo = tasks.undo(&store, original)?;
    assert!(!store.read(|r| Ok(r.get(&tasks.table, 1)?.unwrap().done))?);
    tasks.undo(&store, redo)?;
    assert_eq!(
        tasks.add(&store, "ship".into(), "Build something useful".into())?,
        2
    );
    println!(
        "Task demo passed: derive, CRUD, unique lookup, explicit migration, unknown fields, checkpoints and undo/redo."
    );
    Ok(())
}
