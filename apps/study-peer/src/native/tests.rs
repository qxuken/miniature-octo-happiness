use super::*;
use std::{
    ffi::OsStr,
    fs,
    io::{BufReader, Read},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use weekend_db::crdt::Policy;

const WORKER: &str = "STUDY_PEER_TEST_WORKER";
const COLLECTION: [u8; 16] = [9; 16];

pub(super) struct Directory(pub(super) PathBuf);
impl Directory {
    pub(super) fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "study-peer-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn roster_policy() -> Policy {
    Enrollment::new("a", COLLECTION).unwrap().policy
}

fn hex_identity(bytes: [u8; 16]) -> String {
    format!("{:032x}", u128::from_be_bytes(bytes))
}

fn write_roster_policy(file: &Path, policy: Policy) {
    fs::write(
        file,
        format!(
            "study-policy-v1 {} {} {} {}\n",
            hex_identity(policy.collection),
            policy.generation,
            hex_identity(policy.actors[0]),
            hex_identity(policy.actors[1]),
        ),
    )
    .unwrap();
}

fn invoke(command: &str, arguments: &[&OsStr]) -> AppResult<()> {
    super::main(
        std::iter::once(OsString::from(command))
            .chain(arguments.iter().map(|argument| argument.to_os_string()))
            .collect(),
    )
}

#[test]
fn run_options_accept_each_flag_once() {
    let parse = |flags: &[&str]| {
        let arguments = flags
            .iter()
            .copied()
            .map(OsString::from)
            .collect::<Vec<_>>();
        RunOptions::parse(&arguments)
    };
    for (flags, pushing, sync_only) in [
        (&[][..], true, false),
        (&["--no-push"][..], false, false),
        (&["--sync-only"][..], true, true),
        (&["--no-push", "--sync-only"][..], false, true),
        (&["--sync-only", "--no-push"][..], false, true),
    ] {
        let options = parse(flags).unwrap();
        assert_eq!((options.pushing, options.sync_only), (pushing, sync_only));
    }
    for invalid in [
        &["--extra"][..],
        &["--no-push", "--no-push"][..],
        &["--sync-only", "--sync-only"][..],
        &["--no-push", "--sync-only", "--no-push"][..],
        &["--sync-only", "--no-push", "--sync-only"][..],
    ] {
        assert_eq!(parse(invalid).err().unwrap().to_string(), USAGE);
    }
}

#[test]
fn receipt_argument_reads_only_the_selected_file_and_preserves_errors() {
    let directory = Directory::new();
    let path = directory.0.join("receipt");
    let path_arg = path.as_os_str().to_owned();
    let flags = ["--expect-old", "--expect-staged"];
    for (flag, other) in flags.into_iter().zip(flags.into_iter().rev()) {
        assert_eq!(expected_receipt(&[], flag).unwrap(), None);
        for wrong in [
            vec![flag.into()],
            vec![other.into(), path_arg.clone()],
            vec![flag.into(), path_arg.clone(), "extra".into()],
        ] {
            assert_eq!(
                expected_receipt(&wrong, flag).unwrap_err().to_string(),
                USAGE
            );
        }
    }
    let arguments = ["--expect-old".into(), path_arg];
    assert_eq!(
        expected_receipt(&arguments, "--expect-old")
            .unwrap_err()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    let bytes = std::array::from_fn::<_, 128, _>(|index| index as u8);
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        expected_receipt(&arguments, "--expect-old")
            .unwrap()
            .unwrap(),
        bytes
    );
    fs::write(&path, [0; 129]).unwrap();
    assert_eq!(
        expected_receipt(&arguments, "--expect-old")
            .unwrap_err()
            .to_string(),
        "credential size outside bounds"
    );
}

#[test]
fn local_startup_dispatch_checks_arguments_before_opening_paths() {
    invoke("--help", &[]).unwrap();
    invoke("-h", &[]).unwrap();

    let directory = Directory::new();
    let keys = directory.0.join("keys");
    let database = directory.0.join("study.wdb");
    let missing = directory.0.join("missing");
    let absent_database = missing.join("study.wdb");
    let id = hex_identity(COLLECTION);
    let args = |file: &Path, actor: &'static str| {
        [
            file.as_os_str().to_owned(),
            OsString::from(actor),
            id.clone().into(),
        ]
    };
    assert_eq!(
        invoke("wrong", &[keys.as_os_str()])
            .unwrap_err()
            .to_string(),
        USAGE
    );
    assert!(!keys.exists());
    let keygen = invoke("keygen", &[keys.as_os_str()]);
    #[cfg(unix)]
    {
        keygen.unwrap();
        for name in ["cert.der", "key.der"] {
            assert!(fs::metadata(keys.join(name)).unwrap().len() > 0);
        }
    }
    #[cfg(windows)]
    {
        assert_eq!(
            keygen.unwrap_err().to_string(),
            "keygen requires Unix private-file modes; provision PKCS#8 key.der and cert.der in an ACL-protected directory on Windows"
        );
        assert!(!keys.exists());
    }
    let rejected = args(&database, "c");
    assert_eq!(
        invoke("init", &rejected.each_ref().map(OsString::as_os_str))
            .unwrap_err()
            .to_string(),
        "actor must be a or b"
    );
    let accepted = args(&database, "a");
    let accepted = accepted.each_ref().map(OsString::as_os_str);
    assert_eq!(invoke("wrong", &accepted).unwrap_err().to_string(), USAGE);
    assert!(!database.exists());
    let missing_parent = args(&absent_database, "a");
    assert_eq!(
        invoke("init", &missing_parent.each_ref().map(OsString::as_os_str))
            .unwrap_err()
            .downcast_ref::<Error>(),
        Some(&Error::NotFound)
    );
    invoke("init", &accepted).unwrap();
    let bytes = fs::read(&database).unwrap();
    assert_eq!(
        invoke("init", &accepted)
            .unwrap_err()
            .downcast_ref::<Error>(),
        Some(&Error::AlreadyExists)
    );
    assert_eq!(fs::read(&database).unwrap(), bytes);
    {
        let db = Database::open_file(&database, Limits::default()).unwrap();
        let enrollment = Enrollment::new("a", COLLECTION).unwrap();
        assert_eq!(db.snapshot().revision(), 2);
        let replica = StudyReplica::open(
            &db,
            db.collection_id(COLLECTION_ROW),
            enrollment.policy,
            enrollment.actor,
        )
        .unwrap();
        assert_eq!(replica.read().unwrap(), study::StudyProgress::default());
    }

    let pin = missing.join("peer.der");
    let mut run = accepted.to_vec();
    run.extend([
        OsStr::new("127.0.0.1:0"),
        OsStr::new("127.0.0.1:0"),
        missing.as_os_str(),
        pin.as_os_str(),
    ]);
    run.push(OsStr::new("--extra"));
    assert_eq!(invoke("run", &run).unwrap_err().to_string(), USAGE);
    run.pop();
    assert_eq!(
        invoke("run", &run)
            .unwrap_err()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(fs::read(&database).unwrap(), bytes);
}

#[test]
fn service_startup_preserves_native_errors_and_releases_resources() {
    #[derive(Clone, Copy)]
    struct Startup<'a> {
        database: &'a Path,
        actor: &'a OsStr,
        config: &'a OsStr,
        listen: &'a OsStr,
        peer: &'a OsStr,
        credentials: &'a Path,
        pin: &'a Path,
    }

    let directory = Directory::new();
    let [
        database,
        policy,
        credentials,
        peer_credentials,
        missing,
        malformed,
    ] = [
        "study.wdb",
        "policy",
        "local",
        "peer",
        "missing",
        "malformed",
    ]
    .map(|name| directory.0.join(name));
    let enrollment = Enrollment::new("a", COLLECTION).unwrap();
    initialize(&database, enrollment).unwrap();
    write_roster_policy(&policy, enrollment.policy);
    Credentials::generate().unwrap().write_fixture(&credentials);
    Credentials::generate()
        .unwrap()
        .write_fixture(&peer_credentials);
    fs::write(&malformed, [0]).unwrap();
    let original = fs::read(&database).unwrap();
    let id = hex_identity(COLLECTION);
    let pin = peer_credentials.join("cert.der");
    let direct = Startup {
        database: &database,
        actor: OsStr::new("a"),
        config: OsStr::new(&id),
        listen: OsStr::new("127.0.0.1:0"),
        peer: OsStr::new("127.0.0.1:1"),
        credentials: &credentials,
        pin: &pin,
    };
    let configured = Startup {
        config: policy.as_os_str(),
        ..direct
    };
    let reject = |command: &str, args: Startup<'_>| {
        let error = invoke(
            command,
            &[
                args.database.as_os_str(),
                args.actor,
                args.config,
                args.listen,
                args.peer,
                args.credentials.as_os_str(),
                args.pin.as_os_str(),
                OsStr::new("--no-push"),
            ],
        )
        .unwrap_err();
        assert_eq!(fs::read(&database).unwrap(), original);
        let db = Database::open_file(&database, Limits::default()).unwrap();
        assert_eq!(db.snapshot().revision(), 2);
        error
    };
    let run = |args| reject("run", args);
    let run_policy = |args| reject("run-policy", args);
    let actor = OsStr::new("c");
    assert_eq!(
        run(Startup { actor, ..direct }).to_string(),
        "actor must be a or b"
    );
    assert_eq!(
        run_policy(Startup {
            actor,
            ..configured
        })
        .to_string(),
        "actor must be a or b"
    );
    #[cfg(unix)]
    let invalid_utf8 = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let invalid_utf8 = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    };
    for error in [
        run_policy(Startup {
            actor: &invalid_utf8,
            config: missing.as_os_str(),
            ..configured
        }),
        run(Startup {
            listen: &invalid_utf8,
            ..direct
        }),
        run(Startup {
            peer: &invalid_utf8,
            ..direct
        }),
    ] {
        assert_eq!(error.to_string(), "expected a UTF-8 argument");
    }
    for error in [
        run_policy(Startup {
            config: missing.as_os_str(),
            ..configured
        }),
        run(Startup {
            credentials: &missing,
            pin: &malformed,
            ..direct
        }),
        run(Startup {
            pin: &missing,
            ..direct
        }),
    ] {
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::NotFound)
        );
    }
    assert_eq!(
        run_policy(Startup {
            config: malformed.as_os_str(),
            ..configured
        })
        .downcast_ref::<Error>(),
        Some(&Error::UnsupportedFormat)
    );
    assert!(
        run(Startup {
            pin: &malformed,
            database: &missing,
            ..direct
        })
        .is::<rustls::Error>()
    );
    assert_eq!(
        run(Startup {
            database: &missing,
            listen: OsStr::new("bad address"),
            ..direct
        })
        .downcast_ref::<Error>(),
        Some(&Error::NotFound)
    );

    let bad_address = OsStr::new("bad address");
    assert!(
        run(Startup {
            listen: bad_address,
            ..direct
        })
        .is::<std::net::AddrParseError>()
    );
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = held.local_addr().unwrap();
    let listen = address.to_string();
    let listen = OsStr::new(&listen);
    let refusal = TcpListener::bind(address).unwrap_err();
    let bind_error = run(Startup { listen, ..direct });
    let bind_error = bind_error.downcast_ref::<io::Error>().unwrap();
    assert_eq!(bind_error.kind(), refusal.kind());
    assert_eq!(bind_error.raw_os_error(), refusal.raw_os_error());
    drop(held);
    assert!(
        run(Startup {
            listen,
            peer: bad_address,
            ..direct
        })
        .is::<std::net::AddrParseError>()
    );
    let rebound = TcpListener::bind(address).unwrap();
    drop(rebound);
    assert_eq!(
        run_policy(Startup {
            actor: OsStr::new("b"),
            ..configured
        })
        .downcast_ref::<Error>(),
        Some(&Error::ForeignStore)
    );
    assert!(!missing.exists());
}

