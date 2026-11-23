//! Authenticated offline edits, repeat sync and process interruption on two hosts.
use crate::{
    artifact_trace::{docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    interactive::Session,
    serialization_scale,
    supervise::Report,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

const COLLECTION: &str = "0123456789abcdef0123456789abcdef";

#[derive(Args)]
pub struct Options {
    output: PathBuf,
    #[arg(long, default_value = "HEAD")]
    source: String,
    #[arg(long, default_value = crate::toolchain::RUST_BOOKWORM_IMAGE)]
    image: String,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=365))]
    rounds: u64,
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..=365))]
    restart_every: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    revision: u64,
    total: u64,
    goals: Vec<u64>,
    limits: Vec<u64>,
    lessons: Vec<u64>,
}
impl State {
    fn parse(line: &str) -> Result<Self> {
        static GRAMMAR: OnceLock<Regex> = OnceLock::new();
        let grammar = GRAMMAR.get_or_init(|| {
            Regex::new(r"^state ([0-9]+) ([0-9]+) (\[[0-9, ]*\]) (\[[0-9, ]*\]) (\[[0-9, ]*\])$")
                .unwrap()
        });
        let fields = grammar
            .captures(line)
            .context("invalid study state reply")?;
        Ok(Self {
            revision: fields[1].parse()?,
            total: fields[2].parse()?,
            goals: serde_json::from_str(&fields[3])?,
            limits: serde_json::from_str(&fields[4])?,
            lessons: serde_json::from_str(&fields[5])?,
        })
    }
    fn matches(&self, expected: &(u64, u64, u64, Vec<u64>)) -> bool {
        self.total == expected.0
            && self.goals == [expected.1]
            && self.limits == [expected.2]
            && self.lessons == expected.3
    }
    fn json(&self) -> Value {
        json!([
            self.revision,
            self.total,
            self.goals,
            self.limits,
            self.lessons
        ])
    }
}

fn peer_ack(line: &str) -> Result<(u64, Option<u64>)> {
    let (n, reply) = line
        .strip_prefix("peer ")
        .context("missing peer reply")?
        .split_once(' ')
        .context("missing peer status")?;
    ensure!(
        !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()),
        "invalid peer count"
    );
    let count = n.parse()?;
    let ack = if let Some(value) = reply.strip_prefix("ack ") {
        ensure!(
            !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()),
            "invalid peer acknowledgment"
        );
        Some(value.parse()?)
    } else {
        ensure!(
            reply == "idle"
                || reply
                    .strip_prefix("error ")
                    .is_some_and(|value| !value.is_empty()),
            "invalid peer status"
        );
        None
    };
    Ok((count, ack))
}

