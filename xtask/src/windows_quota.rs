//! Actual owner quota on a disposable fixed NTFS VHD, without a new host account.
#[path = "../../tools/quota/windows_identity.rs"]
mod identity;
use crate::{
    delivery, quota_append,
    support::{Run, path, sha},
    trace_process::{admission, call, write_proof},
    windows_volume::{self, Capacity},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        CreateWellKnownSid, EqualSid, LookupAccountSidW, OWNER_SECURITY_INFORMATION, TOKEN_QUERY,
        WinLocalServiceSid,
    },
};

const MIB: u64 = 1024 * 1024;
const LIMIT: u64 = MIB;
const RAISED_LIMIT: u64 = 4 * MIB;
const CLUSTER: usize = 4096;

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    fixture: PathBuf,
    #[arg(long)]
    source: String,
    #[arg(long)]
    output: PathBuf,
}

pub(crate) struct Owner {
    sid: [u32; 17],
    pub(crate) account: String,
}

impl Owner {
    pub(crate) fn local_service() -> Result<Self> {
        let mut sid = [0; 17];
        let mut bytes = std::mem::size_of_val(&sid) as u32;
        // SAFETY: initialized, aligned SID buffer and valid size output.
        unsafe {
            ensure!(
                CreateWellKnownSid(
                    WinLocalServiceSid,
                    null_mut(),
                    sid.as_mut_ptr().cast(),
                    &mut bytes
                ) != 0,
                "construct owner SID: {}",
                io::Error::last_os_error()
            );
        }
        Self::resolve(sid)
    }

    pub(crate) fn current_user() -> Result<Self> {
        Self::resolve(identity::token_user(&identity::process_token(
            TOKEN_QUERY,
        )?)?)
    }

    fn resolve(mut sid: [u32; 17]) -> Result<Self> {
        let mut name = [0; 256];
        let mut domain = [0; 256];
        let (mut name_len, mut domain_len, mut kind) = (name.len() as u32, domain.len() as u32, 0);
        // SAFETY: valid owned SID buffer and bounded Windows outputs.
        unsafe {
            ensure!(
                LookupAccountSidW(
                    null(),
                    sid.as_mut_ptr().cast(),
                    name.as_mut_ptr(),
                    &mut name_len,
                    domain.as_mut_ptr(),
                    &mut domain_len,
                    &mut kind
                ) != 0,
                "resolve owner SID: {}",
                io::Error::last_os_error()
            );
        }
        let account = format!(
            "{}\\{}",
            String::from_utf16(&domain[..domain_len as usize])?,
            String::from_utf16(&name[..name_len as usize])?
        );
        Ok(Self { sid, account })
    }

    pub(crate) fn verify(&self, file: &Path) -> Result<()> {
        let name: Vec<u16> = file.as_os_str().encode_wide().chain([0]).collect();
        let (mut owner, mut descriptor) = (null_mut(), null_mut());
        // SAFETY: terminated file path, live outputs and an owned descriptor on success.
        let result = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32))
                .context("query quota file ownership");
        }
        // SAFETY: compare the requested owner while its descriptor is live, then free it.
        let matches = unsafe {
            let matches =
                !owner.is_null() && EqualSid(owner, self.sid.as_ptr().cast_mut().cast()) != 0;
            LocalFree(descriptor);
            matches
        };
        ensure!(matches, "quota file owner differs from the selected SID");
        Ok(())
    }
}

pub(crate) fn exhaust(ballast: &Path, ceiling: u64) -> Result<(i32, u64)> {
    let mut file = OpenOptions::new().write(true).open(ballast)?;
    let mut bytes = file.seek(SeekFrom::End(0))?;
    let block = [0x5a; 64 * 1024];
    let mut count = block.len();
    loop {
        ensure!(
            bytes + count as u64 <= ceiling,
            "NTFS quota did not enforce within the independent {ceiling}-byte bound"
        );
        match file.write(&block[..count]) {
            Ok(n) if n > 0 => bytes += n as u64,
            Err(error) if matches!(error.raw_os_error(), Some(112 | 1295)) => {
                if count == CLUSTER {
                    file.sync_all()?;
                    return Ok((error.raw_os_error().unwrap(), bytes));
                }
                count = CLUSTER;
            }
            other => anyhow::bail!("unexpected native ballast result {other:?}"),
        }
    }
}

