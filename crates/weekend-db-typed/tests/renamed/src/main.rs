use db::{Model, Result, Store};
#[derive(Model)]
#[weekend(crate = "::db", type_id = 12, version = 1)]
struct Renamed {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    version: u64,
}
fn main() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<Renamed>("renamed")?;
    store.write(|w| {
        w.insert(
            &table,
            1,
            &Renamed {
                slug: "works".into(),
                version: 9,
            },
        )
    })?;
    store.read(|r| {
        assert_eq!(r.find(&table, Renamed::SLUG, "works")?.unwrap().version, 9);
        Ok(())
    })
}