#[test]
fn roster_dispatch_preserves_configuration_and_receipt_errors() {
    let directory = Directory::new();
    let [a, b, old, new, bad, new_a, new_b, missing] =
        ["a", "b", "old", "new", "bad", "new-a", "new-b", "missing"]
            .map(|name| directory.0.join(name));
    let policy = roster_policy();
    write_roster_policy(&old, policy);
    write_roster_policy(
        &new,
        Policy {
            collection: [10; 16],
            generation: policy.generation + 1,
            actors: [[3; 16], [4; 16]],
        },
    );
    fs::write(&bad, "unsupported-policy-v2").unwrap();
    for (actor, path) in [("a", &a), ("b", &b)] {
        initialize(path, Enrollment::new(actor, COLLECTION).unwrap()).unwrap();
    }
    let sources = [&a, &b];
    let seen = sources.map(|path| fs::read(path).unwrap());
    let checkpoint = |policy: &Path| {
        invoke(
            "checkpoint-roster",
            &[a.as_os_str(), b.as_os_str(), policy.as_os_str()],
        )
    };
    let stage = |old: &Path, new: &Path, receipt: Option<&Path>| {
        let mut args = vec![
            a.as_os_str(),
            b.as_os_str(),
            old.as_os_str(),
            new_a.as_os_str(),
            new_b.as_os_str(),
            new.as_os_str(),
        ];
        if let Some(receipt) = receipt {
            args.extend([OsStr::new("--expect-old"), receipt.as_os_str()]);
        }
        invoke("stage-roster", &args)
    };
    let verify = |policy: &Path, receipt: Option<&Path>| {
        let mut args = vec![a.as_os_str(), b.as_os_str(), policy.as_os_str()];
        if let Some(receipt) = receipt {
            args.extend([OsStr::new("--expect-staged"), receipt.as_os_str()]);
        }
        invoke("verify-roster", &args)
    };
    for result in [
        checkpoint(&bad),
        stage(&bad, &new, None),
        stage(&old, &bad, None),
        verify(&bad, None),
    ] {
        assert_eq!(
            result.unwrap_err().downcast_ref::<Error>(),
            Some(&Error::UnsupportedFormat)
        );
    }
    for result in [
        stage(&old, &new, Some(&missing)),
        verify(&old, Some(&missing)),
    ] {
        assert_eq!(
            result
                .unwrap_err()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
    }
    for arguments in [
        vec![a.as_os_str(), b.as_os_str(), old.as_os_str()],
        vec![
            a.as_os_str(),
            b.as_os_str(),
            old.as_os_str(),
            new_a.as_os_str(),
            new_b.as_os_str(),
            new.as_os_str(),
        ],
    ] {
        assert_eq!(invoke("wrong", &arguments).unwrap_err().to_string(), USAGE);
    }
    assert!(!new_a.exists() && !new_b.exists());
    assert_eq!(sources.map(|path| fs::read(path).unwrap()), seen);
}

#[test]
fn roster_policy_requires_exact_syntax_and_preserves_input_errors() {
    let directory = Directory::new();
    let path = directory.0.join("policy");
    let error = || roster::policy_file(&path).expect_err("invalid policy");
    assert_eq!(
        error().downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    fs::write(&path, [b'a'; 257]).unwrap();
    assert_eq!(error().to_string(), "credential size outside bounds");
    fs::write(&path, [0xff]).unwrap();
    assert!(error().is::<std::str::Utf8Error>());

    let policy = roster_policy();
    write_roster_policy(&path, policy);
    let valid = fs::read_to_string(&path).unwrap();
    let words: Vec<_> = valid.split_ascii_whitespace().collect();
    assert_eq!(roster::policy_file(&path).unwrap(), policy);
    for invalid in [
        valid.replace("study-policy-v1", "study-policy-v2"),
        words[..4].join(" "),
        format!("{valid} unexpected"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert_eq!(
            error().downcast_ref::<Error>(),
            Some(&Error::UnsupportedFormat)
        );
    }
    for field in [1, 3, 4] {
        for invalid in ["1".repeat(31), "g".repeat(32)] {
            let mut malformed = words.clone();
            malformed[field] = &invalid;
            fs::write(&path, malformed.join(" ")).unwrap();
            assert_eq!(
                error().to_string(),
                "identity must be 32 hexadecimal digits"
            );
        }
    }
    let mut malformed = words;
    malformed[2] = "18446744073709551616";
    fs::write(&path, malformed.join(" ")).unwrap();
    assert!(error().is::<std::num::ParseIntError>());
}

#[test]
fn roster_policy_identity_generation_and_actor_checks_are_independent() {
    let directory = Directory::new();
    let path = directory.0.join("policy");
    let valid = roster_policy();
    write_roster_policy(&path, valid);
    assert_eq!(roster::policy_file(&path).unwrap(), valid);
    for invalid in [
        Policy {
            collection: [0; 16],
            ..valid
        },
        Policy {
            generation: 0,
            ..valid
        },
        Policy {
            actors: [[0; 16], valid.actors[1]],
            ..valid
        },
        Policy {
            actors: [valid.actors[1]; 2],
            ..valid
        },
    ] {
        write_roster_policy(&path, invalid);
        assert_eq!(
            roster::policy_file(&path)
                .err()
                .unwrap()
                .downcast_ref::<Error>(),
            Some(&Error::Invalid)
        );
    }
    for (name, actor) in ["a", "b"].into_iter().zip(valid.actors) {
        let enrollment = roster::enrollment(name, valid).unwrap();
        assert_eq!(enrollment.actor, actor);
        assert_eq!(enrollment.policy, valid);
    }
    assert_eq!(
        roster::enrollment("", valid).err().unwrap().to_string(),
        "actor must be a or b"
    );
}

#[test]
fn roster_stage_requires_fresh_collection_generation_and_actors() {
    let directory = Directory::new();
    let old_files = ["old-a", "old-b"].map(|name| directory.0.join(name));
    let new_files = ["new-a", "new-b"].map(|name| directory.0.join(name));
    let old = roster_policy();
    let new = Policy {
        collection: [10; 16],
        generation: old.generation + 1,
        actors: [[3; 16], [4; 16]],
    };
    for (name, path) in ["a", "b"].into_iter().zip(&old_files) {
        let db = Database::create_file(path, Limits::default()).unwrap();
        enroll(&db, roster::enrollment(name, old).unwrap()).unwrap();
        db.retain_snapshot(&db.snapshot()).unwrap();
    }
    let original = old_files.each_ref().map(|file| fs::read(file).unwrap());
    let stage = |policy| {
        roster::stage(
            old_files.each_ref().map(|file| file.as_path()),
            old,
            new_files.each_ref().map(|file| file.as_path()),
            policy,
            None,
            &mut io::sink(),
        )
    };
    for invalid in [
        Policy {
            collection: old.collection,
            ..new
        },
        Policy {
            generation: old.generation,
            ..new
        },
        Policy {
            actors: [old.actors[0], new.actors[1]],
            ..new
        },
    ] {
        assert_eq!(
            stage(invalid).unwrap_err().downcast_ref::<Error>(),
            Some(&Error::Invalid)
        );
        assert!(new_files.iter().all(|path| !path.exists()));
        assert_eq!(
            old_files.each_ref().map(|file| fs::read(file).unwrap()),
            original
        );
    }
    stage(new).unwrap();
    for (path, actor) in new_files.iter().zip(new.actors) {
        let db = Database::open_file(path, Limits::default()).unwrap();
        let replica =
            StudyReplica::open(&db, db.collection_id(COLLECTION_ROW), new, actor).unwrap();
        assert_eq!(replica.read().unwrap(), study::StudyProgress::default());
    }
    assert_eq!(
        old_files.each_ref().map(|file| fs::read(file).unwrap()),
        original
    );
}

#[test]
fn cli_command_observation_preserves_storage_context_errors_and_retry_budget() {
    let directory = Directory::new();
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Study,
        &mut vec!["--diagnostics=json".into(), "run".into()],
    );
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| {
            Ok(
                Database::open_file(directory.0.join("missing"), Limits::default())?
                    .snapshot()
                    .revision(),
            )
        })
    });
    assert_eq!(
        result.as_ref().err().unwrap().downcast_ref::<Error>(),
        Some(&Error::NotFound)
    );
    let json = diagnostic.unwrap().json(settings).to_string();
    assert!(
        json.contains("\"phase\":\"local_command\",\"category\":\"engine\",\"code\":\"NotFound\"")
    );
    assert!(json.contains("\"native\":{\"operation\":"));

    let database = Database::new().unwrap();
    let replica = enroll(&database, Enrollment::new("a", COLLECTION).unwrap()).unwrap();
    let before = database.snapshot().revision();
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| command(&["practice", "PRIVATE_CANARY"], &database, &replica))
    });
    assert!(
        result
            .as_ref()
            .err()
            .unwrap()
            .is::<std::num::ParseIntError>()
    );
    let json = diagnostic.unwrap().json(settings).to_string();
    assert!(
        json.contains(
            "\"phase\":\"local_command\",\"category\":\"input\",\"code\":\"invalid_input\""
        )
    );
    assert!(json.contains("\"native\":null,\"verification\":null"));
    assert!(!json.contains("PRIVATE_CANARY"));
    assert_eq!(database.snapshot().revision(), before);

    let calls = std::cell::Cell::new(0);
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| {
            Ok(protocol::edit(&replica, |_, _| {
                calls.set(calls.get() + 1);
                Err(Error::Conflict)
            })?)
        })
    });
    assert_eq!(calls.get(), 3);
    assert_eq!(
        result.as_ref().err().unwrap().downcast_ref::<Error>(),
        Some(&Error::Conflict)
    );
    assert!(
        diagnostic
            .unwrap()
            .json(settings)
            .to_string()
            .contains("\"code\":\"Conflict\",\"native\":null,\"verification\":null")
    );
    assert_eq!(database.snapshot().revision(), before);
}

