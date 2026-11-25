//! Opt-in CLI diagnostics: closed fields, no error text or source-chain traversal.
use std::{cell::Cell, error::Error as StdError, ffi::OsString, fmt, io};
use weekend_db::{
    Error,
    storage::{
        self, NativeIoCategory, NativeIoDiagnostic, NativeIoOperation, VerificationDiagnostic,
        VerificationStage,
    },
};

type AppResult<T> = Result<T, Box<dyn StdError + Send + Sync>>;
macro_rules! names {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[allow(dead_code)] // The same closed schema is compiled into both CLI binaries.
        pub(crate) enum $name { $($variant),+ }
        impl $name { fn text(self) -> &'static str { match self { $(Self::$variant => $text),+ } } }
    }
}
names!(App { Snapshot => "weekend-replicate", Study => "study-peer" });
names!(Command {
    Unknown => "unknown", Help => "help", Keygen => "keygen", Describe => "describe",
    FollowerInit => "follower-init", Status => "status", Cleanup => "cleanup-inactive",
    Compact => "compact-manifest", Serve => "serve", Ship => "ship", Capture => "capture",
    ShipCaptured => "ship-captured", Poll => "poll", Init => "init", Run => "run",
    RunPolicy => "run-policy", CheckpointRoster => "checkpoint-roster", StageRoster => "stage-roster",
    VerifyRoster => "verify-roster"
});
impl Command {
    fn parse(value: Option<&OsString>) -> Self {
        match value.and_then(|s| s.to_str()) {
            Some("--help" | "-h") => Self::Help,
            Some("keygen") => Self::Keygen,
            Some("describe") => Self::Describe,
            Some("follower-init") => Self::FollowerInit,
            Some("status") => Self::Status,
            Some("cleanup-inactive") => Self::Cleanup,
            Some("compact-manifest") => Self::Compact,
            Some("serve") => Self::Serve,
            Some("ship") => Self::Ship,
            Some("capture") => Self::Capture,
            Some("ship-captured") => Self::ShipCaptured,
            Some("poll") => Self::Poll,
            Some("init") => Self::Init,
            Some("run") => Self::Run,
            Some("run-policy") => Self::RunPolicy,
            Some("checkpoint-roster") => Self::CheckpointRoster,
            Some("stage-roster") => Self::StageRoster,
            Some("verify-roster") => Self::VerifyRoster,
            _ => Self::Unknown,
        }
    }
}
names!(Phase {
    Arguments => "arguments", CredentialsLoad => "credentials_load", ConfigurationLoad => "configuration_load", TlsConfig => "tls_config",
    StorageOpen => "storage_open", StorageCreate => "storage_create", StorageVerify => "storage_verify",
    Maintenance => "storage_maintenance", Export => "export", Listen => "listen", Connect => "connect",
    TlsHandshake => "tls_handshake", Replication => "replication", LocalCommand => "local_command",
    Incoming => "incoming", Shutdown => "shutdown"
});
names!(Category { Engine => "engine", Io => "io", Tls => "tls", Input => "input", Internal => "internal" });
names!(TlsCode { InvalidCertificate => "invalid_certificate", InvalidMessage => "invalid_message", Other => "other" });
#[derive(Clone, Copy)]
enum Code {
    Engine(Error),
    Io(io::ErrorKind),
    Tls(TlsCode),
    InvalidInput,
    Unclassified,
}
impl Code {
    fn text(self) -> &'static str {
        match self {
            Self::Engine(error) => match error {
                Error::Invalid => "Invalid",
                Error::Type => "Type",
                Error::NotFound => "NotFound",
                Error::Bounds => "Bounds",
                Error::Buffer => "Buffer",
                Error::Busy => "Busy",
                Error::Limit => "Limit",
                Error::OutOfMemory => "OutOfMemory",
                Error::Panic => "Panic",
                Error::Conflict => "Conflict",
                Error::Closed => "Closed",
                Error::ForeignStore => "ForeignStore",
                Error::Exhausted => "Exhausted",
                Error::Io => "Io",
                Error::OutcomeUnknown => "OutcomeUnknown",
                Error::RecoveryRequired => "RecoveryRequired",
                Error::Corrupt => "Corrupt",
                Error::UnsupportedFormat => "UnsupportedFormat",
                Error::AlreadyExists => "AlreadyExists",
                Error::UnsupportedPlatform => "UnsupportedPlatform",
            },
            Self::Io(kind) => match kind {
                io::ErrorKind::NotFound => "not_found",
                io::ErrorKind::PermissionDenied => "permission_denied",
                io::ErrorKind::ConnectionRefused => "connection_refused",
                io::ErrorKind::ConnectionReset => "connection_reset",
                io::ErrorKind::ConnectionAborted => "connection_aborted",
                io::ErrorKind::TimedOut => "timed_out",
                io::ErrorKind::WouldBlock => "would_block",
                io::ErrorKind::Interrupted => "interrupted",
                io::ErrorKind::InvalidData => "invalid_data",
                io::ErrorKind::InvalidInput => "invalid_input",
                io::ErrorKind::UnexpectedEof => "unexpected_eof",
                io::ErrorKind::BrokenPipe => "broken_pipe",
                io::ErrorKind::AlreadyExists => "already_exists",
                _ => "other",
            },
            Self::Tls(code) => code.text(),
            Self::InvalidInput => "invalid_input",
            Self::Unclassified => "unclassified",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Settings {
    pub(crate) enabled: bool,
    app: App,
    command: Command,
}
#[derive(Clone, Copy)]
pub(crate) struct Diagnostic {
    phase: Phase,
    category: Category,
    code: Code,
    native: Option<NativeIoDiagnostic>,
    os_code: Option<i32>,
    verification: Option<VerificationDiagnostic>,
}
fn classify(phase: Phase, error: &(dyn StdError + 'static)) -> Diagnostic {
    let mut diagnostic = Diagnostic {
        phase,
        category: Category::Internal,
        code: Code::Unclassified,
        native: None,
        os_code: None,
        verification: None,
    };
    if let Some(error) = error.downcast_ref::<Error>() {
        diagnostic.category = Category::Engine;
        diagnostic.code = Code::Engine(*error);
    } else if let Some(error) = error.downcast_ref::<io::Error>() {
        diagnostic.category = Category::Io;
        diagnostic.code = Code::Io(error.kind());
        diagnostic.os_code = error.raw_os_error();
        // Only inspect this known concrete payload. Never traverse arbitrary source chains.
        if let Some(tls) = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        {
            diagnostic.category = Category::Tls;
            diagnostic.code = Code::Tls(tls_code(tls));
        }
    } else if let Some(error) = error.downcast_ref::<rustls::Error>() {
        diagnostic.category = Category::Tls;
        diagnostic.code = Code::Tls(tls_code(error));
    } else if error.is::<std::num::ParseIntError>() || error.is::<std::net::AddrParseError>() {
        diagnostic.category = Category::Input;
        diagnostic.code = Code::InvalidInput;
    }
    diagnostic
}
fn tls_code(error: &rustls::Error) -> TlsCode {
    match error {
        rustls::Error::InvalidCertificate(_) => TlsCode::InvalidCertificate,
        rustls::Error::InvalidMessage(_) => TlsCode::InvalidMessage,
        _ => TlsCode::Other,
    }
}
#[derive(Clone, Copy)]
struct Context {
    settings: Settings,
    last: Option<Diagnostic>,
}
thread_local! { static CONTEXT: Cell<Context> = const { Cell::new(Context {
    settings: Settings { enabled: false, app: App::Snapshot, command: Command::Unknown }, last: None
}) }; }
struct Restore(Context);
impl Drop for Restore {
    fn drop(&mut self) {
        CONTEXT.with(|slot| slot.set(self.0));
    }
}
pub(crate) fn settings() -> Settings {
    CONTEXT.with(|slot| slot.get().settings)
}
fn set_last(last: Option<Diagnostic>) {
    CONTEXT.with(|slot| {
        let mut context = slot.get();
        context.last = last;
        slot.set(context);
    });
}
pub(crate) fn take(phase: Phase, error: &(dyn StdError + 'static)) -> Diagnostic {
    let last = CONTEXT.with(|slot| slot.get().last);
    set_last(None);
    last.unwrap_or_else(|| classify(phase, error))
}
#[allow(dead_code)] // Study preserves an operator failure while joining its workers.
pub(crate) fn propagate<T>(result: AppResult<T>, diagnostic: Option<Diagnostic>) -> AppResult<T> {
    if settings().enabled {
        set_last(if result.is_err() { diagnostic } else { None });
    }
    result
}
pub(crate) fn at<T>(phase: Phase, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    if !settings().enabled {
        return operation();
    }
    set_last(None);
    let result = operation();
    match &result {
        Ok(_) => set_last(None),
        Err(error) => {
            let diagnostic = CONTEXT
                .with(|slot| slot.get().last)
                .unwrap_or_else(|| classify(phase, error.as_ref()));
            set_last(Some(diagnostic));
        }
    }
    result
}
pub(crate) fn storage<T>(
    phase: Phase,
    operation: impl FnOnce() -> weekend_db::Result<T>,
) -> weekend_db::Result<T> {
    if !settings().enabled {
        return operation();
    }
    set_last(None);
    match storage::with_storage_diagnostics(operation) {
        Ok(value) => Ok(value),
        Err(failure) => {
            let mut diagnostic = classify(phase, &failure.error);
            diagnostic.native = failure.native;
            diagnostic.verification = failure.verification;
            set_last(Some(diagnostic));
            Err(failure.error)
        }
    }
}
pub(crate) fn capture<T>(
    settings: Settings,
    phase: Phase,
    operation: impl FnOnce() -> AppResult<T>,
) -> (AppResult<T>, Option<Diagnostic>) {
    let _restore = Restore(CONTEXT.with(|slot| {
        slot.replace(Context {
            settings,
            last: None,
        })
    }));
    let result = at(phase, operation);
    let diagnostic = if result.is_err() && settings.enabled {
        CONTEXT.with(|slot| slot.get().last)
    } else {
        None
    };
    (result, diagnostic)
}
pub(crate) fn settings_from_args(app: App, args: &mut Vec<OsString>) -> Settings {
    let enabled = args.first().is_some_and(|arg| arg == "--diagnostics=json");
    if enabled {
        args.remove(0);
    }
    Settings {
        enabled,
        app,
        command: Command::parse(args.first()),
    }
}
pub(crate) fn entry(
    app: App,
    operation: impl FnOnce(Vec<OsString>) -> AppResult<()>,
) -> std::process::ExitCode {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let settings = settings_from_args(app, &mut args);
    let (result, diagnostic) = capture(settings, Phase::Arguments, || operation(args));
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if settings.enabled {
                eprintln!(
                    "{}",
                    diagnostic
                        .unwrap_or_else(|| take(Phase::Arguments, error.as_ref()))
                        .json(settings)
                );
            } else {
                eprintln!("{error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}

/// Only fixed ASCII strings and bounded numeric values enter this formatter.
pub(crate) struct JsonLine {
    bytes: [u8; 1024],
    len: usize,
}
impl JsonLine {
    fn new() -> Self {
        Self {
            bytes: [0; 1024],
            len: 0,
        }
    }
    fn prefix(&mut self, settings: Settings) -> fmt::Result {
        use fmt::Write;
        write!(
            self,
            "{{\"schema\":\"weekend-diagnostic/1\",\"app\":\"{}\",\"command\":\"{}\",",
            settings.app.text(),
            settings.command.text()
        )
    }
}
impl fmt::Write for JsonLine {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .len
            .checked_add(text.len())
            .filter(|end| *end < self.bytes.len()) // Reserve one byte for the line terminator.
            .ok_or(fmt::Error)?;
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
impl fmt::Display for JsonLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(std::str::from_utf8(&self.bytes[..self.len]).map_err(|_| fmt::Error)?)
    }
}
fn operation_name(operation: NativeIoOperation) -> &'static str {
    match operation {
        NativeIoOperation::Open => "open",
        NativeIoOperation::Create => "create",
        NativeIoOperation::OpenDirectory => "open_directory",
        NativeIoOperation::DuplicateHandle => "duplicate_handle",
        NativeIoOperation::Metadata => "metadata",
        NativeIoOperation::Read => "read",
        NativeIoOperation::Write => "write",
        NativeIoOperation::Sync => "sync",
        NativeIoOperation::Lock => "lock",
        NativeIoOperation::Unlock => "unlock",
        NativeIoOperation::Exchange => "exchange",
        NativeIoOperation::Remove => "remove",
        NativeIoOperation::CopyMetadata => "copy_metadata",
        NativeIoOperation::Random => "random",
        NativeIoOperation::Volume => "volume",
    }
}
fn stage_name(stage: VerificationStage) -> &'static str {
    match stage {
        VerificationStage::ImageLength => "image_length",
        VerificationStage::Header => "header",
        VerificationStage::RootSlot => "root_slot",
        VerificationStage::Witness => "witness",
        VerificationStage::RootSelection => "root_selection",
        VerificationStage::CommitDescriptor => "commit_descriptor",
        VerificationStage::Directory => "directory",
        VerificationStage::Value => "value",
    }
}
fn optional_number<T: fmt::Display>(out: &mut JsonLine, value: Option<T>) -> fmt::Result {
    use fmt::Write;
    match value {
        Some(value) => write!(out, "{value}"),
        None => out.write_str("null"),
    }
}
impl Diagnostic {
    fn fields(self, out: &mut JsonLine) -> fmt::Result {
        use fmt::Write;
        write!(
            out,
            "\"phase\":\"{}\",\"category\":\"{}\",\"code\":\"{}\",\"native\":",
            self.phase.text(),
            self.category.text(),
            self.code.text()
        )?;
        if let Some(native) = self.native {
            let category = match native.category {
                NativeIoCategory::Permission => "permission",
                NativeIoCategory::DiskFull => "disk_full",
                NativeIoCategory::Quota => "quota",
                NativeIoCategory::Other => "other",
            };
            write!(
                out,
                "{{\"operation\":\"{}\",\"category\":\"{category}\",\"os_code\":",
                operation_name(native.operation)
            )?;
            optional_number(out, native.raw_os_error)?;
            out.write_str("}")?;
        } else if let Some(code) = self.os_code {
            write!(
                out,
                "{{\"operation\":null,\"category\":null,\"os_code\":{code}}}"
            )?;
        } else {
            out.write_str("null")?;
        }
        out.write_str(",\"verification\":")?;
        if let Some(verification) = self.verification {
            write!(
                out,
                "{{\"stage\":\"{}\",\"offset\":",
                stage_name(verification.stage)
            )?;
            optional_number(out, verification.offset)?;
            out.write_str("}")
        } else {
            out.write_str("null")
        }
    }
    pub(crate) fn json(self, settings: Settings) -> JsonLine {
        use fmt::Write;
        let mut out = JsonLine::new();
        out.prefix(settings).expect("bounded schema");
        if matches!(settings.command, Command::Ship | Command::ShipCaptured) {
            out.write_str("\"follower_outcome\":\"unconfirmed\",\"guidance\":\"poll_or_reconcile_before_retry\",").expect("bounded schema");
        }
        self.fields(&mut out).expect("bounded schema");
        out.write_str("}").expect("bounded schema");
        out
    }
}
#[allow(dead_code)] // The snapshot CLI has no live diagnostic query interface.
pub(crate) fn incoming(
    settings: Settings,
    served: u64,
    failed: u64,
    busy: u64,
    last: Option<Diagnostic>,
) -> JsonLine {
    use fmt::Write;
    let mut out = JsonLine::new();
    out.prefix(settings).expect("bounded schema");
    write!(
        &mut out,
        "\"phase\":\"{}\",\"served\":{served},\"failed\":{failed},\"busy\":{busy},\"last\":",
        Phase::Incoming.text()
    )
    .expect("bounded schema");
    if let Some(last) = last {
        out.write_str("{").expect("bounded schema");
        last.fields(&mut out).expect("bounded schema");
        out.write_str("}").expect("bounded schema");
    } else {
        out.write_str("null").expect("bounded schema");
    }
    out.write_str("}").expect("bounded schema");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn enabled() -> Settings {
        Settings {
            enabled: true,
            app: App::Snapshot,
            command: Command::Describe,
        }
    }

    #[test]
    fn opt_in_is_leading_only_and_never_serializes_argument_text() {
        let mut args = vec![
            "--diagnostics=json".into(),
            "ship".into(),
            "PRIVATE_CANARY".into(),
        ];
        let config = settings_from_args(App::Snapshot, &mut args);
        assert!(config.enabled);
        assert_eq!(
            args,
            [OsString::from("ship"), OsString::from("PRIVATE_CANARY")]
        );
        let json = classify(Phase::Arguments, &Error::Invalid)
            .json(config)
            .to_string();
        assert!(!json.contains("PRIVATE_CANARY"));
        assert!(json.contains("\"follower_outcome\":\"unconfirmed\""));
        let mut args = vec!["ship".into(), "--diagnostics=json".into()];
        assert!(!settings_from_args(App::Snapshot, &mut args).enabled);
        assert_eq!(args.len(), 2);
    }

    struct Poison;
    impl fmt::Display for Poison {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            panic!("must not format unknown error");
        }
    }
    impl fmt::Debug for Poison {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            panic!("must not debug unknown error");
        }
    }
    impl StdError for Poison {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            panic!("must not traverse unknown error");
        }
    }

    #[test]
    fn unknown_errors_never_format_or_traverse_arbitrary_sources() {
        let (result, diagnostic) = capture(enabled(), Phase::TlsConfig, || {
            Err::<(), _>(Box::new(Poison))
        });
        assert!(result.is_err());
        assert_eq!(
            diagnostic.unwrap().json(enabled()).to_string(),
            "{\"schema\":\"weekend-diagnostic/1\",\"app\":\"weekend-replicate\",\"command\":\"describe\",\"phase\":\"tls_config\",\"category\":\"internal\",\"code\":\"unclassified\",\"native\":null,\"verification\":null}"
        );
        let wrapped = io::Error::other(Poison);
        let diagnostic = classify(Phase::Connect, &wrapped);
        assert!(
            diagnostic
                .json(enabled())
                .to_string()
                .contains("\"category\":\"io\",\"code\":\"other\"")
        );
    }

    #[test]
    fn storage_capture_preserves_original_error_and_clears_between_attempts() {
        let missing = std::env::temp_dir().join(format!(
            "weekend-diagnostic-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (first, diagnostic) = capture(enabled(), Phase::Arguments, || {
            Ok(storage(Phase::StorageOpen, || {
                weekend_db::Database::open_file(&missing, weekend_db::Limits::default())
            })?
            .snapshot()
            .revision())
        });
        assert_eq!(
            first.as_ref().err().unwrap().downcast_ref::<Error>(),
            Some(&Error::NotFound)
        );
        assert!(diagnostic.unwrap().native.is_some());
        let (result, diagnostic) = capture(enabled(), Phase::Arguments, || {
            Ok(storage(Phase::Replication, || {
                Err::<(), _>(Error::ForeignStore)
            })?)
        });
        assert_eq!(
            result.as_ref().err().unwrap().downcast_ref::<Error>(),
            Some(&Error::ForeignStore)
        );
        assert!(diagnostic.unwrap().native.is_none());
        assert!(diagnostic.unwrap().verification.is_none());
        let (_, diagnostic) = capture(enabled(), Phase::Arguments, || {
            let failed = at(Phase::Connect, || {
                Err::<(), _>(io::Error::from_raw_os_error(2).into())
            });
            assert!(failed.is_err());
            at(Phase::Connect, || Ok(()))?;
            Err::<(), _>(Error::Limit.into())
        });
        let diagnostic = diagnostic.unwrap();
        assert_eq!(diagnostic.phase, Phase::Arguments);
        assert_eq!(diagnostic.code.text(), "Limit");
        assert!(diagnostic.os_code.is_none());
    }

    #[test]
    fn nested_captures_and_threads_restore_their_own_context() {
        let (_, diagnostic) = capture(enabled(), Phase::Arguments, || {
            let failed = at(Phase::StorageOpen, || Err::<(), _>(Error::Corrupt.into()));
            let (inner, _) = capture(enabled(), Phase::TlsHandshake, || {
                Err::<(), _>(io::Error::from(io::ErrorKind::TimedOut).into())
            });
            assert!(inner.is_err());
            std::thread::spawn(|| {
                assert!(!settings().enabled);
                let (_, diagnostic) = capture(enabled(), Phase::Connect, || {
                    Err::<(), _>(Error::Busy.into())
                });
                assert_eq!(diagnostic.unwrap().code.text(), "Busy");
                assert!(!settings().enabled);
            })
            .join()
            .unwrap();
            failed
        });
        assert_eq!(diagnostic.unwrap().phase, Phase::StorageOpen);
        assert_eq!(diagnostic.unwrap().code.text(), "Corrupt");
        assert!(!settings().enabled);
        let panic = std::panic::catch_unwind(|| {
            capture(enabled(), Phase::Connect, || -> AppResult<()> {
                panic!("synthetic diagnostic-scope unwind");
            })
        });
        assert!(panic.is_err());
        assert!(!settings().enabled);
    }

    #[test]
    fn renderer_is_valid_for_maximal_numeric_fields_and_unknown_commands() {
        let mut config = enabled();
        config.app = App::Study;
        config.command = Command::parse(Some(&OsString::from("SECRET\"\\\nCANARY")));
        let diagnostic = Diagnostic {
            phase: Phase::Replication,
            category: Category::Engine,
            code: Code::Engine(Error::RecoveryRequired),
            native: Some(NativeIoDiagnostic {
                operation: NativeIoOperation::DuplicateHandle,
                category: NativeIoCategory::Permission,
                raw_os_error: Some(i32::MIN),
            }),
            os_code: None,
            verification: Some(VerificationDiagnostic {
                stage: VerificationStage::CommitDescriptor,
                offset: Some(u64::MAX),
            }),
        };
        let json = incoming(config, u64::MAX, u64::MAX, u64::MAX, Some(diagnostic)).to_string();
        assert!(json.len() < 1024 && !json.contains("SECRET") && !json.contains('\n'));
        assert!(json.contains("\"command\":\"unknown\""));
        assert!(json.contains("\"os_code\":-2147483648"));
        assert!(json.contains("\"offset\":18446744073709551615"));
        assert!(json.ends_with("}}}"));
    }

    #[test]
    fn diagnostic_fields_reserve_the_terminator_and_propagate_full_buffers() {
        use fmt::Write;
        let base = classify(Phase::StorageVerify, &Error::Corrupt);
        let native = NativeIoDiagnostic {
            operation: NativeIoOperation::Read,
            category: NativeIoCategory::DiskFull,
            raw_os_error: Some(-28),
        };
        let verification = VerificationDiagnostic {
            stage: VerificationStage::Directory,
            offset: None,
        };
        for (detail, native_json, verification_json) in [
            (base, "null", "null"),
            (
                Diagnostic {
                    os_code: Some(2),
                    ..base
                },
                r#"{"operation":null,"category":null,"os_code":2}"#,
                "null",
            ),
            (
                Diagnostic {
                    native: Some(native),
                    verification: Some(verification),
                    ..base
                },
                r#"{"operation":"read","category":"disk_full","os_code":-28}"#,
                r#"{"stage":"directory","offset":null}"#,
            ),
            (
                Diagnostic {
                    native: Some(NativeIoDiagnostic {
                        category: NativeIoCategory::Quota,
                        raw_os_error: None,
                        ..native
                    }),
                    verification: Some(VerificationDiagnostic {
                        offset: Some(u64::MAX),
                        ..verification
                    }),
                    ..base
                },
                r#"{"operation":"read","category":"quota","os_code":null}"#,
                r#"{"stage":"directory","offset":18446744073709551615}"#,
            ),
        ] {
            let expected = format!(
                r#""phase":"storage_verify","category":"engine","code":"Corrupt","native":{native_json},"verification":{verification_json}"#
            );
            for remaining in 0..=expected.len() {
                let mut line = JsonLine::new();
                let start = line.bytes.len() - 1 - remaining;
                line.write_str(&".".repeat(start)).unwrap();
                assert_eq!(
                    detail.fields(&mut line).is_ok(),
                    remaining == expected.len()
                );
                assert!(line.len < line.bytes.len());
                assert!(line.bytes[..start].iter().all(|&byte| byte == b'.'));
                assert!(
                    expected
                        .as_bytes()
                        .starts_with(&line.bytes[start..line.len])
                );
                if remaining == expected.len() {
                    assert_eq!(&line.bytes[start..line.len], expected.as_bytes());
                    assert!(line.write_str("x").is_err());
                }
                assert_eq!(line.bytes[line.len..], vec![0; line.bytes.len() - line.len]);
            }
        }
    }

    #[test]
    fn capture_propagates_only_the_current_failed_operation() {
        for active in [false, true] {
            let config = Settings {
                enabled: active,
                ..enabled()
            };
            let detail = classify(Phase::Shutdown, &Error::Busy);
            for fails in [false, true] {
                let (result, captured) = capture(config, Phase::Incoming, || {
                    let result = if fails {
                        Err(Error::Limit.into())
                    } else {
                        Ok(7)
                    };
                    propagate(result, Some(detail))
                });
                if fails {
                    assert_eq!(
                        result.unwrap_err().downcast_ref::<Error>(),
                        Some(&Error::Limit)
                    );
                } else {
                    assert_eq!(result.unwrap(), 7);
                }
                assert_eq!(
                    captured.map(|d| (d.phase, d.code.text())),
                    (active && fails).then_some((Phase::Shutdown, "Busy"))
                );
            }
        }
        let (result, _) = capture(enabled(), Phase::Incoming, || {
            let failure = classify(Phase::Shutdown, &Error::Busy);
            for succeeds in [false, true] {
                set_last(Some(failure));
                let result = if succeeds {
                    Ok(())
                } else {
                    Err(Error::Limit.into())
                };
                let _ = propagate(result, succeeds.then_some(failure));
                assert_eq!(
                    take(Phase::Incoming, &Error::Corrupt).code.text(),
                    "Corrupt"
                );
            }
            Ok(())
        });
        result.unwrap();
        let json = incoming(enabled(), 0, u64::MAX, 1, None).to_string();
        assert!(
            json.ends_with(r#""served":0,"failed":18446744073709551615,"busy":1,"last":null}"#)
        );
    }

    #[test]
    fn classifier_distinguishes_parse_errors_and_preserves_known_tls_codes() {
        for (error, category, code) in [
            (
                &"bad".parse::<u64>().unwrap_err() as &dyn StdError,
                Category::Input,
                "invalid_input",
            ),
            (
                &"bad".parse::<std::net::SocketAddr>().unwrap_err(),
                Category::Input,
                "invalid_input",
            ),
            (&Poison, Category::Internal, "unclassified"),
            (&Error::OutcomeUnknown, Category::Engine, "OutcomeUnknown"),
            (
                &io::Error::from(io::ErrorKind::Interrupted),
                Category::Io,
                "interrupted",
            ),
        ] {
            let diagnostic = classify(Phase::Connect, error);
            assert_eq!(
                (diagnostic.category, diagnostic.code.text()),
                (category, code)
            );
        }
        for (error, expected) in [
            (
                rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding),
                "invalid_certificate",
            ),
            (
                rustls::Error::InvalidMessage(rustls::InvalidMessage::InvalidContentType),
                "invalid_message",
            ),
            (rustls::Error::General("PRIVATE_CANARY".into()), "other"),
        ] {
            let direct = classify(Phase::TlsHandshake, &error);
            let wrapped = classify(Phase::TlsHandshake, &io::Error::other(error));
            for diagnostic in [direct, wrapped] {
                assert_eq!(
                    (diagnostic.category, diagnostic.code.text()),
                    (Category::Tls, expected)
                );
                assert!(
                    !diagnostic
                        .json(enabled())
                        .to_string()
                        .contains("PRIVATE_CANARY")
                );
            }
        }
    }

    #[test]
    fn stable_diagnostic_names_for_each_storage_code_and_command() {
        use NativeIoOperation as Op;
        use VerificationStage as Stage;
        for (op, label) in [
            (Op::Open, "open"),
            (Op::Create, "create"),
            (Op::OpenDirectory, "open_directory"),
            (Op::DuplicateHandle, "duplicate_handle"),
            (Op::Metadata, "metadata"),
            (Op::Read, "read"),
            (Op::Write, "write"),
            (Op::Sync, "sync"),
            (Op::Lock, "lock"),
            (Op::Unlock, "unlock"),
            (Op::Exchange, "exchange"),
            (Op::Remove, "remove"),
            (Op::CopyMetadata, "copy_metadata"),
            (Op::Random, "random"),
            (Op::Volume, "volume"),
        ] {
            assert_eq!(operation_name(op), label);
        }
        for (stage, label) in [
            (Stage::ImageLength, "image_length"),
            (Stage::Header, "header"),
            (Stage::RootSlot, "root_slot"),
            (Stage::Witness, "witness"),
            (Stage::RootSelection, "root_selection"),
            (Stage::CommitDescriptor, "commit_descriptor"),
            (Stage::Directory, "directory"),
            (Stage::Value, "value"),
        ] {
            assert_eq!(stage_name(stage), label);
        }
        for (category, label) in [
            (NativeIoCategory::DiskFull, "disk_full"),
            (NativeIoCategory::Permission, "permission"),
            (NativeIoCategory::Quota, "quota"),
            (NativeIoCategory::Other, "other"),
        ] {
            let mut diagnostic = classify(Phase::StorageOpen, &Error::Io);
            diagnostic.native = Some(NativeIoDiagnostic {
                operation: Op::Open,
                category,
                raw_os_error: None,
            });
            assert!(diagnostic.json(enabled()).to_string().contains(&format!(
                r#""operation":"open","category":"{label}","os_code":null"#
            )));
        }
        for code in [
            Error::Invalid,
            Error::Type,
            Error::NotFound,
            Error::Bounds,
            Error::Buffer,
            Error::Busy,
            Error::Limit,
            Error::OutOfMemory,
            Error::Panic,
            Error::Conflict,
            Error::Closed,
            Error::ForeignStore,
            Error::Exhausted,
            Error::Io,
            Error::OutcomeUnknown,
            Error::RecoveryRequired,
            Error::Corrupt,
            Error::UnsupportedFormat,
            Error::AlreadyExists,
            Error::UnsupportedPlatform,
        ] {
            assert_eq!(Code::Engine(code).text(), code.to_string());
        }
        use io::ErrorKind as Kind;
        for (kind, label) in [
            (Kind::NotFound, "not_found"),
            (Kind::PermissionDenied, "permission_denied"),
            (Kind::ConnectionRefused, "connection_refused"),
            (Kind::ConnectionReset, "connection_reset"),
            (Kind::ConnectionAborted, "connection_aborted"),
            (Kind::TimedOut, "timed_out"),
            (Kind::WouldBlock, "would_block"),
            (Kind::Interrupted, "interrupted"),
            (Kind::InvalidData, "invalid_data"),
            (Kind::InvalidInput, "invalid_input"),
            (Kind::UnexpectedEof, "unexpected_eof"),
            (Kind::BrokenPipe, "broken_pipe"),
            (Kind::AlreadyExists, "already_exists"),
            (Kind::Other, "other"),
        ] {
            assert_eq!(Code::Io(kind).text(), label);
        }
        for word in [
            "--help",
            "-h",
            "keygen",
            "describe",
            "follower-init",
            "status",
            "cleanup-inactive",
            "compact-manifest",
            "serve",
            "ship",
            "capture",
            "ship-captured",
            "poll",
            "init",
            "run",
            "run-policy",
            "checkpoint-roster",
            "stage-roster",
            "verify-roster",
            "unrecognized",
        ] {
            let command = Command::parse(Some(&OsString::from(word)));
            assert_eq!(
                command.text(),
                match word {
                    "-h" | "--help" => "help",
                    "unrecognized" => "unknown",
                    other => other,
                }
            );
            let config = Settings {
                command,
                ..enabled()
            };
            let report = classify(Phase::Arguments, &Error::Invalid)
                .json(config)
                .to_string();
            assert_eq!(
                report.contains("\"follower_outcome\":\"unconfirmed\""),
                matches!(word, "ship" | "ship-captured")
            );
        }
        assert_eq!(Command::parse(None), Command::Unknown);
    }
}
