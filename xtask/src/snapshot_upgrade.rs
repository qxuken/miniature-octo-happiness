//! Mixed-version authenticated snapshot CLI qualification.
#[cfg(unix)]
use crate::assurance::Campaign;
#[cfg(windows)]
use crate::windows_compatibility::Campaign;
use crate::{
    snapshot_proof::{
        Receipt, description_identity, finish_with_key_cleanup, receipt, selected_slot,
    },
    source_archive,
    support::{Run, cargo_build_env, cmd, path, qualification_inventory as inventory, sha, text},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    env::consts::EXE_SUFFIX,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

const STREAM: &str = "11111111111111111111111111111111";
const FIXTURE: &str = "tools/snapshot-cli-upgrade-fixture.rs";
const HISTORICAL: &str = "315068fc38b99d8ec616fdb5a8ee81107b471534";
#[cfg(unix)]
#[path = "snapshot_upgrade_linux.rs"]
mod linux;

const CURRENT: &str = "365daf6c8a6b7c85fb049fb5a0370b401dde2424";

fn check_poll(output: &str, local: u64, ack: &Receipt) -> Result<()> {
    let expected = format!("local verified_revision={local}");
    ensure!(
        output.lines().filter(|line| *line == expected).count() == 1
            && receipt(output, STREAM)? == *ack,
        "local observation or durable receipt mismatch"
    );
    Ok(())
}

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = HISTORICAL)]
    historical: String,
    #[arg(long, default_value = CURRENT)]
    current: String,
    #[arg(long)]
    output: PathBuf,
    /// Run the follower on ext4 in this already-local Docker image (macOS client).
    #[arg(long)]
    docker_image: Option<String>,
}

struct Endpoint {
    cli: PathBuf,
    fixture: PathBuf,
    source: PathBuf,
    inventory: BTreeMap<String, String>,
    cli_hash: String,
    fixture_hash: String,
}
impl Endpoint {
    fn unchanged(&self) -> Result<bool> {
        Ok(inventory(&self.source, &[])? == self.inventory
            && sha(&self.cli)? == self.cli_hash
            && sha(&self.fixture)? == self.fixture_hash)
    }
}

struct FixtureLink {
    libraries: BTreeMap<String, PathBuf>,
    search: BTreeSet<PathBuf>,
}

impl FixtureLink {
    fn from_build(messages: &str, required: &[&str]) -> Result<Self> {
        let mut libraries = BTreeMap::new();
        let mut search = BTreeSet::new();
        for line in messages.lines() {
            let artifact: serde_json::Value = serde_json::from_str(line)?;
            if artifact["reason"] != "compiler-artifact" {
                continue;
            }
            let name = text(&artifact["target"]["name"])?;
            for filename in artifact["filenames"]
                .as_array()
                .context("Cargo artifact filenames")?
            {
                let file = Path::new(text(filename)?);
                search.insert(file.parent().context("Cargo artifact parent")?.to_owned());
                if required.contains(&name) && file.extension() == Some("rlib".as_ref()) {
                    let prior = libraries.insert(name.to_owned(), file.to_owned());
                    ensure!(
                        prior.as_deref().is_none_or(|prior| prior == file),
                        "ambiguous Cargo artifact for {name}"
                    );
                }
            }
        }
        for name in required {
            ensure!(
                libraries.contains_key(*name),
                "missing Cargo rlib for {name}"
            );
        }
        Ok(Self { libraries, search })
    }

    fn flags(&self) -> Vec<OsString> {
        let mut flags = Vec::new();
        for (name, file) in &self.libraries {
            let mut external = OsString::from(format!("{name}="));
            external.push(file);
            flags.extend(["--extern".into(), external]);
        }
        for directory in &self.search {
            let mut dependency = OsString::from("dependency=");
            dependency.push(directory);
            flags.extend(["-L".into(), dependency]);
        }
        flags
    }
}