struct LimitedOutput {
    remaining: usize,
    bytes: Vec<u8>,
}

impl Write for LimitedOutput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let length = self.remaining.min(input.len());
        self.bytes.extend_from_slice(&input[..length]);
        self.remaining -= length;
        Ok(length)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct InteractiveObservation {
    result: AppResult<()>,
    diagnostic: Option<String>,
    replies: String,
    revision: u64,
    minutes: u64,
}

fn interactive_case(
    input: &[u8],
    sync_only: bool,
    structured: bool,
    refuse_replies: bool,
) -> InteractiveObservation {
    let directory = Directory::new();
    let path = directory.0.join("study.wdb");
    let enrollment = Enrollment::new("a", COLLECTION).unwrap();
    initialize(&path, enrollment).unwrap();
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let peer = directory.0.join("peer");
    Credentials::generate().unwrap().write_fixture(&peer);
    let tls = Tls::new(
        &Credentials::generate().unwrap(),
        fs::read(peer.join("cert.der")).unwrap().into(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let ready = format!("ready {address}\n");
    let network = Network {
        listener,
        peer: address,
        tls,
    };
    let options = RunOptions {
        pushing: false,
        sync_only,
    };
    let mut output = LimitedOutput {
        remaining: if refuse_replies {
            ready.len()
        } else {
            usize::MAX
        },
        bytes: Vec::new(),
    };
    let mut arguments = Vec::new();
    if structured {
        arguments.push("--diagnostics=json".into());
    }
    let settings = diagnostics::settings_from_args(diagnostics::App::Study, &mut arguments);
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        run_with_io(
            &db,
            enrollment,
            network,
            options,
            |stream| stream,
            io::Cursor::new(input),
            &mut output,
        )
    });
    // A returned command result owns no listener. Reopening also checks the
    // committed state through the actual file adapter.
    let rebound = TcpListener::bind(address).unwrap();
    drop(rebound);
    drop(db);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let replica = StudyReplica::open(
        &db,
        db.collection_id(COLLECTION_ROW),
        enrollment.policy,
        enrollment.actor,
    )
    .unwrap();
    let replies = String::from_utf8(output.bytes).unwrap();
    InteractiveObservation {
        result,
        diagnostic: diagnostic.map(|value| value.json(settings).to_string()),
        replies: replies.strip_prefix(&ready).unwrap().into(),
        revision: db.snapshot().revision(),
        minutes: replica.read().unwrap().practice_minutes.total(),
    }
}

