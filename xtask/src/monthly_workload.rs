//! Deterministic two-peer calendar workload with separate native process phases.
use crate::{
    artifact_trace::{docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, serialization_scale,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CString, OsString},
    fs::{self, File},
    io::{BufWriter, Write},
    net::TcpListener,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const DRIVER: &str = "tools/monthly-workload";
const FILES: [&str; 3] = [
    "tools/monthly_workload.rs",
    "tools/monthly-workload/Cargo.toml",
    "tools/monthly-workload/Cargo.lock",
];
const DAYS: u64 = 30;

#[derive(Args)]
pub struct Options {
    #[arg(long, alias = "output")]
    directory: PathBuf,
    #[arg(long, conflicts_with_all = ["source", "image", "live_budget"])]
    binary: Option<PathBuf>,
    #[arg(long)]
    source: Option<String>,
    #[arg(long, alias = "docker-image", conflicts_with_all = ["server_command", "server_db",
        "server_listen", "server_address", "client_listen", "client_address"])]
    image: Option<String>,
    #[arg(long)]
    live_budget: bool,
    #[arg(long, default_value = "working-tree")]
    engine_source: String,
    #[arg(long, requires = "server_db")]
    server_command: Option<String>,
    #[arg(long)]
    server_db: Option<String>,
    #[arg(long)]
    server_listen: Option<String>,
    #[arg(long)]
    server_address: Option<String>,
    #[arg(long)]
    client_listen: Option<String>,
    #[arg(long)]
    client_address: Option<String>,
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}
#[derive(Clone)]
struct Peer {
    prefix: Vec<OsString>,
    database: PathBuf,
    listen: String,
    address: String,
}
impl Peer {
    fn command(&self, role: usize, phase: &str, day: u64, extra: &[&str]) -> Vec<OsString> {
        let mut command = self.prefix.clone();
        command.extend([
            phase.into(),
            path(&self.database),
            role.to_string().into(),
            day.to_string().into(),
        ]);
        command.extend(extra.iter().map(OsString::from));
        command
    }
}

fn physical_space(directory: &Path) -> Result<u128> {
    let path = CString::new(directory.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // Valid NUL-terminated path and writable statvfs result. Read only on success.
    let status = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stats = unsafe { stats.assume_init() };
    Ok(u128::from(stats.f_bavail) * u128::from(stats.f_frsize))
}

fn child_max_rss_bytes() -> Result<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage writes the supplied struct on success.
    let status = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, usage.as_mut_ptr()) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let rss = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss)?;
    Ok(rss * if cfg!(target_os = "macos") { 1 } else { 1024 })
}

fn distribution(values: impl Iterator<Item = u64>) -> Value {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_unstable();
    let len = values.len();
    json!({
        "count": len,
        "min": values[0],
        "median": values[len / 2],
        "p95": values[95 * len / 100],
        "p99": values[99 * len / 100],
        "max": values[len - 1],
    })
}
fn command_sample(c: &Campaign<'_>, label: &str) -> Result<(f64, u64)> {
    let command = c.evidence["commands"]
        .as_array()
        .unwrap()
        .last()
        .context("missing sampled phase")?;
    ensure!(command["name"] == label, "sampled phase identity mismatch");
    let sample = &command["supervision"];
    Ok((
        sample["elapsed_seconds"]
            .as_f64()
            .context("missing phase wall time")?,
        sample["sampled_peak_rss_kib"]
            .as_u64()
            .context("missing phase RSS sample summary")?,
    ))
}
fn row(output: &str, role: usize, day: u64, phase: &str, elapsed: f64) -> Result<Value> {
    let last = output.lines().last().context("empty phase output")?;
    let mut value = delivery::json(last.as_bytes())?;
    ensure!(
        value["phase"] == phase
            && value["role"].as_u64() == Some(role as u64)
            && value["day"].as_u64() == Some(day)
            && value["elapsed_us"].is_u64(),
        "phase output identity or timing mismatch"
    );
    if phase == "day" {
        let interrupted = day.is_multiple_of(7);
        let transactions = value["transaction_us"]
            .as_array()
            .context("stale driver transaction samples")?;
        ensure!(
            transactions.len() == if interrupted { 1 } else { 2 }
                && transactions.iter().all(Value::is_u64)
                && value["commits"] == if interrupted { 1 } else { 3 }
                && value["conflicts"] == u8::from(!interrupted)
                && value["retries"] == u8::from(!interrupted),
            "daily receipt or write outcomes mismatch"
        );
    } else if phase == "backup" {
        ensure!(
            value["backup_us"].is_u64() && value["restore_us"].is_u64(),
            "stale driver backup timings"
        );
    } else if phase == "verify" {
        ensure!(
            value["reopen_us"].is_u64() && value["file_bytes"].as_u64().is_some_and(|v| v > 0),
            "verified file length or timing missing"
        );
    }
    value["process_wall_ms"] = json!((elapsed * 1_000_000.).round() / 1000.);
    Ok(value)
}