struct Peer {
    label: String,
    session: Session,
}
impl Peer {
    fn start(c: &mut Campaign<'_>, args: Vec<OsString>, label: &str, port: u16) -> Result<Self> {
        let request = c.request(args, label, 14_400., &c.run.root)?;
        c.evidence["peer_commands"][label] = json!(request.argv);
        c.save()?;
        let mut session = Session::start(request)?;
        let readiness = session.line()?;
        let address: SocketAddr = readiness
            .strip_prefix("ready ")
            .context("study peer did not report readiness")?
            .parse()?;
        ensure!(address.port() == port, "study peer readiness port mismatch");
        Ok(Self {
            label: label.to_owned(),
            session,
        })
    }
    fn state(&mut self) -> Result<State> {
        State::parse(&self.session.command("show")?)
    }
    fn ack(&mut self) -> Result<(u64, Option<u64>)> {
        peer_ack(&self.session.command("peer")?)
    }
    fn command(&mut self, command: &str) -> Result<String> {
        self.session.command(command)
    }
    fn close(mut self, c: &mut Campaign<'_>, mode: Exit) -> Result<()> {
        if mode == Exit::Graceful {
            self.session.send("quit")?;
        }
        let report = self.session.close(mode == Exit::NativeKill)?;
        c.evidence["peer_exits"][&self.label] = json!(report);
        c.save()?;
        ensure!(
            mode.accepts(&report),
            "unexpected peer terminal status in {}",
            self.label
        );
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Graceful,
    NativeKill,
    ContainerKill,
}
impl Exit {
    fn accepts(self, report: &Report) -> bool {
        match self {
            Self::Graceful => report.success(),
            Self::NativeKill => report.stopped_cleanly(),
            Self::ContainerKill => {
                report.cleanup_verified
                    && report.cleanup_failure.is_none()
                    && !report.timed_out
                    && !report.cancelled
                    && report.returncode == Some(137)
                    && report.failure.as_deref() == Some("child exit Some(137)")
            }
        }
    }
}

fn states(peers: &mut [Peer; 2]) -> Result<[State; 2]> {
    Ok([peers[0].state()?, peers[1].state()?])
}
fn acks(peers: &mut [Peer; 2]) -> Result<[(u64, Option<u64>); 2]> {
    Ok([peers[0].ack()?, peers[1].ack()?])
}

fn paused(peers: &mut [Peer; 2]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut counts = acks(peers)?.map(|(count, _)| count);
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline {
        let latest = acks(peers)?.map(|(count, _)| count);
        if latest != counts {
            counts = latest;
            quiet_since = Instant::now();
        } else if quiet_since.elapsed() >= Duration::from_millis(3500) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("outgoing study replies did not quiesce")
}

fn converge(
    peers: &mut [Peer; 2],
    expected: &(u64, u64, u64, Vec<u64>),
) -> Result<([State; 2], [u64; 2])> {
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut covering = Covering::default();
    while Instant::now() < deadline {
        let observed = states(peers)?;
        let replies = acks(peers)?;
        let counts = replies.map(|(count, _)| count);
        if covering.observe(&observed, replies, expected) {
            return Ok((observed, counts));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("study convergence and covering replies did not qualify")
}
#[derive(Default)]
struct Covering {
    revisions: Option<[u64; 2]>,
    baseline: [u64; 2],
}
impl Covering {
    fn observe(
        &mut self,
        states: &[State; 2],
        replies: [(u64, Option<u64>); 2],
        expected: &(u64, u64, u64, Vec<u64>),
    ) -> bool {
        if !states.iter().all(|state| state.matches(expected))
            || !replies.iter().all(|(_, ack)| *ack == Some(expected.0))
        {
            self.revisions = None;
            return false;
        }
        let revisions = [states[0].revision, states[1].revision];
        let counts = replies.map(|(count, _)| count);
        if self.revisions != Some(revisions) {
            self.revisions = Some(revisions);
            self.baseline = counts;
            return false;
        }
        counts
            .into_iter()
            .zip(self.baseline)
            .all(|(count, start)| count.saturating_sub(start) >= 2)
    }
}

fn log_event(c: &Campaign<'_>, event: &Value) -> Result<()> {
    let mut log = OpenOptions::new()
        .append(true)
        .create(true)
        .open(c.output.join("events.jsonl"))?;
    serde_json::to_writer(&mut log, event)?;
    log.write_all(b"\n")?;
    Ok(())
}

struct Topology<'a> {
    container: &'a str,
    volume: &'a str,
    image: &'a str,
    source: &'a Path,
    registry: &'a Path,
    binary: &'a Path,
    mac_port: u16,
    linux_port: u16,
}
impl Topology<'_> {
    fn start_container(&self, c: &mut Campaign<'_>, label: &str) -> Result<()> {
        c.command(
            cmd([
                "docker",
                "run",
                "--pull=never",
                "-d",
                "--name",
                self.container,
                "--memory=2g",
                "--cpus=2",
                "--pids-limit=256",
                "--publish",
                &format!("127.0.0.1:{}:{}", self.linux_port, self.linux_port),
                "--mount",
                &format!("type=volume,source={},target=/data", self.volume),
                "--mount",
                &format!(
                    "type=bind,source={},target=/src,readonly",
                    self.source.display()
                ),
                "--mount",
                &format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    self.registry.display()
                ),
                self.image,
                "tail",
                "-f",
                "/dev/null",
            ]),
            label,
            None,
            60.,
        )?;
        Ok(())
    }

    fn peers(&self, c: &mut Campaign<'_>, epoch: u64) -> Result<[Peer; 2]> {
        let callback = c.command(
            cmd([
                "docker",
                "exec",
                self.container,
                "getent",
                "ahostsv4",
                "host.docker.internal",
            ]),
            &format!("callback-{epoch}"),
            None,
            30.,
        )?;
        let address: Ipv4Addr = callback
            .split_ascii_whitespace()
            .next()
            .context("Docker callback address unavailable")?
            .parse()?;
        let a = Peer::start(
            c,
            vec![
                path(self.binary),
                "run".into(),
                path(c.output.join("a.db")),
                "a".into(),
                COLLECTION.into(),
                format!("0.0.0.0:{}", self.mac_port).into(),
                format!("127.0.0.1:{}", self.linux_port).into(),
                path(c.output.join("a-identity")),
                path(c.output.join("b-identity/cert.der")),
                "--no-push".into(),
            ],
            &format!("mac-peer-{epoch}"),
            self.mac_port,
        )?;
        let b = Peer::start(
            c,
            cmd([
                "docker",
                "exec",
                "-i",
                self.container,
                "/data/build/release/study-peer",
                "run",
                "/data/b.db",
                "b",
                COLLECTION,
                &format!("0.0.0.0:{}", self.linux_port),
                &format!("{address}:{}", self.mac_port),
                "/data/b-identity",
                "/data/a-cert.der",
                "--no-push",
            ]),
            &format!("linux-peer-{epoch}"),
            self.linux_port,
        )?;
        Ok([a, b])
    }

    fn restart(&self, c: &mut Campaign<'_>, peers: [Peer; 2], epoch: u64) -> Result<[Peer; 2]> {
        let [mac, linux] = peers;
        mac.close(c, Exit::NativeKill)?;
        c.command(
            cmd(["docker", "kill", "--signal=KILL", self.container]),
            &format!("kill-linux-{epoch}"),
            None,
            30.,
        )?;
        let status = c.command(
            cmd([
                "docker",
                "inspect",
                "--format={{.State.ExitCode}}",
                self.container,
            ]),
            &format!("inspect-kill-{epoch}"),
            None,
            30.,
        )?;
        ensure!(status == "137", "Linux container SIGKILL not observed");
        linux.close(c, Exit::ContainerKill)?;
        c.command(
            cmd(["docker", "rm", self.container]),
            &format!("retire-linux-{epoch}"),
            None,
            30.,
        )?;
        self.start_container(c, &format!("restart-linux-{epoch}"))?;
        self.peers(c, epoch)
    }

    fn workload(&self, c: &mut Campaign<'_>, options: &Options) -> Result<()> {
        let mut peers = self.peers(c, 0)?;
        paused(&mut peers)?;
        let mut total = 0;
        let mut lessons = BTreeSet::new();
        for round in 1..=options.rounds {
            ensure!(!c.cancelled(), "study peer soak cancelled");
            let before = states(&mut peers)?;
            let counts = acks(&mut peers)?.map(|(count, _)| count);
            for (actor, peer) in peers.iter_mut().enumerate() {
                let actor = actor as u64;
                let lesson = (round * 2 + actor) % 64;
                lessons.insert(lesson);
                let reply = peer.command(&format!(
                    "edit {lesson} {} {} {}",
                    round * (actor + 1),
                    100 * (actor + 1) + round,
                    20 - 10 * actor + round % 5
                ))?;
                ensure!(
                    reply == format!("local {}", before[actor as usize].revision + 1),
                    "local study commit revision mismatch"
                );
                ensure!(
                    peer.ack()?.0 == counts[actor as usize],
                    "paused peer acknowledged an offline edit"
                );
            }
            total += 3 * round;
            let local = states(&mut peers)?.map(|state| state.json());
            log_event(
                c,
                &json!({"phase":"local","round":round,"states":local,"prior_remote_ack_counts":counts}),
            )?;
            for peer in &mut peers {
                ensure!(
                    peer.command("push on")? == "push on",
                    "study push admission failed"
                );
            }
            let start = Instant::now();
            let expected = (
                total,
                200 + round,
                10 + round % 5,
                lessons.iter().copied().collect(),
            );
            let (stable, acknowledgments) = converge(&mut peers, &expected)?;
            for peer in &mut peers {
                ensure!(
                    peer.command("push off")? == "push off",
                    "study push suspension failed"
                );
            }
            paused(&mut peers)?;
            ensure!(
                states(&mut peers)? == stable,
                "study state changed after outgoing attempts quiesced"
            );
            let mut result = json!({"round":round,"states":stable.iter().map(State::json).collect::<Vec<_>>(),
                "remote_ack_counts":acknowledgments,"converge_repeat_sync_seconds":start.elapsed().as_secs_f64()});
            if round.is_multiple_of(options.restart_every) {
                peers = self.restart(c, peers, round)?;
                ensure!(
                    states(&mut peers)? == stable,
                    "acknowledged study state or revision lost on restart"
                );
                result["restart_verified"] = json!(true);
            }
            c.evidence["rounds"]
                .as_array_mut()
                .unwrap()
                .push(result.clone());
            result["phase"] = json!("converged");
            log_event(c, &result)?;
            c.save()?;
        }
        let [mac, linux] = peers;
        mac.close(c, Exit::Graceful)?;
        linux.close(c, Exit::Graceful)?;
        Ok(())
    }
}

fn admit(root: &Path, options: &Options) -> Result<PathBuf> {
    ensure!(
        (1..=365).contains(&options.rounds)
            && (1..=options.rounds).contains(&options.restart_every),
        "rounds must be 1..365 and restart-every must be within rounds"
    );
    assurance::fresh_output(root, &options.output)
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "study peer soak requires a native macOS client and Docker Linux"
    );
    let output = admit(&run.root, &options)?;
    let mut c = Campaign::new(run, &output, "authenticated cross-OS study peer soak")?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    c.evidence["verified"] = json!(false);
    c.evidence["rounds_requested"] = json!(options.rounds);
    c.evidence["restart_every"] = json!(options.restart_every);
    c.evidence["rounds"] = json!([]);
    c.evidence["credentials"] =
        json!("synthetic identities retained in the private evidence directory");
    c.limits(8 * 1024 * 1024, None)?;
    c.env = serialization_scale::clean_env(&c.env);
    c.save()?;
    let result = (|| -> Result<()> {
        let (revision, source) = assurance::archive_source(&mut c, &options.source)?;
        let source_hashes = inventory(&source, &[])?;
        c.evidence["engine_source_sha256"] = json!(source_hashes);
        let image = c.command(
            cmd([
                "docker",
                "image",
                "inspect",
                "--format={{.Id}}",
                &options.image,
            ]),
            "image",
            None,
            30.,
        )?;
        ensure!(
            image.len() == 71
                && image.starts_with("sha256:")
                && image[7..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "immutable local Docker image required"
        );
        let target = output.join("native-build");
        c.env
            .insert("WEEKEND_DB_SOURCE_ID".into(), revision.clone().into());
        let mut build = cmd([
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
            "-p",
            "study-peer",
            "--target-dir",
        ]);
        build.push(path(&target));
        c.command_at(build, "build-local", None, 180., &source)?;
        let binary = target.join("release/study-peer");
        let local_hash = sha(&binary)?;
        c.evidence["native_binary_sha256"] = json!(local_hash);
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-vV"]), "rustc", None, 30.)?);
        let native_fs = c.command(
            vec!["df".into(), "-P".into(), path(&output)],
            "native-filesystem",
            None,
            30.,
        )?;
        let device = native_fs
            .lines()
            .last()
            .and_then(|line| line.split_ascii_whitespace().next())
            .context("macOS native volume")?;
        c.evidence["native_volume"] = json!(c.command(
            cmd(["diskutil", "info", device]),
            "native-volume",
            None,
            30.
        )?);
        c.evidence["native_filesystem"] = json!(native_fs);
        for actor in ["a", "b"] {
            c.command(
                vec![
                    path(&binary),
                    "keygen".into(),
                    path(output.join(format!("{actor}-identity"))),
                ],
                &format!("keygen-{actor}"),
                None,
                60.,
            )?;
        }
        c.command(
            vec![
                path(&binary),
                "init".into(),
                path(output.join("a.db")),
                "a".into(),
                COLLECTION.into(),
            ],
            "init-a",
            None,
            60.,
        )?;
        let registry = toolchain::cargo_home(&run.root, &c.env)?.join("registry");
        let container = format!("weekend-study-{}", nonce()?);
        let volume = format!("{container}-data");
        let first = TcpListener::bind("127.0.0.1:0")?;
        let second = TcpListener::bind("127.0.0.1:0")?;
        let (mac_port, linux_port) = (first.local_addr()?.port(), second.local_addr()?.port());
        drop((first, second));
        let topology = Topology {
            container: &container,
            volume: &volume,
            image: &image,
            source: &source,
            registry: &registry,
            binary: &binary,
            mac_port,
            linux_port,
        };
        c.evidence["docker"] =
            json!({"image":image,"container":container,"volume":volume,"cleanup_verified":false});
        c.save()?;
        let qualification = (|| -> Result<()> {
            c.command(
                cmd(["docker", "volume", "create", &volume]),
                "create-volume",
                None,
                30.,
            )?;
            topology.start_container(&mut c, "start-linux")?;
            c.command(
                cmd([
                    "docker",
                    "exec",
                    "--workdir",
                    "/src",
                    "-e",
                    "CARGO_INCREMENTAL=0",
                    "-e",
                    &format!("WEEKEND_DB_SOURCE_ID={revision}"),
                    &container,
                    "cargo",
                    "build",
                    "--offline",
                    "--locked",
                    "--release",
                    "-p",
                    "study-peer",
                    "--target-dir",
                    "/data/build",
                ]),
                "build-linux",
                None,
                180.,
            )?;
            for (local, remote, label) in [
                ("b-identity", "b-identity", "copy-linux-key"),
                ("a-identity/cert.der", "a-cert.der", "copy-mac-cert"),
            ] {
                c.command(
                    vec![
                        "docker".into(),
                        "cp".into(),
                        path(output.join(local)),
                        format!("{container}:/data/{remote}").into(),
                    ],
                    label,
                    None,
                    60.,
                )?;
            }
            c.command(
                cmd([
                    "docker",
                    "exec",
                    &container,
                    "/data/build/release/study-peer",
                    "init",
                    "/data/b.db",
                    "b",
                    COLLECTION,
                ]),
                "init-b",
                None,
                60.,
            )?;
            let linux_hash = || -> Vec<OsString> {
                cmd([
                    "docker",
                    "exec",
                    &container,
                    "sha256sum",
                    "/data/build/release/study-peer",
                ])
            };
            let hash = c.command(linux_hash(), "linux-binary", None, 30.)?;
            let digest = hash
                .split_ascii_whitespace()
                .next()
                .context("missing Linux study executable hash")?;
            ensure!(
                digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid Linux study executable hash"
            );
            c.evidence["linux_binary_sha256"] = json!(digest);
            c.evidence["linux_platform"] = json!(c.command(
                cmd(["docker", "exec", &container, "uname", "-a"]),
                "linux-platform",
                None,
                30.
            )?);
            c.evidence["linux_rustc"] = json!(c.command(
                cmd(["docker", "exec", &container, "rustc", "-Vv"]),
                "linux-rustc",
                None,
                30.
            )?);
            let fs = c.command(
                cmd([
                    "docker",
                    "exec",
                    &container,
                    "findmnt",
                    "-T",
                    "/data",
                    "-n",
                    "-o",
                    "FSTYPE,SOURCE,TARGET",
                ]),
                "linux-filesystem",
                None,
                30.,
            )?;
            ensure!(
                fs.split_ascii_whitespace().next() == Some("ext4"),
                "Linux study data must use ext4"
            );
            c.evidence["linux_filesystem"] = json!(fs);
            topology.workload(&mut c, &options)?;
            c.command(
                vec![
                    "docker".into(),
                    "cp".into(),
                    format!("{container}:/data/b.db").into(),
                    path(output.join("b.db")),
                ],
                "copy-linux-database",
                None,
                60.,
            )?;
            ensure!(
                c.command(linux_hash(), "verify-linux-binary", None, 30.)? == hash,
                "Linux study executable changed"
            );
            c.evidence["final_database_sha256"] =
                json!({"a":sha(&output.join("a.db"))?,"b":sha(&output.join("b.db"))?});
            Ok(())
        })();
        let cleanup = docker_retire(
            &mut c,
            &container,
            &volume,
            [
                cmd(["docker", "rm", "-f", &container]),
                cmd(["docker", "volume", "rm", &volume]),
            ],
        );
        c.evidence["docker"]["cleanup_verified"] = json!(cleanup.is_ok());
        cleanup?;
        qualification?;
        ensure!(
            inventory(&source, &[])? == source_hashes && sha(&binary)? == local_hash,
            "frozen study source or executable changed"
        );
        c.evidence["artifacts"] = json!(inventory(
            &output,
            &["source", "native-build", "evidence.json"]
        )?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::os::unix::fs::symlink;

    #[test]
    fn exact_covering_replies_are_required_across_stable_revisions() {
        let mut gate = Covering::default();
        let expected = (3, 201, 11, vec![2, 3]);
        let mut states = [
            State::parse("state 4 3 [201] [11] [2, 3]").unwrap(),
            State::parse("state 6 3 [201] [11] [2, 3]").unwrap(),
        ];
        for count in 1..=20 {
            assert!(!gate.observe(&states, [(count, Some(0)); 2], &expected));
        }
        assert!(!gate.observe(&states, [(21, Some(3)); 2], &expected));
        assert!(!gate.observe(&states, [(22, Some(3)); 2], &expected));
        states[1].revision += 1;
        assert!(!gate.observe(&states, [(23, Some(3)); 2], &expected));
        assert!(!gate.observe(&states, [(24, Some(3)); 2], &expected));
        assert!(gate.observe(&states, [(25, Some(3)); 2], &expected));
        assert!(!gate.observe(&states, [(26, None); 2], &expected));
    }

    #[test]
    fn admission_rejects_existing_outputs_and_invalid_round_bounds() {
        let scratch = Scratch::new(&std::env::temp_dir(), "study-soak").unwrap();
        let root = scratch.path();
        let file = root.join("file");
        let dir = root.join("directory");
        let link = root.join("dangling");
        fs::write(&file, b"preserve").unwrap();
        fs::create_dir(&dir).unwrap();
        symlink(root.join("missing"), &link).unwrap();
        for (output, rounds, restart_every) in [
            (file.clone(), 30, 5),
            (dir, 30, 5),
            (link, 30, 5),
            (root.join("new"), 0, 1),
            (root.join("new"), 2, 3),
        ] {
            let options = Options {
                output,
                source: "HEAD".into(),
                image: crate::toolchain::RUST_BOOKWORM_IMAGE.into(),
                rounds,
                restart_every,
            };
            assert!(admit(root, &options).is_err());
        }
        assert_eq!(fs::read(file).unwrap(), b"preserve");
        assert!(!root.join("missing").exists());
        assert!(!root.join("new").exists());
    }
}