fn with_interactive_deadline(test: impl FnOnce() + Send + 'static) {
    let (finish, completed) = mpsc::channel();
    let worker = thread::spawn(move || {
        test();
        finish.send(()).unwrap();
    });
    assert!(
        completed.recv_timeout(Duration::from_secs(20)) != Err(mpsc::RecvTimeoutError::Timeout),
        "interactive service did not shut down"
    );
    worker.join().unwrap();
}

#[test]
fn service_launch_failure_stops_and_joins_an_already_started_receiver() {
    with_interactive_deadline(|| {
        let mut arguments = vec!["--diagnostics=json".into(), "run".into()];
        let settings = diagnostics::settings_from_args(diagnostics::App::Study, &mut arguments);
        for (fail_at, panic_receiver) in [(0, false), (1, false), (2, false), (2, true)] {
            let stop = AtomicBool::new(false);
            let received = AtomicBool::new(false);
            let sent = AtomicBool::new(false);
            let (entered, enter) = mpsc::channel();
            let mut attempts = 0;
            let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
                thread::scope(|scope| {
                    let (receiving, outgoing) = launch_workers(
                        scope,
                        &stop,
                        || {
                            entered.send(()).unwrap();
                            let deadline = Instant::now() + Duration::from_secs(4);
                            while !stop.load(Relaxed) {
                                assert!(Instant::now() < deadline, "receiver did not stop");
                                thread::park_timeout(Duration::from_millis(25));
                            }
                            received.store(true, Relaxed);
                            assert!(!panic_receiver, "controlled receiving worker panic");
                        },
                        || sent.store(true, Relaxed),
                        || {
                            attempts += 1;
                            if attempts == fail_at {
                                if attempts == 2 {
                                    enter.recv_timeout(Duration::from_secs(4)).unwrap();
                                }
                                return Err(io::Error::new(
                                    io::ErrorKind::WouldBlock,
                                    "launch refused",
                                ));
                            }
                            Ok(thread::Builder::new())
                        },
                    )?;
                    stop.store(true, Relaxed);
                    receiving.thread().unpark();
                    receiving.join().unwrap();
                    outgoing.join().unwrap();
                    Ok(())
                })
            });
            assert_eq!(attempts, if fail_at == 1 { 1 } else { 2 });
            assert_eq!(stop.load(Relaxed), fail_at != 1);
            assert_eq!(received.load(Relaxed), fail_at != 1);
            assert_eq!(sent.load(Relaxed), fail_at == 0);
            if fail_at == 0 {
                result.unwrap();
                assert!(diagnostic.is_none());
            } else {
                let error = result.unwrap_err();
                assert_eq!(
                    error.downcast_ref::<io::Error>().map(io::Error::kind),
                    Some(io::ErrorKind::WouldBlock)
                );
                assert_eq!(error.to_string(), "launch refused");
                let phase = if fail_at == 1 { "incoming" } else { "connect" };
                assert!(
                    diagnostic
                        .unwrap()
                        .json(settings)
                        .to_string()
                        .contains(&format!(
                            "\"phase\":\"{phase}\",\"category\":\"io\",\"code\":\"would_block\""
                        ))
                );
            }
        }
    });
}