fn endpoint(c: &mut Campaign<'_>, name: &str, reference: &str, fixture: &Path) -> Result<Endpoint> {
    let output = c.output.clone();
    let (commit, source, archive) =
        source_archive::revision(&output, reference, Some(name), |args, label, timeout| {
            c.command(args, label, None, timeout)
        })?;
    let sources = inventory(&source, &[])?;
    c.evidence["sources"][name] = json!({"commit":commit,"archive_sha256":archive,"files":sources});
    let lock = source.join("Cargo.lock");
    let lock_object = source_archive::lock_object(&commit, &lock, name, |args, label, timeout| {
        c.command(args, label, None, timeout)
    })?;
    let lock_hash = sha(&lock)?;
    fs::copy(&lock, output.join(format!("{name}-Cargo.lock")))?;
    let manifest = source.join("Cargo.toml");
    let original: toml::Value = toml::from_str(&fs::read_to_string(manifest)?)?;
    ensure!(
        original["package"]["publish"].as_bool() == Some(false),
        "engine manifest must be private"
    );
    let target = c.output.join("build").join(name);
    c.env
        .insert("WEEKEND_DB_SOURCE_ID".into(), commit.clone().into());
    let mut build = cmd([
        "cargo",
        "build",
        "--release",
        "--offline",
        "--locked",
        "--message-format=json",
        "-p",
        "weekend-replicate",
        "--target-dir",
    ]);
    build.push(path(&target));
    let messages = c.command_at(build, &format!("{name}-build"), None, 240., &source)?;
    let cli = output.join(format!("{name}-weekend-replicate{EXE_SUFFIX}"));
    fs::copy(
        target
            .join("release")
            .join(format!("weekend-replicate{EXE_SUFFIX}")),
        &cli,
    )?;
    #[cfg(windows)]
    let required = ["weekend_db", "rcgen"];
    #[cfg(unix)]
    let required = ["weekend_db"];
    let link = FixtureLink::from_build(&messages, &required)?;
    let rlib = &link.libraries["weekend_db"];
    let linked_sha256 = sha(rlib)?;
    #[cfg(windows)]
    let credential_hash = sha(&link.libraries["rcgen"])?;
    let fixture_bin = output.join(format!("{name}-fixture{EXE_SUFFIX}"));
    let mut fixture_build = vec![
        "rustc".into(),
        "--edition=2024".into(),
        "-O".into(),
        path(fixture),
    ];
    fixture_build.extend(link.flags());
    fixture_build.extend(["-o".into(), path(&fixture_bin)]);
    c.command(fixture_build, &format!("{name}-fixture-build"), None, 90.)?;
    ensure!(
        sha(rlib)? == linked_sha256,
        "linked core rlib changed during fixture build"
    );
    let identity = c.command(
        vec![path(&fixture_bin), "identity".into()],
        &format!("{name}-identity"),
        None,
        45.,
    )?;
    ensure!(identity == commit, "linked runtime identity mismatch");
    let cli_hash = sha(&cli)?;
    let fixture_hash = sha(&fixture_bin)?;
    c.evidence["binaries"][name] = json!({"cli":cli_hash,"fixture":fixture_hash,
          "runtime_source_id":identity,"rlib":linked_sha256,"lock_sha256":lock_hash,"lock_object":lock_object});
    #[cfg(windows)]
    {
        ensure!(
            sha(&link.libraries["rcgen"])? == credential_hash,
            "fixture credential library changed"
        );
        c.evidence["binaries"][name]["credential_rlib"] = json!(credential_hash);
    }
    ensure!(
        inventory(&source, &[])? == sources,
        "prepared source changed during build"
    );
    c.save()?;
    Ok(Endpoint {
        cli,
        fixture: fixture_bin,
        source,
        inventory: sources,
        cli_hash,
        fixture_hash,
    })
}

