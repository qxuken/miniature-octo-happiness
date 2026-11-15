//! Mixed-version authenticated snapshot CLI qualification.
use crate::{
    artifact_trace::inventory,
    assurance::{self, Campaign},
    follower_backup::{self, Receipt, description_identity, receipt, selected_slot},
    serialization_scale,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

const STREAM: &str = "11111111111111111111111111111111";
const FIXTURE: &str = "tools/snapshot-cli-upgrade-fixture.rs";
const HISTORICAL: &str = "315068fc38b99d8ec616fdb5a8ee81107b471534";
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

fn single_rlib(folder: &Path) -> Result<PathBuf> {
    let mut matches = Vec::new();
    for item in fs::read_dir(folder)? {
        let item = item?;
        let name = item.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("libweekend_db") && name.ends_with(".rlib"))
        {
            ensure!(item.file_type()?.is_file(), "nonregular linked core rlib");
            matches.push(item.path());
        }
    }
    ensure!(
        matches.len() == 1,
        "one exact core rlib required, found {}",
        matches.len()
    );
    Ok(matches.pop().unwrap())
}

fn endpoint(c: &mut Campaign<'_>, name: &str, reference: &str, fixture: &Path) -> Result<Endpoint> {
    let (commit, source, archive) = assurance::archive_revision(c, reference, Some(name))?;
    let sources = inventory(&source, &[])?;
    c.evidence["sources"][name] = json!({"commit":commit,"archive_sha256":archive,"files":sources});
    let lock = source.join("Cargo.lock");
    let lock_hash = sha(&lock)?;
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
        "-p",
        "weekend-replicate",
        "--target-dir",
    ]);
    build.push(path(&target));
    c.command_at(build, &format!("{name}-build"), None, 240., &source)?;
    let cli = target.join("release/weekend-replicate");
    let deps = target.join("release/deps");
    let rlib = single_rlib(&deps)?;
    let linked_sha256 = sha(&rlib)?;
    let fixture_bin = c.output.join(format!("{name}-fixture"));
    let mut external = OsString::from("weekend_db=");
    external.push(&rlib);
    let mut dependencies = OsString::from("dependency=");
    dependencies.push(&deps);
    c.command(
        vec![
            "rustc".into(),
            "--edition=2024".into(),
            "-O".into(),
            path(fixture),
            "--extern".into(),
            external,
            "-L".into(),
            dependencies,
            "-o".into(),
            path(&fixture_bin),
        ],
        &format!("{name}-fixture-build"),
        None,
        90.,
    )?;
    ensure!(
        sha(&rlib)? == linked_sha256,
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
          "runtime_source_id":identity,"rlib":linked_sha256,"lock_sha256":lock_hash});
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
}
impl Scenario<'_> {
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
        let outfile = if wrong_pin {
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
                if wrong_pin { "wrong-pin" } else { "trusted" }
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
        for (name, dir) in [
            ("client", &self.client),
            ("server", &self.server),
            ("wrong", &self.wrong),
        ] {
            self.client_call(
                c,
                vec![path(&old.cli), "keygen".into(), path(dir)],
                &format!("{name}-keygen"),
            )?;
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
            self.poll(c, 1, address, 5, &third, "poll-5-repeat")
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
    let mut c = Campaign::new(
        run,
        &opts.output,
        "mixed-version authenticated snapshot CLI qualification",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    let work = c.output.join("work");
    let client = work.join("client");
    let server = work.join("server");
    let wrong = work.join("wrong");
    c.env = serialization_scale::clean_env(&c.env);
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
        let scenario = Scenario {
            endpoints: [&old, &new],
            source,
            follower,
            identity: description_identity(&description)?,
            client: client.clone(),
            server: server.clone(),
            wrong: wrong.clone(),
        };
        scenario.execute(&mut c)?;
        ensure!(
            old.unchanged()? && new.unchanged()? && sha(&fixture)? == hash,
            "frozen source, fixture or executable changed"
        );
        Ok(())
    })();
    follower_backup::finish_with_key_cleanup(c, result, &[&client, &server, &wrong])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{delivery, support::Scratch};

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
        let clean = serialization_scale::clean_env(&original);
        assert_eq!(
            clean,
            BTreeMap::from([
                ("RUSTUP_TOOLCHAIN".into(), "1.99.1".into()),
                ("CARGO_INCREMENTAL".into(), "0".into()),
            ])
        );
    }

    #[test]
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
        assert!(follower_backup::finish_with_key_cleanup(c, result, &[&private]).is_err());
        let proof = delivery::read_json(&output.join("evidence.json")).unwrap();
        assert_eq!(proof["verified"], false);
        assert_eq!(proof["error"], "fixture setup failed");
        assert_eq!(proof["private_credentials_retained"], false);
        assert!(!private.join("key.der").exists());
    }
}