#[test]
fn shutdown_joins_both_service_workers_before_reporting_errors() {
    #[derive(Clone, Copy)]
    enum Receive {
        Complete,
        Error,
        Panic,
    }

    with_interactive_deadline(|| {
        let mut arguments = vec!["--diagnostics=json".into(), "run".into()];
        let settings = diagnostics::settings_from_args(diagnostics::App::Study, &mut arguments);
        let local = ("local_command", "io", "broken_pipe", "local reply failed");
        let receiving = ("incoming", "io", "connection_aborted", "receiver failed");
        let panic = ("shutdown", "engine", "Panic", "Panic");
        for (receive, panic_outgoing, fail_local, expected) in [
            (Receive::Complete, false, false, None),
            (Receive::Complete, false, true, Some(local)),
            (Receive::Complete, true, true, Some(panic)),
            (Receive::Panic, false, false, Some(panic)),
            (Receive::Error, true, true, Some(receiving)),
            (Receive::Panic, true, true, Some(panic)),
        ] {
            let stop = AtomicBool::new(false);
            let receive_finished = AtomicBool::new(false);
            let outgoing_finished = AtomicBool::new(false);
            let wait_for_stop = || {
                let deadline = Instant::now() + Duration::from_secs(4);
                while !stop.load(Relaxed) {
                    assert!(Instant::now() < deadline, "service worker did not stop");
                    thread::park_timeout(Duration::from_millis(25));
                }
            };
            let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
                thread::scope(|scope| {
                    let receiver = scope.spawn(|| {
                        wait_for_stop();
                        receive_finished.store(true, Relaxed);
                        diagnostics::capture(settings, Phase::Incoming, || match receive {
                            Receive::Complete => Ok(()),
                            Receive::Error => Err(io::Error::new(
                                io::ErrorKind::ConnectionAborted,
                                "receiver failed",
                            )
                            .into()),
                            Receive::Panic => panic!("controlled receiver panic"),
                        })
                    });
                    let outgoing = scope.spawn(|| {
                        wait_for_stop();
                        outgoing_finished.store(true, Relaxed);
                        assert!(!panic_outgoing, "controlled outgoing worker panic");
                    });
                    let local = diagnostics::capture(settings, Phase::LocalCommand, || {
                        if fail_local {
                            return Err(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "local reply failed",
                            )
                            .into());
                        }
                        Ok(())
                    });
                    finish_workers(&stop, receiver, outgoing, local)
                })
            });
            assert!(receive_finished.load(Relaxed) && outgoing_finished.load(Relaxed));
            if let Some((phase, category, code, text)) = expected {
                assert_eq!(result.unwrap_err().to_string(), text);
                assert!(
                    diagnostic
                        .unwrap()
                        .json(settings)
                        .to_string()
                        .contains(&format!(
                            "\"phase\":\"{phase}\",\"category\":\"{category}\",\"code\":\"{code}\""
                        ))
                );
            } else {
                result.unwrap();
                assert!(diagnostic.is_none());
            }
        }
    });
}

