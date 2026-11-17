use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};
use weekend_db_typed::{Document, Store};
use weekend_tasks::{Task, Tasks};
const HELP: &str = "weekend-tasks [--db PATH] COMMAND

  init                         Create a new database (never overwrite)
  add SLUG TITLE               Add a task; quote titles containing spaces
  list [--snapshot ID]         List current or historical tasks
  show SLUG                    Show one task
  done|reopen SLUG             Set completion
  title|rename SLUG TEXT       Change title or unique slug
  tag SLUG TAG                 Add a tag
  note SLUG TEXT|--clear       Set or clear a note
  priority SLUG 0..5           Set priority
  remove SLUG                  Delete a task
  checkpoint                   Retain the current state; print its ID
  history                      List retained IDs and task counts
  undo ID                      Restore task data; retain a redo checkpoint
  forget ID                    Release one retained checkpoint
  upgrade                      Explicitly migrate older task records
  compact                      Compact the closed database (macOS/Linux)
  demo                         Run the portable in-memory demo

Default path: tasks.wdb. File storage supports macOS, Linux, and Windows NTFS.";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn fail(message: &str) -> Box<dyn std::error::Error> {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into()
}
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn open(path: &Path, create: bool) -> Result<Store> {
    Ok(if create {
        Store::create_file(path)?
    } else {
        Store::open_file(path)?
    })
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn open(_path: &Path, _create: bool) -> Result<Store> {
    Err(fail(
        "File storage requires macOS, Linux, or Windows NTFS. Run `weekend-tasks demo` here.",
    ))
}
fn print_task(task: &Document<Task>) {
    println!(
        "{}\t{}\t{}\tp{}\t{}",
        task.id(),
        if task.done { "[x]" } else { "[ ]" },
        task.slug,
        task.priority,
        task.title
    );
    if !task.tags.is_empty() {
        println!("  tags: {}", task.tags.join(", "));
    }
    if let Some(note) = &task.note {
        println!("  note: {note}");
    }
}
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut path = PathBuf::from("tasks.wdb");
    let mut first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--db")) {
        path = PathBuf::from(args.next().ok_or_else(|| fail("--db requires a path"))?);
        first = args.next();
    }
    let Some(first) = first else {
        println!("{HELP}");
        return Ok(());
    };
    let command = first
        .to_str()
        .ok_or_else(|| fail("command must be UTF-8"))?;
    let args: Vec<String> = args
        .map(|s| {
            s.into_string()
                .map_err(|_| fail("task arguments must be UTF-8"))
        })
        .collect::<Result<_>>()?;
    if matches!(command, "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if command == "demo" {
        if !args.is_empty() {
            return Err(fail("demo takes no arguments"));
        }
        weekend_tasks::demo()?;
        return Ok(());
    }
    if command == "compact" {
        if !args.is_empty() {
            return Err(fail("compact takes no arguments"));
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let stats = Store::compact_file(&path)?;
            println!(
                "Compacted: {} -> {} bytes",
                stats.before_bytes, stats.after_bytes
            );
            return Ok(());
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            return Err(fail("Compaction requires macOS or Linux."));
        }
    }
    let expected = match command {
        "init" | "checkpoint" | "history" | "upgrade" => 0,
        "show" | "done" | "reopen" | "remove" | "undo" | "forget" => 1,
        "add" | "title" | "rename" | "tag" | "note" | "priority" => 2,
        "list" => {
            if args.is_empty() {
                0
            } else {
                2
            }
        }
        _ => return Err(fail("Unknown command. Run `weekend-tasks help`.")),
    };
    if args.len() != expected {
        return Err(fail("Wrong number of arguments. Run `weekend-tasks help`."));
    }
    if command == "list" && !args.is_empty() && args[0] != "--snapshot" {
        return Err(fail("list accepts only --snapshot ID"));
    }
    let mut store = open(&path, command == "init")?;
    let tasks = Tasks::attach(&mut store)?;
    match command {
        "init" => println!("Created {}", path.display()),
        "add" => println!(
            "Added #{}",
            tasks.add(&store, args[0].clone(), args[1].clone())?
        ),
        "list" => {
            let values = if args.is_empty() {
                store.read(|r| r.list(&tasks.table))?
            } else {
                store.read_retained(args[1].parse()?, |r| r.list(&tasks.table))?
            };
            for task in values {
                print_task(&task);
            }
        }
        "show" => {
            let task = store
                .read(|r| r.find(&tasks.table, Task::SLUG, &args[0]))?
                .ok_or_else(|| fail("Task not found"))?;
            print_task(&task);
        }
        "done" | "reopen" => tasks.update(&store, &args[0], |t| {
            t.done = command == "done";
            Ok(())
        })?,
        "title" | "rename" => {
            if args[1].trim().is_empty() {
                return Err(fail("Title/slug cannot be empty"));
            }
            tasks.update(&store, &args[0], |t| {
                if command == "title" {
                    t.title = args[1].clone();
                } else {
                    t.slug = args[1].clone();
                }
                Ok(())
            })?;
        }
        "tag" => {
            if args[1].trim().is_empty() {
                return Err(fail("Tag cannot be empty"));
            }
            tasks.update(&store, &args[0], |t| {
                if !t.tags.contains(&args[1]) {
                    t.tags.push(args[1].clone());
                }
                Ok(())
            })?;
        }
        "note" => tasks.update(&store, &args[0], |t| {
            t.note = if args[1] == "--clear" {
                None
            } else {
                Some(args[1].clone())
            };
            Ok(())
        })?,
        "priority" => {
            let n: u8 = args[1].parse()?;
            if n > 5 {
                return Err(fail("Priority must be 0..5"));
            }
            tasks.update(&store, &args[0], |t| {
                t.priority = n;
                Ok(())
            })?;
        }
        "remove" => tasks.remove(&store, &args[0])?,
        "checkpoint" => println!("{}", store.checkpoint()?),
        "history" => {
            for id in store.history()? {
                let count = store.read_retained(id, |r| Ok(r.list(&tasks.table)?.len()))?;
                println!("{id}\t{count} tasks");
            }
        }
        "undo" => println!(
            "Restored checkpoint {}; redo checkpoint: {}",
            args[0],
            tasks.undo(&store, args[0].parse()?)?
        ),
        "forget" => store.release_checkpoint(args[0].parse()?)?,
        "upgrade" => println!(
            "Migrated {} tasks",
            store.write(|w| w.migrate(&tasks.table, |_, _| Ok(())))?
        ),
        _ => unreachable!(),
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            if e.downcast_ref::<weekend_db_typed::Error>()
                == Some(&weekend_db_typed::Error::MigrationRequired)
            {
                eprintln!(
                    "Run `weekend-tasks --db PATH upgrade` to migrate older tasks explicitly."
                );
            }
            ExitCode::FAILURE
        }
    }
}
