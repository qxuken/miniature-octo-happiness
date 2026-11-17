//! Reproduce the finite authenticated month against one archived engine source.
use crate::{
    artifact_trace::{docker_retire, nonce},
    assurance::{self, Campaign},
    snapshot_proof::{finish_with_key_cleanup, hex, receipt, selected_slot},
    support::{Run, cargo_build_env, cmd, path, qualification_inventory as inventory, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

const STREAM: &str = "11111111111111111111111111111111";
const PORT: u16 = 43183;
const HEADER: &str = "day,local_revision,captured_revision,ship_revision,poll_revision,gap,image_bytes,commit_us,retain_us,capture_us,heartbeat_us,ship_us,post_upload_us,poll_us,digest";
const CLI: &str = "/data/build/release/weekend-replicate";
const DRIVER: &str = "/data/build/release/examples/live_month";

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "HEAD")]
    source: String,
    /// Already installed immutable Docker image; builds stay offline and locked.
    #[arg(long)]
    image: String,
    #[arg(long)]
    output: PathBuf,
}

fn number(text: &str) -> Result<u64> {
    ensure!(
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()),
        "invalid month number"
    );
    Ok(text.parse()?)
}

// This oracle is deliberately independent of the example's private constants
// and assertions. In particular, local revision must not stand in for a receipt.
fn observations(csv: &str) -> Result<Value> {
    let mut lines = csv.lines();
    ensure!(lines.next() == Some(HEADER), "unexpected month CSV header");
    let mut rows = Vec::new();
    for (offset, line) in lines.enumerate() {
        let fields: Vec<_> = line.split(',').collect();
        ensure!(
            fields.len() == 15 && offset < 31,
            "unexpected month row shape/count"
        );
        let day = offset as u64 + 1;
        let final_export = day == 31;
        let captured = if final_export { 93 } else { day * 3 + 2 };
        let local = if final_export { 93 } else { day * 3 + 3 };
        let lost = !final_export && day.is_multiple_of(7);
        let shipped = if fields[3].is_empty() {
            None
        } else {
            Some(number(fields[3])?)
        };
        ensure!(
            number(fields[0])? == day
                && number(fields[1])? == local
                && number(fields[2])? == captured
                && number(fields[4])? == captured
                && number(fields[5])? == u64::from(!final_export)
                && shipped == if lost { None } else { Some(captured) },
            "month local/capture/receipt schedule mismatch on day {day}"
        );
        let bytes = number(fields[6])?;
        ensure!(
            bytes > 0 && bytes <= 64 * 1024 * 1024 && hex(fields[14], 64),
            "invalid month image length/digest"
        );
        let timings: Vec<_> = fields[7..14]
            .iter()
            .map(|v| number(v))
            .collect::<Result<_>>()?;
        if final_export {
            ensure!(
                timings[0] == 0 && timings[1] == 0 && timings[3] == 0,
                "final shipment unexpectedly wrote the primary"
            );
        }
        rows.push(
            json!({"day":day,"local_revision":local,"captured_revision":captured,
            "ship_revision":shipped,"follower_revision":captured,"lost_reply":lost,
            "image_bytes":bytes,"digest":fields[14],"timings_us":timings}),
        );
    }
    ensure!(rows.len() == 31, "incomplete month CSV");
    let last = rows.last().unwrap();
    Ok(
        json!({"days":30,"installations":31,"polls":32,"authenticated_exchanges":63,
        "lost_reply_days":[7,14,21,28],"retained_views":31,"final_revision":93,
        "final_digest":last["digest"],"final_image_bytes":last["image_bytes"],"rows":rows}),
    )
}

fn exec(container: &str, args: Vec<OsString>) -> Vec<OsString> {
    let mut argv = cmd(["docker", "exec", container]);
    argv.extend(args);
    argv
}

fn copy_out(
    c: &mut Campaign<'_>,
    container: &str,
    remote: &str,
    local: &Path,
    label: &str,
) -> Result<()> {
    c.command(
        vec![
            "docker".into(),
            "cp".into(),
            format!("{container}:{remote}").into(),
            path(local),
        ],
        label,
        None,
        60.,
    )?;
    Ok(())
}