struct Schedule {
    peers: [Peer; 2],
    deadline: Instant,
    events: BufWriter<File>,
    rows: Vec<Value>,
    sampled_rss: u64,
}
impl Schedule {
    fn remaining(&self, limit: f64) -> Result<f64> {
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64();
        ensure!(remaining > 0., "month schedule deadline");
        Ok(limit.min(remaining))
    }
    fn save(&mut self, row: Value, peak: u64) -> Result<()> {
        self.sampled_rss = self.sampled_rss.max(peak);
        serde_json::to_writer(&mut self.events, &row)?;
        self.events.write_all(b"\n")?;
        self.events.flush()?;
        self.rows.push(row);
        Ok(())
    }
    fn phase(
        &mut self,
        c: &mut Campaign<'_>,
        role: usize,
        day: u64,
        phase: &str,
        extra: &[&str],
        label: &str,
    ) -> Result<Value> {
        let args = self.peers[role].command(role, phase, day, extra);
        let output = c.command(args, label, None, self.remaining(60.)?)?;
        let (elapsed, peak) = command_sample(c, label)?;
        let value = row(&output, role, day, phase, elapsed)?;
        self.save(value.clone(), peak)?;
        Ok(value)
    }
    fn exchange(
        &mut self,
        c: &mut Campaign<'_>,
        sender: usize,
        day: u64,
        repeat: bool,
    ) -> Result<()> {
        let receiver = 1 - sender;
        let receiver_peer = self.peers[receiver].clone();
        let label = format!(
            "{day}-{sender}-{}",
            if repeat { "repeat" } else { "exchange" }
        );
        let server_label = format!("{label}-serve");
        let client_label = format!("{label}-sync");
        let args = receiver_peer.command(receiver, "serve", day, &[&receiver_peer.listen]);
        let client = c.ready_process(
            args,
            &server_label,
            (Duration::from_secs(30), self.remaining(90.)?),
            true,
            |line| {
                ensure!(line == "READY", "monthly server readiness mismatch");
                Ok(())
            },
            |campaign, ()| {
                self.phase(
                    campaign,
                    sender,
                    day,
                    "sync",
                    &[&receiver_peer.address],
                    &client_label,
                )
            },
        )?;
        let (elapsed, peak) = command_sample(c, &server_label)?;
        let seen = fs::read_to_string(c.output.join(format!("{server_label}.stdout.log")))?;
        let server = row(&seen, receiver, day, "serve", elapsed)?;
        if repeat {
            ensure!(
                client["commits"] == 0 && server["commits"] == 0,
                "repeat sync committed"
            );
        }
        self.save(server, peak)?;
        Ok(())
    }
    fn run(mut self, c: &mut Campaign<'_>) -> Result<Value> {
        let started = Instant::now();
        let free_before = physical_space(&c.output)?;
        c.sample_rss = true;
        for role in [0, 1] {
            self.phase(c, role, 0, "init", &[], &format!("init-{role}"))?;
        }
        for day in 1..=DAYS {
            for role in [0, 1] {
                if day.is_multiple_of(7) {
                    let label = format!("day-{day}-{role}-interrupt");
                    let args = self.peers[role].command(role, "day", day, &["interrupt"]);
                    c.command_exit(args, &label, 70, self.remaining(60.)?)?;
                    let (elapsed, peak) = command_sample(c, &label)?;
                    self.save(json!({"phase":"interrupted-after-receipt","role":role,"day":day,"process_wall_ms":(elapsed*1_000_000.).round()/1000.}), peak)?;
                }
                self.phase(c, role, day, "day", &[], &format!("day-{day}-{role}"))?;
            }
            for repeat in [false, true] {
                for sender in [0, 1] {
                    self.exchange(c, sender, day, repeat)?;
                }
            }
            for role in [0, 1] {
                for phase in ["checkpoint", "verify"] {
                    self.phase(c, role, day, phase, &[], &format!("{phase}-{day}-{role}"))?;
                }
            }
        }
        for role in [0, 1] {
            self.phase(c, role, DAYS, "backup", &[], &format!("backup-{role}"))?;
        }
        let samples = self
            .rows
            .iter()
            .filter_map(|row| row["transaction_us"].as_array())
            .flat_map(|timings| timings.iter().filter_map(Value::as_u64));
        let transactions = distribution(samples);
        ensure!(
            transactions["count"] == 112 && self.rows.len() == 432,
            "month operation or timing sample count mismatch"
        );
        let latency = |phase: &str, key: &str| {
            distribution(
                self.rows
                    .iter()
                    .filter(|v| v["phase"] == phase)
                    .filter_map(|v| v[key].as_u64()),
            )
        };
        let latency_us = ["day", "sync", "serve", "checkpoint", "verify", "backup"]
            .into_iter()
            .map(|phase| (phase, latency(phase, "elapsed_us")))
            .collect::<BTreeMap<_, _>>();
        write_csv(&c.output.join("events.csv"), &self.rows)?;
        c.sample_rss = false;
        Ok(json!({
            "calendar_days":DAYS,"wall_seconds":started.elapsed().as_secs_f64(),
            "kind":"accelerated deterministic calendar",
            "scheduled":{"task_edits":480,"conflicts":60,"retries":60,"offline_crdt_updates":60,
                "daily_checkpoints":60,"interrupted_processes":8,"sync_sessions":120},
            "final_oracle":{"counter":1830,"completed_ids":(0..60).collect::<Vec<_>>(),"weekly_goal":120,
                "session_limit":20,"task_score_formula":"3000 + 10*role + id, id=1..8","retained_roots_per_node":30},
            "latency_us":latency_us,
            "core_memory_by_phase":self.rows.iter().filter_map(|row| {
                let mut usage = row["memory_usage"].as_object()?.clone();
                for name in ["role","day","phase"] { usage.insert(name.into(),row[name].clone()); }
                Some(usage)
            }).collect::<Vec<_>>(),
            "memory_scope":"requested layouts per core domain",
            "reopen_us":latency("verify","reopen_us"),"successful_transaction_us":transactions,
            "backup_us":latency("backup","backup_us"),"restore_us":latency("backup","restore_us"),
            "file_bytes_by_day":self.rows.iter().filter(|row| row["phase"]=="verify").map(|row|
                json!([row["role"],row["day"],row["file_bytes"]])).collect::<Vec<_>>(),
            "local_disk_free_before":free_before,"local_disk_free_after":physical_space(&c.output)?,
            "child_max_rss_bytes":child_max_rss_bytes()?, "sampled_child_max_rss_bytes":self.sampled_rss*1024,
            "rss_scope":"host child high-water mark includes setup; sampled RSS covers workload commands; container descendants excluded",
            "ack_scope":"CRDT request/reply",
        }))
    }
}

