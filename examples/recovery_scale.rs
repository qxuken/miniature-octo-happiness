//! Reproducible recovery scaling probe. Run with --release. Fixture generation
//! and backend image copying are outside the timed interval.
use std::{hint::black_box, time::Instant};
use weekend_db::{
    Database, Limits, Result,
    storage::{self, SimulatedDisk},
};
fn main() -> Result<()> {
    println!("records,image_bytes,samples,min_ns,median_ns,max_ns");
    let limits = Limits {
        max_entries: 16_000,
        max_writes: 16_000,
        max_observations: 16_000,
        ..Limits::default()
    };
    for records in [1000, 2000, 4000, 8000, 16000] {
        let disk = SimulatedDisk::new()?;
        let db = Database::create_on(disk.clone(), [7; 16], limits)?;
        let collection = db.create_collection()?;
        let mut tx = db.begin();
        for key in 0..records as u64 {
            tx.put(collection, &key.to_be_bytes(), &db.unsigned(key)?)?;
        }
        assert_eq!(tx.commit()?, 2);
        drop(tx);
        drop(db);
        let image = disk.durable_image()?;
        let mut elapsed = Vec::new();
        for _ in 0..5 {
            let source = SimulatedDisk::from_image(&image)?;
            let start = Instant::now();
            let info = black_box(storage::inspect_on(source, limits)?);
            elapsed.push(start.elapsed().as_nanos());
            assert_eq!(info.revision, 2);
        }
        elapsed.sort_unstable();
        println!(
            "{records},{},5,{},{},{}",
            image.len(),
            elapsed[0],
            elapsed[2],
            elapsed[4]
        );
        let source = SimulatedDisk::from_image(&image)?;
        let reopened = Database::open_on(source, limits)?;
        let rows = reopened.snapshot().scan(reopened.collection_id(1))?;
        assert_eq!(rows.len(), records);
        for (key, value) in rows {
            assert_eq!(value.as_u64()?, u64::from_be_bytes(key.try_into().unwrap()));
        }
    }
    Ok(())
}
