use super::*;
use std::{io, io::Write, net::Shutdown, os::fd::AsRawFd};

#[test]
fn blocked_diagnostics_pipe_does_not_hold_follower_admission() {
    let dir = directory("full-diagnostics");
    let source = dir.0.join("source.wdb");
    seed_primary(&source);
    let source = source.to_str().unwrap();
    let stream = "11111111111111111111111111111111";
    let description = run(&["describe", source, stream]);
    let identity = description
        .split_whitespace()
        .find_map(|word| word.strip_prefix("file_identity="))
        .unwrap();
    let folder = dir.0.join("follower");
    fs::create_dir(&folder).unwrap();
    let folder = folder.to_str().unwrap();
    let client = dir.0.join("client");
    let server = dir.0.join("server");
    credentials(&client);
    credentials(&server);
    let client_pin = client.join("cert.der");
    let server_pin = server.join("cert.der");
    let args = |maximum| {
        [
            "serve",
            folder,
            stream,
            identity,
            "127.0.0.1:0",
            server.to_str().unwrap(),
            client_pin.to_str().unwrap(),
            "--max-exchanges",
            maximum,
        ]
    };
    run(&["follower-init", folder, stream, identity]);
    let (mut initial, address) = Server::start(&dir.0, &args("1"));
    let image = dir.0.join("export.wdb");
    let shipped = run(&[
        "ship",
        source,
        stream,
        image.to_str().unwrap(),
        &address,
        client.to_str().unwrap(),
        server_pin.to_str().unwrap(),
        "--attempts",
        "1",
    ]);
    initial.finished();
    let receipt = shipped
        .lines()
        .find(|line| line.starts_with("follower durable_revision=4 "))
        .unwrap();
    let manifest = Path::new(folder).join("replica-manifest.wdb");
    let initial_manifest = fs::read(&manifest).unwrap();

    let (mut unread, mut stderr) = io::pipe().unwrap();
    // SAFETY: stderr is an open pipe writer. F_GETPIPE_SZ takes no argument.
    let capacity = unsafe { libc::fcntl(stderr.as_raw_fd(), libc::F_GETPIPE_SZ) };
    assert!(capacity > 0);
    let prefill = vec![b'x'; capacity as usize];
    stderr.write_all(&prefill).unwrap();
    let (mut follower, address) =
        Server::start_with_stderr(&dir.0, &args("5"), Stdio::from(stderr));
    // Four rejected handshakes cover every admission worker. Hold the pipe
    // full through the following authenticated poll and through server exit.
    for _ in 0..4 {
        let mut bad = TcpStream::connect(&address).unwrap();
        bad.shutdown(Shutdown::Write).unwrap();
        bad.set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        match bad.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::ConnectionReset
                ) => {}
            result => panic!("unexpected response from rejected handshake: {result:?}"),
        }
    }
    let mut poll = Server(
        Command::new(BIN)
            .args([
                "poll",
                source,
                stream,
                &address,
                client.to_str().unwrap(),
                server_pin.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(
        poll.exit_status().success(),
        "authenticated poll lost admission while the diagnostics pipe was full"
    );
    let mut output = String::new();
    poll.0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert!(output.lines().any(|line| line == receipt));
    follower.finished();
    let mut diagnostics = Vec::new();
    unread.read_to_end(&mut diagnostics).unwrap();
    assert_eq!(diagnostics, prefill);
    assert_eq!(fs::read(manifest).unwrap(), initial_manifest);
    assert_eq!(
        fs::read(Path::new(folder).join("replica-0.wdb")).unwrap(),
        fs::read(image).unwrap()
    );
    verify_follower(Path::new(folder), stream, identity);
}