fn write_csv(path: &Path, rows: &[Value]) -> Result<()> {
    let keys = rows
        .iter()
        .filter_map(Value::as_object)
        .flat_map(|row| row.keys())
        .collect::<BTreeSet<_>>();
    let mut output = BufWriter::new(File::create(path)?);
    writeln!(
        output,
        "{}",
        keys.iter()
            .map(|key| key.as_str())
            .collect::<Vec<_>>()
            .join(",")
    )?;
    for row in rows {
        let mut columns = Vec::new();
        for key in &keys {
            let value = &row[key.as_str()];
            let encoded = match value {
                Value::Null => String::new(),
                Value::String(value) => value.clone(),
                _ => value.to_string(),
            };
            columns.push(if encoded.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", encoded.replace('"', "\"\""))
            } else {
                encoded
            });
        }
        writeln!(output, "{}", columns.join(","))?;
    }
    output.flush()?;
    Ok(())
}

fn local_peers(c: &Campaign<'_>, options: &Options, binary: &Path) -> Result<[Peer; 2]> {
    let a = TcpListener::bind("127.0.0.1:0")?;
    let b = TcpListener::bind("127.0.0.1:0")?;
    let addresses = [a.local_addr()?.to_string(), b.local_addr()?.to_string()];
    let local = vec![path(binary)];
    let prefix = if let Some(spec) = &options.server_command {
        let args: Vec<String> = serde_json::from_str(spec)?;
        ensure!(
            !args.is_empty() && !args[0].is_empty(),
            "nonempty server argv required"
        );
        args.into_iter().map(Into::into).collect()
    } else {
        local.clone()
    };
    let server_listen = options
        .server_listen
        .clone()
        .unwrap_or_else(|| addresses[0].clone());
    let client_listen = options
        .client_listen
        .clone()
        .unwrap_or_else(|| addresses[1].clone());
    Ok([
        Peer {
            prefix,
            database: options
                .server_db
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| c.output.join("server.wdb")),
            address: options
                .server_address
                .clone()
                .unwrap_or_else(|| server_listen.clone()),
            listen: server_listen,
        },
        Peer {
            prefix: local,
            database: c.output.join("client.wdb"),
            address: options
                .client_address
                .clone()
                .unwrap_or_else(|| client_listen.clone()),
            listen: client_listen,
        },
    ])
}