#[test]
fn interactive_io_errors_stop_workers_and_preserve_commits() {
    with_interactive_deadline(|| {
        let invalid = interactive_case(b"peer\n\xff\n", false, true, false);
        assert_eq!(
            invalid
                .result
                .unwrap_err()
                .downcast_ref::<io::Error>()
                .map(io::Error::kind),
            Some(io::ErrorKind::InvalidData)
        );
        assert!(
            invalid.diagnostic.unwrap().contains(
                "\"phase\":\"local_command\",\"category\":\"io\",\"code\":\"invalid_data\""
            )
        );
        assert_eq!(invalid.replies, "peer 0 idle\n");
        assert_eq!((invalid.revision, invalid.minutes), (2, 0));

        for (input, sync_only, structured, revision, minutes) in [
            (&b"push off\n"[..], false, true, 2, 0),
            (&b"peer\n"[..], false, true, 2, 0),
            (&b"incoming\n"[..], false, true, 2, 0),
            (&b"practice 3\n"[..], true, true, 2, 0),
            (&b"practice 3\n"[..], true, false, 2, 0),
            (&b"practice 3\n"[..], false, true, 3, 3),
            (&b"unknown\n"[..], false, true, 2, 0),
            (&b"unknown\n"[..], false, false, 2, 0),
        ] {
            let observed = interactive_case(input, sync_only, structured, true);
            assert_eq!(
                observed
                    .result
                    .unwrap_err()
                    .downcast_ref::<io::Error>()
                    .map(io::Error::kind),
                Some(io::ErrorKind::BrokenPipe)
            );
            if structured {
                assert!(observed.diagnostic.unwrap().contains(
                    "\"phase\":\"local_command\",\"category\":\"io\",\"code\":\"broken_pipe\""
                ));
            } else {
                assert!(observed.diagnostic.is_none());
            }
            assert!(observed.replies.is_empty());
            assert_eq!((observed.revision, observed.minutes), (revision, minutes));
        }
    });
}

#[test]
fn interactive_modes_report_selected_replies_and_stop_at_quit() {
    with_interactive_deadline(|| {
        let eof = interactive_case(b"", false, true, false);
        eof.result.unwrap();
        assert!(eof.diagnostic.is_none());
        assert!(eof.replies.is_empty());
        assert_eq!((eof.revision, eof.minutes), (2, 0));

        for structured in [false, true] {
            for sync_only in [false, true] {
                let observed = interactive_case(
                    b"\npeer\nincoming\npush off\npractice 3\nunknown\nquit\npractice 99\n",
                    sync_only,
                    structured,
                    false,
                );
                observed.result.unwrap();
                assert!(observed.diagnostic.is_none());
                let lines = observed.replies.lines().collect::<Vec<_>>();
                assert_eq!(lines.len(), 5);
                assert_eq!(lines[0], "peer 0 idle");
                assert_eq!(lines[2], "push off");
                if structured {
                    assert!(
                        lines[1].contains(
                            "\"phase\":\"incoming\",\"served\":0,\"failed\":0,\"busy\":0"
                        )
                    );
                    assert!(lines[4].contains("\"phase\":\"local_command\""));
                } else {
                    assert_eq!(lines[1], "incoming served=0 failed=0 busy=0");
                    assert!(lines[4].starts_with("error unknown command"));
                }
                if sync_only {
                    assert_eq!((observed.revision, observed.minutes), (2, 0));
                    if structured {
                        assert!(lines[3].contains("\"phase\":\"local_command\",\"category\":\"engine\",\"code\":\"Invalid\""));
                    } else {
                        assert_eq!(lines[3], "error local edits are disabled during staging");
                    }
                } else {
                    assert_eq!(lines[3], "local 3");
                    assert_eq!((observed.revision, observed.minutes), (3, 3));
                }
            }
        }
    });
}