struct Trial<'a> {
    output: &'a Path,
    source: &'a str,
    fixture: &'a Path,
    env: &'a BTreeMap<OsString, OsString>,
    records: Vec<Value>,
    processes: BTreeSet<(u32, u64)>,
}

impl Trial<'_> {
    fn system(
        &self,
        proof: &mut Value,
        tool: &str,
        label: &str,
        args: impl IntoIterator<Item = OsString>,
    ) -> Result<()> {
        let mut argv = vec![path(windows_volume::system_command(tool)?)];
        argv.extend(args);
        call(
            proof,
            self.output,
            argv,
            self.output,
            label,
            self.env,
            false,
        )?;
        Ok(())
    }

    fn case(
        &mut self,
        proof: &mut Value,
        database: &Path,
        label: &str,
        stage: &str,
        revision: u64,
        code: i32,
    ) -> Result<()> {
        let mut args = vec![path(self.fixture), stage.into(), path(database)];
        if stage == "inspect" {
            args.push(revision.to_string().into());
        }
        let mut env = self.env.clone();
        env.insert("WEEKEND_DB_SOURCE_ID".into(), self.source.into());
        env.insert(
            "WEEKEND_PRIVATE_QUOTA_ERRNO".into(),
            code.to_string().into(),
        );
        let report = call(proof, self.output, args, self.output, label, &env, false)?;
        let pid = report.pid.context("quota fixture process ID")?;
        let created = report
            .creation_time_100ns
            .context("quota fixture creation time")?;
        ensure!(
            self.processes.insert((pid, created)),
            "quota phases require distinct processes"
        );
        let row = delivery::read_json(&self.output.join(format!("{label}.stdout")))?;
        quota_append::verify(&row, self.source, stage, revision, pid, code)?;
        self.records
            .push(json!({"label":label,"pid":pid,"observation":row}));
        Ok(())
    }

    fn calibrate(&self, proof: &mut Value, data: &Path, ballast: &Path) -> Result<(i32, u64)> {
        let (code, bytes) = exhaust(ballast, 2 * LIMIT)?;
        let free = windows_volume::space(data, Capacity::Append)?;
        ensure!(
            free > 32 * MIB,
            "physical disk lacks independent quota headroom"
        );
        proof["calibration"] =
            json!({"native_errno":code,"ballast_bytes":bytes,"global_free_bytes":free});
        Ok((code, bytes))
    }

    fn run(&mut self, proof: &mut Value, data: &Path) -> Result<()> {
        let root = data.parent().context("private drive root")?;
        let root = root.to_str().context("UTF-8 private drive root")?;
        let drive = root.strip_suffix('\\').context("private drive letter")?;
        ensure!(
            drive.len() == 2
                && drive.as_bytes()[0].is_ascii_uppercase()
                && drive.ends_with(':')
                && proof["private_volume"]["root"] == root,
            "quota administration needs the bound drive letter"
        );
        let owner = Owner::local_service()?;
        let database = data.join("database.wdb");
        self.case(proof, &database, "seed", "seed", 3, 112)?;
        let initial = sha(&database)?;
        let ballast = data.join("ballast");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&ballast)?;
        for (file, label) in [(&database, "own-database"), (&ballast, "own-ballast")] {
            self.system(
                proof,
                "icacls.exe",
                label,
                [
                    path(file),
                    "/setowner".into(),
                    owner.account.as_str().into(),
                ],
            )?;
            owner.verify(file)?;
        }
        proof["owner"] = json!({"account":owner.account,"sid":"S-1-5-19","database_verified":true,"ballast_verified":true});
        self.system(
            proof,
            "fsutil.exe",
            "track",
            ["quota".into(), "track".into(), drive.into()],
        )?;
        self.system(
            proof,
            "fsutil.exe",
            "limit",
            [
                "quota".into(),
                "modify".into(),
                drive.into(),
                (LIMIT / 2).to_string().into(),
                LIMIT.to_string().into(),
                owner.account.as_str().into(),
            ],
        )?;
        self.system(
            proof,
            "fsutil.exe",
            "enforce",
            ["quota".into(), "enforce".into(), drive.into()],
        )?;
        self.system(
            proof,
            "fsutil.exe",
            "query-limit",
            ["quota".into(), "query".into(), drive.into()],
        )?;
        let (code, bytes) = self.calibrate(proof, data, &ballast)?;
        self.case(proof, &database, "append-fail", "fail", 3, code)?;
        let failed = sha(&database)?;
        self.case(proof, &database, "reopen-at-limit", "inspect", 3, code)?;
        ensure!(
            sha(&database)? == failed && exhaust(&ballast, 2 * LIMIT)? == (code, bytes),
            "open changed the failed image or quota boundary"
        );
        let global = windows_volume::space(data, Capacity::Append)?;
        ensure!(
            global > 32 * MIB,
            "insufficient volume space independently of the quota"
        );

        self.system(
            proof,
            "fsutil.exe",
            "raise-limit",
            [
                "quota".into(),
                "modify".into(),
                drive.into(),
                (RAISED_LIMIT / 2).to_string().into(),
                RAISED_LIMIT.to_string().into(),
                owner.account.as_str().into(),
            ],
        )?;
        self.system(
            proof,
            "fsutil.exe",
            "query-raised",
            ["quota".into(), "query".into(), drive.into()],
        )?;
        ensure!(
            fs::metadata(&ballast)?.len() == bytes,
            "ballast changed before quota was raised"
        );
        let mut control = OpenOptions::new().append(true).open(&ballast)?;
        control.write_all(&[0x6b; CLUSTER])?;
        control.sync_all()?;
        drop(control);
        self.case(proof, &database, "quota-retry", "retry", 4, code)?;
        self.case(proof, &database, "reopen-after-retry", "inspect", 4, code)?;
        owner.verify(&database)?;
        let final_hash = sha(&database)?;
        let archive = self.output.join("database.wdb");
        fs::copy(&database, &archive)?;
        ensure!(
            failed != final_hash && sha(&archive)? == final_hash,
            "retry image verification failed"
        );
        proof["results"] = json!(self.records);
        proof["retry"] = json!({"original_limit":LIMIT,"raised_limit":RAISED_LIMIT,"write_control_bytes":CLUSTER,
            "ballast_preserved":true,"global_free_before_raise":global,"seed_sha256":initial,"full_reopen_sha256":failed,"retry_sha256":final_hash});
        Ok(())
    }
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    ensure!(
        opts.source.len() == 40
            && opts
                .source
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "exact fixture source SHA required"
    );
    let output = admission(&run.root, &opts.output)?;
    let binary = dunce::canonicalize(run.root.join(opts.fixture))?;
    ensure!(
        binary.is_file() && !binary.starts_with(&output),
        "fixture must be outside output"
    );
    let digest = sha(&binary)?;
    fs::create_dir_all(output.parent().context("output parent")?)?;
    fs::create_dir(&output)?;
    let fixture = output.join("fixture.exe");
    fs::copy(&binary, &fixture)?;
    let env = run
        .env
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.to_string_lossy().to_ascii_uppercase().as_str(),
                "SYSTEMROOT" | "WINDIR" | "COMSPEC" | "TEMP" | "TMP" | "PATH"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut trial = Trial {
        output: &output,
        source: &opts.source,
        fixture: &fixture,
        env: &env,
        records: vec![],
        processes: BTreeSet::new(),
    };
    let mut proof = json!({"verified":false,"source_id":opts.source,"fixture_sha256":digest,
        "fixture_input_sha256":sha(&run.root.join("tools/quota/fixture.rs"))?,"runner_sha256":sha(&std::env::current_exe()?)?,"commands":[]});
    let result = windows_volume::with_volume(
        &mut proof,
        &output,
        &env,
        Capacity::Append,
        |proof, data| trial.run(proof, data),
    )
    .and_then(|()| {
        ensure!(
            sha(&binary)? == digest && sha(&fixture)? == digest,
            "executed fixture changed"
        );
        let mut files = delivery::inventory(&output)?;
        files.remove("evidence.json");
        proof["artifact_sha256"] = json!(files);
        Ok(())
    });
    proof["verified"] = json!(result.is_ok());
    if let Err(error) = &result {
        proof["failure"] = json!(format!("{error:#}"));
    }
    write_proof(&output, &proof)?;
    result?;
    println!("{}", output.join("evidence.json").display());
    Ok(())
}