struct Scenario<'a> {
    endpoints: [&'a Endpoint; 2],
    source: PathBuf,
    follower: PathBuf,
    identity: String,
    client: PathBuf,
    server: PathBuf,
    wrong: PathBuf,
    #[cfg(unix)]
    linux: Option<&'a linux::Follower>,
}
impl Scenario<'_> {
    #[cfg(windows)]
    fn prepare_credentials(&self, c: &mut Campaign<'_>) -> Result<()> {
        let directories = [
            ("client", self.client.as_path()),
            ("server", self.server.as_path()),
            ("wrong", self.wrong.as_path()),
        ];
        for (_, directory) in directories {
            fs::create_dir(directory)?;
        }
        c.restrict_directories(&directories)?;
        for (name, dir) in directories {
            self.client_call(
                c,
                vec![path(&self.endpoints[0].fixture), "keygen".into(), path(dir)],
                &format!("{name}-keygen"),
            )?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn prepare_credentials(&self, c: &mut Campaign<'_>) -> Result<()> {
        for (name, dir) in [
            ("client", &self.client),
            ("server", &self.server),
            ("wrong", &self.wrong),
        ] {
            self.client_call(
                c,
                vec![path(&self.endpoints[0].cli), "keygen".into(), path(dir)],
                &format!("{name}-keygen"),
            )?;
        }
        Ok(())
    }

    fn client_call(
        &self,
        c: &mut Campaign<'_>,
        args: Vec<OsString>,
        label: &str,
    ) -> Result<String> {
        c.command(args, label, None, 45.)
    }

    fn serve<T>(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        label: &str,
        call: impl FnOnce(&mut Campaign<'_>, SocketAddr) -> Result<T>,
    ) -> Result<T> {
        #[cfg(unix)]
        if let Some(linux) = self.linux {
            return linux.serve(c, endpoint, label, &self.identity, call);
        }
        c.loopback(
            vec![
                path(&self.endpoints[endpoint].cli),
                "serve".into(),
                path(&self.follower),
                STREAM.into(),
                self.identity.clone().into(),
                "127.0.0.1:0".into(),
                path(&self.server),
                path(self.client.join("cert.der")),
                "--max-exchanges".into(),
                "20".into(),
            ],
            label,
            (Duration::from_secs(10), 200.),
            false,
            call,
        )
    }

    fn poll(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        address: SocketAddr,
        local: u64,
        ack: &Receipt,
        label: &str,
    ) -> Result<()> {
        let out = self.client_call(
            c,
            vec![
                path(&self.endpoints[endpoint].cli),
                "poll".into(),
                path(&self.source),
                STREAM.into(),
                address.to_string().into(),
                path(&self.client),
                path(self.server.join("cert.der")),
            ],
            label,
        )?;
        check_poll(&out, local, ack)
    }

    fn ship(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        address: SocketAddr,
        revision: u64,
        wrong_pin: bool,
    ) -> Result<Option<(Receipt, PathBuf)>> {
        self.ship_export(c, endpoint, address, revision, wrong_pin, false)
    }

    fn ship_export(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        address: SocketAddr,
        revision: u64,
        wrong_pin: bool,
        repeat: bool,
    ) -> Result<Option<(Receipt, PathBuf)>> {
        let outfile = if repeat {
            format!("repeat-export-{revision}.wdb")
        } else if wrong_pin {
            "unconfirmed-export".into()
        } else {
            format!("export-{revision}.wdb")
        };
        let export = self.source.parent().unwrap().join(outfile);
        let credentials = if wrong_pin { &self.wrong } else { &self.server };
        let out = c.command(
            vec![
                path(&self.endpoints[endpoint].cli),
                "ship".into(),
                path(&self.source),
                STREAM.into(),
                path(&export),
                address.to_string().into(),
                path(&self.client),
                path(credentials.join("cert.der")),
                "--attempts".into(),
                "1".into(),
            ],
            &format!(
                "ship-{revision}-{}",
                if repeat {
                    "repeat"
                } else if wrong_pin {
                    "wrong-pin"
                } else {
                    "trusted"
                }
            ),
            wrong_pin.then_some("transfer not confirmed"),
            45.,
        )?;
        ensure!(
            out.lines()
                .any(|line| line.starts_with(&format!("local captured_revision={revision} "))),
            "local capture missing"
        );
        if wrong_pin {
            ensure!(
                !out.lines()
                    .any(|line| line.starts_with("follower durable_revision=")),
                "failed TLS shipment emitted durable receipt"
            );
            return Ok(None);
        }
        let ack = receipt(&out, STREAM)?;
        ensure!(
            ack.revision == revision && ack.digest == sha(&export)?,
            "export and durable receipt mismatch"
        );
        Ok(Some((ack, export)))
    }

    fn offline(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        receipt_expected: &Receipt,
        export: &Path,
        label: &str,
    ) -> Result<()> {
        #[cfg(unix)]
        if let Some(linux) = self.linux {
            linux.offline(c, endpoint, &self.identity, receipt_expected, export, label)?;
            linux.copy_follower(c, &self.follower, label)?;
        }
        let status = self.client_call(
            c,
            vec![
                path(&self.endpoints[endpoint].cli),
                "status".into(),
                path(&self.follower),
                STREAM.into(),
                self.identity.clone().into(),
            ],
            &format!("{label}-status"),
        )?;
        ensure!(
            receipt(&status, STREAM)? == *receipt_expected,
            "reopened durable receipt mismatch"
        );
        let file = self
            .follower
            .join(format!("replica-{}.wdb", selected_slot(&status)?));
        ensure!(
            fs::read(&file)? == fs::read(export)?,
            "reopened follower image differs from export"
        );
        let before = sha(&file)?;
        for (index, build) in self.endpoints.iter().enumerate() {
            self.client_call(
                c,
                vec![
                    path(&build.fixture),
                    "check".into(),
                    path(&file),
                    receipt_expected.revision.to_string().into(),
                ],
                &format!("{label}-oracle-{index}"),
            )?;
        }
        ensure!(sha(&file)? == before, "oracle changed selected image");
        Ok(())
    }

    fn execute(&self, c: &mut Campaign<'_>) -> Result<()> {
        let [old, new] = self.endpoints;
        #[cfg(unix)]
        if let Some(linux) = self.linux {
            linux.initialize(c, &self.identity)?;
        }
        #[cfg(unix)]
        let local_follower = self.linux.is_none();
        #[cfg(windows)]
        let local_follower = true;
        if local_follower {
            self.client_call(
                c,
                vec![
                    path(&old.cli),
                    "follower-init".into(),
                    path(&self.follower),
                    STREAM.into(),
                    self.identity.clone().into(),
                ],
                "follower-init",
            )?;
        }
        self.prepare_credentials(c)?;
        #[cfg(unix)]
        if let Some(linux) = self.linux {
            linux.credentials(c, &self.server, &self.client)?;
        }
        let (first, export3) = self.serve(c, 0, "old-server", |c, address| {
            let result = self
                .ship(c, 0, address, 3, false)?
                .context("revision-3 receipt")?;
            self.poll(c, 0, address, 3, &result.0, "poll-3")?;
            Ok(result)
        })?;
        self.offline(c, 1, &first, &export3, "new-reopens-3")?;
        self.client_call(
            c,
            vec![
                path(&new.fixture),
                "advance".into(),
                path(&self.source),
                "4".into(),
            ],
            "advance-4",
        )?;
        let before_failed = inventory(&self.follower, &[])?;
        self.serve(c, 0, "old-server-new-client", |c, address| {
            self.poll(c, 1, address, 4, &first, "poll-3-while-local-4")?;
            ensure!(
                self.ship(c, 1, address, 4, true)?.is_none(),
                "wrong pin yielded acknowledgment"
            );
            self.poll(c, 1, address, 4, &first, "poll-after-wrong-pin")
        })?;
        self.offline(c, 0, &first, &export3, "old-reopens-3")?;
        ensure!(
            inventory(&self.follower, &[])? == before_failed,
            "wrong pin mutated follower files"
        );
        c.evidence["wrong_pin"] = json!({"local_capture":4,"follower_durable_before_and_after":3,"follower_files_unchanged":true});
        let (second, export4) = self.serve(c, 0, "retry-new-to-old", |c, address| {
            self.ship(c, 1, address, 4, false)?
                .context("revision-4 receipt")
        })?;
        self.offline(c, 1, &second, &export4, "new-reopens-4")?;
        self.client_call(
            c,
            vec![
                path(&old.fixture),
                "advance".into(),
                path(&self.source),
                "5".into(),
            ],
            "advance-5",
        )?;
        let (third, export5) = self.serve(c, 1, "new-server-old-client", |c, address| {
            self.poll(c, 0, address, 5, &second, "poll-4-while-local-5")?;
            self.ship(c, 0, address, 5, false)?
                .context("revision-5 receipt")
        })?;
        self.offline(c, 0, &third, &export5, "old-reopens-5")?;
        let before_poll = inventory(&self.follower, &[])?;
        self.serve(c, 1, "both-new", |c, address| {
            self.poll(c, 1, address, 5, &third, "poll-5-first")?;
            self.poll(c, 1, address, 5, &third, "poll-5-repeat")?;
            #[cfg(unix)]
            if self.linux.is_some() {
                let (repeated, repeated_export) = self
                    .ship_export(c, 1, address, 5, false, true)?
                    .context("repeated revision-5 receipt")?;
                ensure!(
                    repeated == third && fs::read(repeated_export)? == fs::read(&export5)?,
                    "repeat shipment changed revision or snapshot"
                );
                c.evidence["repeat_ship"] = json!({"revision":5,"same_receipt_and_export":true});
            }
            Ok(())
        })?;
        self.offline(c, 1, &third, &export5, "new-reopens-5")?;
        ensure!(
            inventory(&self.follower, &[])? == before_poll,
            "repeat poll changed follower files"
        );
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            self.client_call(
                c,
                vec![
                    path(&endpoint.fixture),
                    "check".into(),
                    path(&self.source),
                    "5".into(),
                ],
                &format!("final-primary-{index}"),
            )?;
        }
        c.evidence["receipts"] = json!([first, second, third]);
        c.evidence["repeat_poll_files_unchanged"] = json!(true);
        c.evidence["retained_revision"] = json!(2);
        Ok(())
    }
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    ensure!(
        opts.docker_image.is_none() || cfg!(target_os = "macos"),
        "Docker follower requires a native macOS client"
    );
    let mut c = Campaign::new(
        run,
        &opts.output,
        "mixed-version authenticated snapshot CLI qualification",
    )?;
    #[cfg(unix)]
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    let work = c.output.join("work");
    let client = work.join("client");
    let server = work.join("server");
    let wrong = work.join("wrong");
    c.env = cargo_build_env(&c.env);
    #[cfg(windows)]
    c.env
        .retain(|name, _| !name.eq_ignore_ascii_case("WEEKEND_DB_SOURCE_ID"));
    c.save()?;
    let result = (|| -> Result<()> {
        let fixture = run.root.join(FIXTURE);
        let hash = sha(&fixture)?;
        c.evidence["fixture_sha256"] = json!(hash);
        c.evidence["compiler"] = json!(c.command(cmd(["rustc", "-Vv"]), "compiler", None, 45.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 45.)?);
        let old = endpoint(&mut c, "historical", &opts.historical, &fixture)?;
        let new = endpoint(&mut c, "current", &opts.current, &fixture)?;
        fs::create_dir(&work)?;
        let source = work.join("primary.wdb");
        let follower = work.join("follower");
        fs::create_dir(&follower)?;
        c.command(
            vec![path(&old.fixture), "create".into(), path(&source)],
            "create-primary",
            None,
            45.,
        )?;
        let description = c.command(
            vec![
                path(&old.cli),
                "describe".into(),
                path(&source),
                STREAM.into(),
            ],
            "describe-primary",
            None,
            45.,
        )?;
        #[cfg(unix)]
        let linux = opts
            .docker_image
            .as_ref()
            .map(|image| linux::Follower::new(image))
            .transpose()?;
        #[cfg(unix)]
        if let Some(linux) = &linux {
            // Cleanup remains mandatory if any preparation or scenario step fails.
            let qualification = (|| -> Result<()> {
                linux.prepare(&mut c, [&old, &new], &fixture)?;
                let scenario = Scenario {
                    endpoints: [&old, &new],
                    source: source.clone(),
                    follower: follower.clone(),
                    identity: description_identity(&description)?,
                    client: client.clone(),
                    server: server.clone(),
                    wrong: wrong.clone(),
                    linux: Some(linux),
                };
                scenario.execute(&mut c)?;
                linux.verify(&mut c, [&old, &new])
            })();
            let cleanup = linux.cleanup(&mut c);
            match (qualification, cleanup) {
                (Ok(()), Ok(())) => (),
                (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
                (Err(work), Err(cleanup)) => anyhow::bail!("{work:#}; {cleanup:#}"),
            }
        } else {
            Scenario {
                endpoints: [&old, &new],
                source: source.clone(),
                follower: follower.clone(),
                identity: description_identity(&description)?,
                client: client.clone(),
                server: server.clone(),
                wrong: wrong.clone(),
                linux: None,
            }
            .execute(&mut c)?;
        }
        #[cfg(windows)]
        let scenario = Scenario {
            endpoints: [&old, &new],
            source,
            follower,
            identity: description_identity(&description)?,
            client: client.clone(),
            server: server.clone(),
            wrong: wrong.clone(),
        };
        #[cfg(windows)]
        scenario.execute(&mut c)?;
        ensure!(
            old.unchanged()? && new.unchanged()? && sha(&fixture)? == hash,
            "frozen source, fixture or executable changed"
        );
        Ok(())
    })();
    let result = finish_with_key_cleanup(
        &mut c.evidence,
        &c.output,
        result,
        &[&client, &server, &wrong],
    );
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::{delivery, support::Scratch};

    #[test]
    fn fixture_link_uses_reported_libraries_across_cargo_layouts() {
        let root = std::env::temp_dir();
        let artifact = |name: &str, file: &Path| json!({"reason":"compiler-artifact", "target":{"name":name}, "filenames":[file]});
        let legacy = root.join("release").join("deps");
        let modern = root.join("release").join("build");
        let engine = modern.join("weekend-db").join("hash").join("out");
        let credential = modern.join("rcgen").join("hash").join("out");
        let link = |engine: &Path, credential: &Path| {
            let library = engine.join("libweekend_db.rlib");
            let credentials = credential.join("librcgen-hash.rlib");
            let messages = format!(
                "{}\n{}\n{}",
                artifact("weekend_db", &library),
                artifact("rcgen", &credentials),
                json!({"reason":"build-finished", "success":true}),
            );
            let link = FixtureLink::from_build(&messages, &["weekend_db", "rcgen"]).unwrap();
            assert_eq!(link.libraries["weekend_db"], library);
            assert_eq!(link.libraries["rcgen"], credentials);
            assert_eq!(
                link.search,
                BTreeSet::from([engine.into(), credential.into()])
            );
            assert!(FixtureLink::from_build(&messages, &["other"]).is_err());
            assert!(
                FixtureLink::from_build(
                    &format!(
                        "{messages}\n{}",
                        artifact("weekend_db", &root.join("other.rlib"))
                    ),
                    &["weekend_db", "rcgen"],
                )
                .is_err()
            );
        };
        link(&legacy, &legacy);
        link(&engine, &credential);
    }

    #[test]
    fn poll_accepts_historical_local_line_alongside_older_durable_receipt() {
        let digest = "ab".repeat(32);
        let ack = Receipt {
            revision: 3,
            stream: STREAM.to_owned(),
            digest: digest.clone(),
        };
        let line = format!(
            "local verified_revision=4\nfollower durable_revision=3 stream={STREAM} digest={digest}\n"
        );
        check_poll(&line, 4, &ack).unwrap();
        assert!(check_poll(&line, 3, &ack).is_err());
        assert!(check_poll("local verified_revision=4\n", 4, &ack).is_err());
    }

    #[test]
    fn environment_preserves_selected_toolchain_and_removes_overrides() {
        let original = BTreeMap::from([
            ("RUSTUP_TOOLCHAIN".into(), "1.99.1".into()),
            ("RUSTFLAGS".into(), "bad".into()),
            ("CARGO_BUILD_RUSTFLAGS".into(), "bad".into()),
        ]);
        let clean = cargo_build_env(&original);
        assert_eq!(
            clean,
            BTreeMap::from([
                ("RUSTUP_TOOLCHAIN".into(), "1.99.1".into()),
                ("CARGO_INCREMENTAL".into(), "0".into()),
            ])
        );
    }

    #[test]
    #[cfg(unix)]
    fn failed_setup_scrubs_private_keys_and_records_failure() {
        let tmp = Scratch::new(&std::env::temp_dir(), "snapshot-upgrade").unwrap();
        let run = Run::new(tmp.path().to_owned());
        let output = tmp.path().join("negative");
        let mut c = Campaign::detached(&run, &output, "snapshot upgrade control").unwrap();
        c.evidence["verified"] = json!(false);
        let private = output.join("work/client");
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("key.der"), b"private").unwrap();
        let result = Err(anyhow::anyhow!("fixture setup failed"));
        let result = finish_with_key_cleanup(&mut c.evidence, &c.output, result, &[&private]);
        assert!(c.finish(result).is_err());
        let proof = delivery::read_json(&output.join("evidence.json")).unwrap();
        assert_eq!(proof["verified"], false);
        assert_eq!(proof["error"], "fixture setup failed");
        assert_eq!(proof["private_credentials_retained"], false);
        assert!(!private.join("key.der").exists());
    }
}