struct Worker {
    child: Child,
    input: ChildStdin,
    lines: Receiver<String>,
}

impl Worker {
    fn start(
        dir: &Path,
        actor: &str,
        listen: SocketAddr,
        peer: SocketAddr,
        pause_reply: bool,
        structured: bool,
    ) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native::tests::two_writers_recover_a_lost_ack",
                "--nocapture",
                "--quiet",
            ])
            .env(WORKER, actor)
            .env("STUDY_PEER_TEST_DIR", dir)
            .env("STUDY_PEER_TEST_LISTEN", listen.to_string())
            .env("STUDY_PEER_TEST_PEER", peer.to_string())
            .env(
                "STUDY_PEER_TEST_PAUSE",
                if pause_reply { "yes" } else { "no" },
            )
            .env(
                "STUDY_PEER_TEST_JSON",
                if structured { "yes" } else { "no" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        thread::spawn(move || {
            let mut ready = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let line = if ready {
                    line
                } else if let Some((_, address)) = line.split_once("ready ") {
                    ready = true;
                    format!("ready {address}")
                } else {
                    continue;
                };
                if !line.is_empty() && send.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            lines,
        }
    }

    fn line(&self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(15))
            .expect("worker stopped producing output")
    }

    fn ready(&self) -> SocketAddr {
        self.line()
            .strip_prefix("ready ")
            .expect("worker did not start")
            .parse()
            .unwrap()
    }

    fn command(&mut self, command: &str) -> String {
        writeln!(self.input, "{command}").unwrap();
        self.line()
    }

    fn wait_for(&mut self, command: &str, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let reply = self.command(command);
            if predicate(&reply) {
                return reply;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {command}: {reply}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn kill(&mut self) {
        self.child.kill().unwrap();
        assert!(!self.child.wait().unwrap().success());
    }

    fn quit(&mut self) {
        writeln!(self.input, "quit").unwrap();
        self.input.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "worker failed to stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct PauseReply<S> {
    stream: S,
    enabled: bool,
}
impl<S: Read> Read for PauseReply<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}
impl<S: Write> Write for PauseReply<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.enabled {
            println!("reply-pending");
            // The parent kills us here, after publication and before the reply.
            thread::sleep(Duration::from_secs(30));
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn config(dir: &Path, actor: &str) -> (PathBuf, PathBuf, PathBuf, Enrollment) {
    let peer = if actor == "a" { "b" } else { "a" };
    (
        dir.join(format!("{actor}.db")),
        dir.join(actor),
        dir.join(peer).join("cert.der"),
        Enrollment::new(actor, COLLECTION).unwrap(),
    )
}

fn ack_count(line: &str, total: u64) -> u64 {
    let mut parts = line.split_whitespace();
    assert_eq!(parts.next(), Some("peer"));
    let count = parts.next().unwrap().parse().unwrap();
    if parts.next() == Some("ack") && parts.next().and_then(|n| n.parse().ok()) == Some(total) {
        count
    } else {
        0
    }
}

#[test]
fn two_writers_recover_a_lost_ack() {
    if let Ok(actor) = std::env::var(WORKER) {
        let dir = PathBuf::from(std::env::var_os("STUDY_PEER_TEST_DIR").unwrap());
        let (path, identity, peer_cert, enrollment) = config(&dir, &actor);
        let db = Database::open_file(path, Limits::default()).unwrap();
        let network = Network {
            listener: TcpListener::bind(std::env::var("STUDY_PEER_TEST_LISTEN").unwrap()).unwrap(),
            peer: std::env::var("STUDY_PEER_TEST_PEER")
                .unwrap()
                .parse()
                .unwrap(),
            tls: Tls::new(
                &Credentials::load(&identity).unwrap(),
                fs::read(peer_cert).unwrap().into(),
            )
            .unwrap(),
        };
        let enabled = std::env::var("STUDY_PEER_TEST_PAUSE").unwrap() == "yes";
        let options = RunOptions {
            pushing: false,
            sync_only: false,
        };
        let mut arguments = vec!["run".into()];
        if std::env::var("STUDY_PEER_TEST_JSON").unwrap() == "yes" {
            arguments.insert(0, "--diagnostics=json".into());
        }
        let settings = diagnostics::settings_from_args(diagnostics::App::Study, &mut arguments);
        diagnostics::capture(settings, Phase::Arguments, || {
            run(&db, enrollment, network, options, |stream| PauseReply {
                stream,
                enabled,
            })
        })
        .0
        .unwrap();
        return;
    }

    let dir = Directory::new();
    for actor in ["a", "b"] {
        let (path, identity, _, enrollment) = config(&dir.0, actor);
        Credentials::generate().unwrap().write_fixture(&identity);
        initialize(&path, enrollment).unwrap();
    }
    let dynamic = "127.0.0.1:0".parse().unwrap();
    let reserved = TcpListener::bind(dynamic).unwrap();
    let b_address = reserved.local_addr().unwrap();
    let mut a = Worker::start(&dir.0, "a", dynamic, b_address, false, false);
    let a_address = a.ready();
    drop(reserved);
    let mut b = Worker::start(&dir.0, "b", b_address, a_address, true, false);
    assert_eq!(b.ready(), b_address);

    assert_eq!(a.command("edit 1 20 60 20"), "local 3");
    assert_eq!(b.command("edit 2 15 90 30"), "local 3");
    assert_eq!(a.command("show"), "state 3 20 [60] [20] [1]");
    assert_eq!(b.command("show"), "state 3 15 [90] [30] [2]");
    assert_eq!(a.command("push on"), "push on");
    assert_eq!(b.line(), "reply-pending");
    assert_eq!(b.command("show"), "state 4 35 [90] [20] [1, 2]");
    assert_eq!(b.command("practice 5"), "local 5");
    b.kill();

    let failure = a.wait_for("peer", |line| line.starts_with("peer 0 error "));
    assert_eq!(failure, "peer 0 error Io");
    let mut b = Worker::start(&dir.0, "b", b_address, a_address, false, false);
    assert_eq!(b.ready(), b_address);
    a.wait_for("peer", |line| ack_count(line, 40) > 0);
    assert_eq!(a.command("show"), "state 3 20 [60] [20] [1]");
    assert_eq!(b.command("show"), "state 5 40 [90] [20] [1, 2]");

    assert_eq!(b.command("push on"), "push on");
    let first_b = b.wait_for("peer", |line| ack_count(line, 40) > 0);
    assert_eq!(a.command("show"), "state 4 40 [90] [20] [1, 2]");
    let first_a = a.command("peer");
    a.wait_for("peer", |line| ack_count(line, 40) > ack_count(&first_a, 40));
    b.wait_for("peer", |line| ack_count(line, 40) > ack_count(&first_b, 40));
    assert_eq!(a.command("show"), "state 4 40 [90] [20] [1, 2]");
    assert_eq!(b.command("show"), "state 5 40 [90] [20] [1, 2]");
}

#[test]
fn paused_peer_reports_idle_errors_and_quits_cleanly() {
    let dir = Directory::new();
    let (path, identity, _, enrollment) = config(&dir.0, "a");
    Credentials::generate().unwrap().write_fixture(&identity);
    let (_, other, _, _) = config(&dir.0, "b");
    Credentials::generate().unwrap().write_fixture(&other);
    initialize(&path, enrollment).unwrap();
    let dynamic = "127.0.0.1:0".parse().unwrap();
    let mut worker = Worker::start(&dir.0, "a", dynamic, dynamic, false, false);
    worker.ready();
    assert_eq!(worker.command("peer"), "peer 0 idle");
    assert!(worker.command("unknown").starts_with("error "));
    assert_eq!(worker.command("push off"), "push off");
    writeln!(worker.input).unwrap();
    worker.quit();
}

#[test]
fn peer_reports_connect_failure_then_acknowledges_retry() {
    for structured in [false, true] {
        let dir = Directory::new();
        for actor in ["a", "b"] {
            let (path, identity, _, enrollment) = config(&dir.0, actor);
            Credentials::generate().unwrap().write_fixture(&identity);
            initialize(&path, enrollment).unwrap();
        }
        let (follower, identity, pin, enrollment) = config(&dir.0, "b");
        let server = Tls::new(
            &Credentials::load(&identity).unwrap(),
            fs::read(pin).unwrap().into(),
        )
        .unwrap();
        let before = fs::read(&follower).unwrap();
        let dynamic = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(dynamic).unwrap();
        listener.set_nonblocking(true).unwrap();
        let b_address = listener.local_addr().unwrap();
        let accept = || {
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                match listener.accept() {
                    Ok((connection, _)) => return connection,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "outgoing worker did not connect");
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("test listener failed: {error}"),
                }
            }
        };
        let mut a = Worker::start(&dir.0, "a", dynamic, b_address, false, structured);
        a.ready();
        assert_eq!(a.command("practice 7"), "local 3");
        assert_eq!(a.command("peer"), "peer 0 idle");
        assert_eq!(a.command("push on"), "push on");

        // Keep the socket open while the send half ends. The client sees EOF
        // before the handshake finishes, even if it has already sent bytes.
        let refused = accept();
        refused.shutdown(std::net::Shutdown::Write).unwrap();
        let failure = a.wait_for("peer", |line| line != "peer 0 idle");
        if structured {
            assert!(
                failure.contains(
                    "\"phase\":\"connect\",\"category\":\"io\",\"code\":\"unexpected_eof\""
                ),
                "unexpected structured connection result: {failure}"
            );
        } else {
            let eof = io::Error::from(io::ErrorKind::UnexpectedEof);
            assert_eq!(failure, format!("peer 0 error {eof}"));
        }
        drop(refused);
        assert_eq!(fs::read(&follower).unwrap(), before);

        {
            let db = Database::open_file(&follower, Limits::default()).unwrap();
            let replica = StudyReplica::open(
                &db,
                db.collection_id(COLLECTION_ROW),
                enrollment.policy,
                enrollment.actor,
            )
            .unwrap();
            let serve = || {
                let channel = server.accept(accept()).unwrap();
                replica.serve_authenticated(channel).unwrap();
            };
            assert_eq!(db.snapshot().revision(), 2);
            serve();
            let ack = a.wait_for("peer", |line| line.contains(" ack "));
            let first = ack_count(&ack, 7);
            assert!(
                first > 0,
                "successful retry did not advance acknowledgments"
            );
            assert_eq!(db.snapshot().revision(), 3);
            assert_eq!(replica.read().unwrap().practice_minutes.total(), 7);
            serve();
            a.wait_for("peer", |line| {
                line.contains(" ack ") && ack_count(line, 7) > first
            });
            assert_eq!(db.snapshot().revision(), 3);
        }
        assert_eq!(a.command("push off"), "push off");
        drop(listener);
        a.quit();

        let db = Database::open_file(&follower, Limits::default()).unwrap();
        assert_eq!(db.snapshot().revision(), 3);
        let replica = StudyReplica::open(
            &db,
            db.collection_id(COLLECTION_ROW),
            enrollment.policy,
            enrollment.actor,
        )
        .unwrap();
        assert_eq!(replica.read().unwrap().practice_minutes.total(), 7);
    }
}