fn linux_digest(c: &mut Campaign<'_>, container: &str, file: &str, label: &str) -> Result<String> {
    let reply = c.command(exec(container, cmd(["sha256sum", file])), label, None, 30.)?;
    let (digest, name) = reply
        .split_once("  ")
        .context("missing Linux hash fields")?;
    ensure!(
        hex(digest, 64) && name == file,
        "unexpected Linux hash output"
    );
    Ok(digest.into())
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "snapshot month requires macOS primary and Docker Linux follower"
    );
    ensure!(
        options
            .image
            .strip_prefix("sha256:")
            .is_some_and(|v| hex(v, 64)),
        "immutable local Docker image required"
    );
    let output = assurance::fresh_output(&run.root, &options.output)?;
    let mut c = Campaign::new(
        run,
        &output,
        "authenticated macOS/APFS to Docker Linux/ext4 snapshot month",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    c.env = cargo_build_env(&c.env);
    let credentials = output.join("client-credentials");
    let result = (|| -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
        let (revision, source) = assurance::archive_source(&mut c, &options.source)?;
        let source_hashes = inventory(&source, &[])?;
        c.evidence["engine_source_sha256"] = json!(source_hashes);
        c.env
            .insert("WEEKEND_DB_SOURCE_ID".into(), revision.clone().into());
        c.evidence["native_rustc"] =
            json!(c.command(cmd(["rustc", "-Vv"]), "native-rustc", None, 30.)?);
        c.evidence["native_filesystem"] = json!(c.command(
            vec!["df".into(), "-P".into(), path(&output)],
            "native-filesystem",
            None,
            30.
        )?);
        #[cfg(target_os = "macos")]
        {
            c.evidence["native_volume"] = crate::macos_volume::host_volume(&output)?;
        }
        let build = output.join("build");
        let mut argv = cmd([
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
            "-p",
            "weekend-replicate",
            "--bin",
            "weekend-replicate",
            "--example",
            "live_month",
            "--target-dir",
        ]);
        argv.push(path(&build));
        c.command_at(argv, "build-native", None, 240., &source)?;
        let cli = build.join("release/weekend-replicate");
        let driver = build.join("release/examples/live_month");
        let cli_hash = sha(&cli)?;
        let driver_hash = sha(&driver)?;
        let native = output.join("native-binaries");
        fs::create_dir(&native)?;
        fs::copy(&cli, native.join("cli"))?;
        fs::copy(&driver, native.join("driver"))?;
        c.evidence["native_binaries"] = json!({"cli":cli_hash,"driver":driver_hash});
        c.command(
            vec![path(&cli), "keygen".into(), path(&credentials)],
            "client-keygen",
            None,
            30.,
        )?;
        let primary = output.join("primary.wdb");
        let identity = c.command(
            vec![path(&driver), "seed".into(), path(&primary)],
            "seed",
            None,
            60.,
        )?;
        ensure!(hex(&identity, 32), "invalid seeded file identity");
        c.evidence["file_identity"] = json!(identity);
        let image = c.command(
            cmd([
                "docker",
                "image",
                "inspect",
                "--format={{.Id}}",
                &options.image,
            ]),
            "docker-image",
            None,
            30.,
        )?;
        ensure!(image == options.image, "Docker image identity changed");
        let reservation = TcpListener::bind("127.0.0.1:0")?;
        let port = reservation.local_addr()?.port();
        let name = format!("weekend-snapshot-month-{}", nonce()?);
        let volume = format!("{name}-data");
        let registry = toolchain::cargo_home(&run.root, &c.env)?.join("registry");
        c.evidence["docker"] = json!({"image":image,"container":name,"volume":volume,
            "address":format!("127.0.0.1:{port}"),"cleanup_verified":false});
        c.save()?;
        drop(reservation);
        let work = (|| -> Result<()> {
            c.command(
                cmd(["docker", "volume", "create", &volume]),
                "linux-volume",
                None,
                30.,
            )?;
            c.command(
                cmd([
                    "docker",
                    "run",
                    "--pull=never",
                    "-d",
                    "--name",
                    &name,
                    "--memory=3g",
                    "--cpus=2",
                    "--pids-limit=256",
                    "--publish",
                    &format!("127.0.0.1:{port}:{PORT}"),
                    "--mount",
                    &format!("type=volume,source={volume},target=/data"),
                    "--mount",
                    &format!("type=bind,source={},target=/src,readonly", source.display()),
                    "--mount",
                    &format!(
                        "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                        registry.display()
                    ),
                    &image,
                    "tail",
                    "-f",
                    "/dev/null",
                ]),
                "linux-start",
                None,
                60.,
            )?;
            c.evidence["linux_platform"] = json!(c.command(
                exec(&name, cmd(["uname", "-a"])),
                "linux-platform",
                None,
                30.
            )?);
            c.evidence["linux_rustc"] =
                json!(c.command(exec(&name, cmd(["rustc", "-Vv"])), "linux-rustc", None, 30.)?);
            let filesystem = c.command(
                exec(
                    &name,
                    cmd(["findmnt", "-T", "/data", "-n", "-o", "FSTYPE,SOURCE,TARGET"]),
                ),
                "linux-filesystem",
                None,
                30.,
            )?;
            ensure!(
                filesystem.split_ascii_whitespace().next() == Some("ext4"),
                "Linux build/runtime files require ext4"
            );
            c.evidence["linux_filesystem"] = json!(filesystem);
            c.command(
                exec(&name, cmd(["mkdir", "-p", "/data/tmp"])),
                "linux-tmp",
                None,
                30.,
            )?;
            c.command(
                cmd([
                    "docker",
                    "exec",
                    "--workdir",
                    "/src",
                    "-e",
                    "TMPDIR=/data/tmp",
                    "-e",
                    "CARGO_INCREMENTAL=0",
                    "-e",
                    &format!("WEEKEND_DB_SOURCE_ID={revision}"),
                    &name,
                    "cargo",
                    "build",
                    "--offline",
                    "--locked",
                    "--release",
                    "-p",
                    "weekend-replicate",
                    "--bin",
                    "weekend-replicate",
                    "--example",
                    "live_month",
                    "--target-dir",
                    "/data/build",
                ]),
                "build-linux",
                None,
                240.,
            )?;
            let binaries = output.join("linux-binaries");
            fs::create_dir(&binaries)?;
            for (label, remote) in [("cli", CLI), ("driver", DRIVER)] {
                let local = binaries.join(label);
                copy_out(
                    &mut c,
                    &name,
                    remote,
                    &local,
                    &format!("copy-linux-{label}"),
                )?;
                let digest = linux_digest(&mut c, &name, remote, &format!("linux-{label}-hash"))?;
                ensure!(sha(&local)? == digest, "Linux retained executable mismatch");
                c.evidence["linux_binaries"][label] = json!(digest);
            }
            c.command(
                exec(&name, cmd([CLI, "keygen", "/data/server-credentials"])),
                "server-keygen",
                None,
                30.,
            )?;
            let pin = output.join("server.der");
            copy_out(
                &mut c,
                &name,
                "/data/server-credentials/cert.der",
                &pin,
                "copy-server-public-cert",
            )?;
            c.command(
                vec![
                    "docker".into(),
                    "cp".into(),
                    path(credentials.join("cert.der")),
                    format!("{name}:/data/client.der").into(),
                ],
                "copy-client-public-cert",
                None,
                30.,
            )?;
            c.command(
                exec(
                    &name,
                    cmd([CLI, "follower-init", "/data/follower", STREAM, &identity]),
                ),
                "follower-init",
                None,
                30.,
            )?;
            let serve = exec(
                &name,
                cmd([
                    CLI,
                    "serve",
                    "/data/follower",
                    STREAM,
                    &identity,
                    &format!("0.0.0.0:{PORT}"),
                    "/data/server-credentials",
                    "/data/client.der",
                    "--max-exchanges",
                    "63",
                ]),
            );
            let expected_ready = format!("ready 0.0.0.0:{PORT}");
            let exports = output.join("exports");
            let csv = c.ready_process(
                serve,
                "linux-follower",
                (Duration::from_secs(30), 240.),
                true,
                |line| {
                    ensure!(
                        line == expected_ready,
                        "unexpected container follower readiness"
                    );
                    Ok(())
                },
                |c, ()| {
                    c.command(
                        vec![
                            path(&driver),
                            "send".into(),
                            path(&primary),
                            path(&exports),
                            format!("127.0.0.1:{port}").into(),
                            path(&credentials),
                            path(&pin),
                        ],
                        "month",
                        None,
                        180.,
                    )
                },
            )?;
            let observed = observations(&csv)?;
            c.evidence["month"] = observed.clone();
            let expected_digest = observed["final_digest"].as_str().unwrap();
            let reopen = c.command(
                exec(
                    &name,
                    cmd([DRIVER, "verify-follower", "/data/follower", &identity]),
                ),
                "reopen-linux",
                None,
                60.,
            )?;
            ensure!(
                reopen == format!("reopened revision=93 retained=31 digest={expected_digest}"),
                "follower reopen disagrees with month"
            );
            let status = c.command(
                exec(
                    &name,
                    cmd([CLI, "status", "/data/follower", STREAM, &identity]),
                ),
                "follower-status",
                None,
                30.,
            )?;
            let ack = receipt(&status, STREAM)?;
            ensure!(
                ack.revision == 93 && ack.digest == expected_digest,
                "reopened durable receipt changed"
            );
            let selected = format!("/data/follower/replica-{}.wdb", selected_slot(&status)?);
            let selected_local = output.join("selected-follower.wdb");
            copy_out(
                &mut c,
                &name,
                &selected,
                &selected_local,
                "copy-selected-image",
            )?;
            let final_export = exports.join("snapshot-31.wdb");
            ensure!(
                fs::read(&selected_local)? == fs::read(&final_export)?
                    && sha(&selected_local)? == expected_digest
                    && fs::metadata(&selected_local)?.len()
                        == observed["final_image_bytes"].as_u64().unwrap(),
                "selected follower differs from exact final export"
            );
            for (label, remote) in [("cli", CLI), ("driver", DRIVER)] {
                ensure!(
                    linux_digest(&mut c, &name, remote, &format!("linux-{label}-after"))?
                        == c.evidence["linux_binaries"][label],
                    "Linux executable changed"
                );
            }
            c.evidence["reopened_receipt"] = json!(ack);
            c.evidence["selected_image_equals_export"] = json!(true);
            Ok(())
        })();
        let cleanup = docker_retire(
            &mut c,
            &name,
            &volume,
            [
                cmd(["docker", "rm", "-f", &name]),
                cmd(["docker", "volume", "rm", &volume]),
            ],
        );
        c.evidence["docker"]["cleanup_verified"] = json!(cleanup.is_ok());
        match (work, cleanup) {
            (Ok(()), Ok(())) => (),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
            (Err(work), Err(cleanup)) => anyhow::bail!("{work:#}; {cleanup:#}"),
        }
        ensure!(
            inventory(&source, &[])? == source_hashes
                && sha(&cli)? == cli_hash
                && sha(&driver)? == driver_hash,
            "archived source or native executables changed"
        );
        Ok(())
    })();
    let result = finish_with_key_cleanup(&mut c.evidence, &output, result, &[&credentials]);
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn csv() -> String {
        let mut lines = vec![HEADER.to_owned()];
        for day in 1..=31_u64 {
            let final_export = day == 31;
            let captured = if final_export { 93 } else { day * 3 + 2 };
            let local = if final_export { 93 } else { day * 3 + 3 };
            let ship = if day <= 30 && day.is_multiple_of(7) {
                String::new()
            } else {
                captured.to_string()
            };
            lines.push(format!(
                "{day},{local},{captured},{ship},{captured},{},4096,0,0,1,0,1,1,1,{}",
                u8::from(!final_export),
                "ab".repeat(32)
            ));
        }
        lines.join("\n")
    }
    #[test]
    fn month_requires_distinct_local_capture_and_follower_observations() {
        let csv = csv();
        let good = observations(&csv).unwrap();
        assert_eq!(good["final_revision"], 93);
        assert_eq!(good["lost_reply_days"], json!([7, 14, 21, 28]));
        for (field, value) in [
            (0, "2"),
            (1, "5"),
            (2, "6"),
            (3, "6"),
            (4, "6"),
            (5, "0"),
            (6, "0"),
            (6, "67108865"),
            (7, "-1"),
            (14, "invalid"),
        ] {
            let mut lines: Vec<_> = csv.lines().map(str::to_owned).collect();
            let mut fields: Vec<_> = lines[1].split(',').map(str::to_owned).collect();
            fields[field] = value.into();
            lines[1] = fields.join(",");
            assert!(
                observations(&lines.join("\n")).is_err(),
                "field {field}: {value}"
            );
        }
    }
    #[test]
    fn month_rejects_missing_extra_and_fabricated_lost_reply_receipts() {
        let csv = csv();
        assert!(observations(&csv.lines().take(31).collect::<Vec<_>>().join("\n")).is_err());
        assert!(observations(&format!("{csv}\n{}", csv.lines().last().unwrap())).is_err());
        assert!(observations(&csv.replacen(HEADER, "stale", 1)).is_err());
        let mut lines: Vec<_> = csv.lines().map(str::to_owned).collect();
        let mut day: Vec<_> = lines[7].split(',').map(str::to_owned).collect();
        day[3] = "23".into();
        lines[7] = day.join(",");
        assert!(observations(&lines.join("\n")).is_err());
        let mut lines: Vec<_> = csv.lines().map(str::to_owned).collect();
        let mut last: Vec<_> = lines[31].split(',').map(str::to_owned).collect();
        last[10] = "1".into();
        lines[31] = last.join(",");
        assert!(observations(&lines.join("\n")).is_err());
    }
}