fn workload(c: &mut Campaign<'_>, peers: [Peer; 2], revision: &str, binary: &Path) -> Result<()> {
    let commands = peers.iter().map(|p| &p.prefix).collect::<Vec<_>>();
    let databases = peers.iter().map(|p| &p.database).collect::<Vec<_>>();
    let addresses = peers.iter().map(|p| &p.address).collect::<Vec<_>>();
    let binding = json!({"declared_engine_source":revision,"commands":commands,"paths":databases,"addresses":addresses,
        "local_binary_sha256":sha(binary)?});
    c.evidence["workload_inputs"] = binding.clone();
    c.save()?;
    let mut summary = Schedule {
        peers,
        deadline: Instant::now() + Duration::from_secs(900),
        events: BufWriter::new(File::create(c.output.join("events.jsonl"))?),
        rows: Vec::new(),
        sampled_rss: 0,
    }
    .run(c)?;
    summary["source"] = c.evidence["source"].clone();
    summary["working_tree_dirty"] = c.evidence["dirty"].clone();
    summary["source_bindings"] = c.evidence["driver_overlay"].clone();
    summary["local_toolchain"] = c.evidence["local_toolchain"].clone();
    let Value::Object(fields) = binding else {
        anyhow::bail!("missing month workload inputs")
    };
    summary.as_object_mut().unwrap().extend(fields);
    summary["passed"] = json!(true);
    let mut file = File::create(c.output.join("summary.json"))?;
    serde_json::to_writer_pretty(&mut file, &summary)?;
    file.write_all(b"\n")?;
    c.evidence["summary_sha256"] = json!(sha(&c.output.join("summary.json"))?);
    Ok(())
}

fn docker(
    c: &mut Campaign<'_>,
    source: &Path,
    revision: &str,
    image: &str,
    binary: &Path,
    features: &[&str],
) -> Result<()> {
    let image = c.command(
        cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
        "inspect-image",
        None,
        30.,
    )?;
    ensure!(
        image.len() == 71
            && image.starts_with("sha256:")
            && image[7..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "immutable local image required"
    );
    let name = format!("weekend-month-{}", nonce()?);
    let volume = format!("{name}-data");
    let registry = toolchain::cargo_home(&c.run.root, &c.env)?.join("registry");
    c.evidence["docker"] =
        json!({"image":image,"container":name,"volume":volume,"cleanup_verified":false});
    c.save()?;
    let result = (|| -> Result<()> {
        c.command(
            cmd(["docker", "volume", "create", &volume]),
            "create-volume",
            None,
            30.,
        )?;
        let start = cmd([
            "docker",
            "run",
            "--pull=never",
            "-d",
            "--name",
            &name,
            "--memory=2g",
            "--cpus=2",
            "--pids-limit=256",
            "--publish",
            "127.0.0.1::8400",
            "--mount",
            &format!("type=volume,source={volume},target=/data"),
            "--mount",
            &format!(
                "type=bind,source={},target=/source,readonly",
                source.display()
            ),
            "--mount",
            &format!(
                "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                registry.display()
            ),
            &image,
            "tail",
            "-f",
            "/dev/null",
        ]);
        c.command(start, "start-container", None, 60.)?;
        let mut build = cmd([
            "docker",
            "exec",
            "--workdir",
            "/source",
            "-e",
            &format!("WEEKEND_DB_SOURCE_ID={revision}"),
            "-e",
            "CARGO_INCREMENTAL=0",
            &name,
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
            "--manifest-path",
            "/source/tools/monthly-workload/Cargo.toml",
            "--target-dir",
            "/build",
        ]);
        build.extend(cmd(features));
        c.command(build, "build-linux", None, 180.)?;
        let architecture = c.command(
            cmd(["docker", "exec", &name, "uname", "-a"]),
            "linux-platform",
            None,
            30.,
        )?;
        let compiler = c.command(
            cmd(["docker", "exec", &name, "rustc", "-Vv"]),
            "linux-rustc",
            None,
            30.,
        )?;
        let filesystem = c.command(
            cmd([
                "docker",
                "exec",
                &name,
                "findmnt",
                "-T",
                "/data",
                "-n",
                "-o",
                "SOURCE,FSTYPE,OPTIONS",
            ]),
            "linux-filesystem",
            None,
            30.,
        )?;
        ensure!(
            filesystem.split_ascii_whitespace().nth(1) == Some("ext4"),
            "Linux month requires native ext4 data"
        );
        let sha = c.command(
            cmd([
                "docker",
                "exec",
                &name,
                "sha256sum",
                "/build/release/monthly-workload",
            ]),
            "linux-binary",
            None,
            30.,
        )?;
        let digest = sha
            .split_ascii_whitespace()
            .next()
            .context("missing remote executable hash")?;
        ensure!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid remote executable hash"
        );
        c.evidence["docker"]["linux"] = json!({"platform":architecture,"compiler":compiler,"filesystem":filesystem,"binary_sha256":digest});
        fs::write(
            c.output.join("linux-platform.txt"),
            format!("{architecture}\n{compiler}\n{filesystem}\n{sha}\n"),
        )?;
        let address: std::net::SocketAddr = c
            .command(
                cmd(["docker", "port", &name, "8400/tcp"]),
                "linux-host-port",
                None,
                30.,
            )?
            .parse()?;
        ensure!(
            address.ip().is_loopback() && address.port() != 0,
            "unexpected published Docker endpoint"
        );
        let client_port = free_port()?;
        let peers = [
            Peer {
                prefix: cmd(["docker", "exec", &name, "/build/release/monthly-workload"]),
                database: "/data/server.wdb".into(),
                listen: "0.0.0.0:8400".into(),
                address: address.to_string(),
            },
            Peer {
                prefix: vec![path(binary)],
                database: c.output.join("client.wdb"),
                listen: format!("0.0.0.0:{client_port}"),
                address: format!("host.docker.internal:{client_port}"),
            },
        ];
        let qualified = workload(c, peers, revision, binary);
        let copy = c.command(
            vec![
                "docker".into(),
                "cp".into(),
                format!("{name}:/data").into(),
                path(c.output.join("linux-data")),
            ],
            "copy-linux-data",
            None,
            180.,
        );
        qualified?;
        copy?;
        Ok(())
    })();
    let cleanup = docker_retire(
        c,
        &name,
        &volume,
        [
            cmd(["docker", "rm", "-f", &name]),
            cmd(["docker", "volume", "rm", &volume]),
        ],
    );
    c.evidence["docker"]["cleanup_verified"] = json!(cleanup.is_ok());
    cleanup?;
    result
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        options.image.is_none() || cfg!(target_os = "macos"),
        "Docker cross-OS month requires a native macOS client"
    );
    let mut c = Campaign::new(
        run,
        &options.directory,
        "accelerated 30-day reversible storage and peer workload",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    c.env = serialization_scale::clean_env(&c.env);
    c.save()?;
    let result = (|| -> Result<()> {
        c.evidence["local_toolchain"] =
            json!(c.command(cmd(["rustc", "-Vv"]), "local-rustc", None, 30.)?);
        if let Some(binary) = &options.binary {
            let binary = dunce::canonicalize(binary)?;
            let original = sha(&binary)?;
            let declared = FILES
                .into_iter()
                .map(|file| Ok((file, sha(&run.root.join(file))?)))
                .collect::<Result<BTreeMap<_, _>>>()?;
            c.evidence["driver_overlay"] = json!(declared);
            let peers = local_peers(&c, &options, &binary)?;
            workload(&mut c, peers, &options.engine_source, &binary)?;
            ensure!(
                sha(&binary)? == original,
                "prebuilt month executable changed"
            );
        } else {
            let (revision, source) =
                assurance::archive_source(&mut c, options.source.as_deref().unwrap_or("HEAD"))?;
            let mut overlay = BTreeMap::new();
            for file in FILES {
                let destination = source.join(file);
                fs::create_dir_all(destination.parent().unwrap())?;
                fs::copy(run.root.join(file), &destination)?;
                overlay.insert(file, sha(&destination)?);
            }
            let bound = inventory(&source, &[])?;
            c.evidence["engine_source_sha256"] = json!(bound);
            c.evidence["driver_overlay"] = json!(overlay);
            let features = if options.live_budget {
                &["--features", "live-budget"][..]
            } else {
                &[]
            };
            c.evidence["driver_features"] = json!(features);
            c.env
                .insert("WEEKEND_DB_SOURCE_ID".into(), revision.clone().into());
            let target = c.output.join("build");
            let mut build = cmd([
                "cargo",
                "build",
                "--offline",
                "--locked",
                "--release",
                "--manifest-path",
            ]);
            build.extend([
                path(source.join(DRIVER).join("Cargo.toml")),
                "--target-dir".into(),
                path(&target),
            ]);
            build.extend(cmd(features));
            c.command_at(build, "build-local", None, 180., &source)?;
            let binary = target.join("release/monthly-workload");
            let original = sha(&binary)?;
            if let Some(image) = &options.image {
                docker(&mut c, &source, &revision, image, &binary, features)?;
            } else {
                let peers = local_peers(&c, &options, &binary)?;
                workload(&mut c, peers, &revision, &binary)?;
            }
            ensure!(
                inventory(&source, &[])? == bound && sha(&binary)? == original,
                "frozen month source or binary changed"
            );
        }
        c.evidence["artifacts"] =
            json!(inventory(&c.output, &["source", "build", "evidence.json"])?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revised_day_records_distinguish_receipt_resume_from_full_replay() {
        let regular = json!({"phase":"day","role":1,"day":13,"elapsed_us":6,
            "transaction_us":[1,2],"commits":3,"conflicts":1,"retries":1});
        let resume = json!({"phase":"day","role":1,"day":14,"elapsed_us":7,
            "transaction_us":[3],"commits":1,"conflicts":0,"retries":0});
        assert!(row(&regular.to_string(), 1, 13, "day", 0.100).is_ok());
        assert!(row(&resume.to_string(), 1, 14, "day", 0.101).is_ok());
        assert!(row(&resume.to_string(), 0, 14, "day", 0.101).is_err());
        let mut replay = regular;
        replay["day"] = json!(14);
        assert!(row(&replay.to_string(), 1, 14, "day", 0.101).is_err());
    }

    #[test]
    fn trailing_phase_json_and_original_quantile_indices_are_preserved() {
        let backup =
            json!({"phase":"backup","role":1,"day":30,"elapsed_us":3,"backup_us":1,"restore_us":2});
        let value = row(&format!("READY\n{backup}"), 1, 30, "backup", 0.031_234).unwrap();
        assert_eq!(value["process_wall_ms"], 31.234);
        assert_eq!(
            distribution([11, 3, 6, 5].into_iter()),
            json!({"count":4,"min":3,"median":6,"p95":11,"p99":11,"max":11}),
        );
        let ordered = distribution((0..200).rev());
        assert_eq!(ordered["p95"], 190);
        assert_eq!(ordered["p99"], 198);
        assert_eq!(ordered["max"], 199);
        let mut stale = backup;
        stale.as_object_mut().unwrap().remove("restore_us");
        assert!(row(&stale.to_string(), 1, 30, "backup", 0.031_234).is_err());
    }
}
