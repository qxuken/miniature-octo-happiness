// Appended to the frozen credential fixture's process/relay/public-state helpers.
// Only Peer::start gains an explicit executable argument; transport stays in the CLIs.
#[test]
fn coordinated_cli_alpn_upgrade() {
    let old_binary = std::env::var("STUDY_OLD_BINARY").unwrap();
    let new_binary = std::env::var("STUDY_NEW_BINARY").unwrap();
    let private = Private::new();
    let root = &private.0;
    let logs = PathBuf::from(std::env::var_os("STUDY_UPGRADE_EVIDENCE").unwrap());
    fs::DirBuilder::new().mode(0o700).create(&logs).unwrap();
    let log = Arc::new(Mutex::new(File::create(logs.join("events.log")).unwrap()));
    for actor in ["a", "b"] {
        once(
            &old_binary,
            &[
                "keygen".as_ref(),
                root.join(format!("{actor}-old")).as_os_str(),
            ],
            &logs,
            &format!("keygen-{actor}"),
        );
        once(
            &old_binary,
            &[
                "init".as_ref(),
                root.join(format!("{actor}.db")).as_os_str(),
                actor.as_ref(),
                ID.as_ref(),
            ],
            &logs,
            &format!("init-{actor}"),
        );
    }
    for actor in ["a", "b"] {
        activate(root, actor, "old", &log);
    }
    let (ra, rb) = (Relay::new(), Relay::new());
    let start = |actor, phase, binary: &str| {
        Peer::start(
            root,
            &logs,
            actor,
            phase,
            if actor == "a" { &rb } else { &ra },
            if actor == "a" { &ra } else { &rb },
            (&log, binary),
        )
    };
    let (mut a, mut b) = (
        start("a", "old-old", &old_binary),
        start("b", "old-old", &old_binary),
    );
    a.local("edit 1 5 30 60");
    b.local("edit 2 7 40 50");
    converge(&mut a, &mut b, 12, 40, 50, &[1, 2]);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    let mut retained = Vec::new();
    for actor in ["a", "b"] {
        let db = Database::open_file(root.join(format!("{actor}.db")), Limits::default()).unwrap();
        let view = db.snapshot();
        let revision = view.revision();
        let id = db.retain_snapshot(&view).unwrap();
        retained.push((revision, id));
    }
    let old_states = [core(root, "a").0, core(root, "b").0];
    assert_eq!(old_states[0], old_states[1]);
    event(
        &log,
        "PASS old-old complete convergence and two retained roots",
    );
    let (mut a, mut b) = (
        start("a", "mixed-new-a", &new_binary),
        start("b", "mixed-old-b", &old_binary),
    );
    let ar = a.local("edit 3 11 80 45");
    let br = b.local("edit 4 13 70 20");
    a.state().assert_values(23, 80, 45, &[1, 2, 3]);
    b.state().assert_values(25, 70, 20, &[1, 2, 4]);
    let before = [a.state(), b.state()];
    a.command("push on");
    b.command("push on");
    let deadline = Instant::now() + BUDGET;
    loop {
        let reports = [a.report(), b.report()];
        assert!(
            reports.iter().all(|(n, _)| *n == 0),
            "mixed binary must never acknowledge: {reports:?}"
        );
        let text = reports
            .iter()
            .map(|(_, s)| s.to_lowercase())
            .collect::<Vec<_>>();
        if text
            .iter()
            .all(|s| s.contains(" error ") && !s.contains("refused") && !s.contains("timed out"))
            && text.iter().any(|s| {
                s.contains("alpn")
                    || s.contains("application protocol")
                    || s.contains("noapplicationprotocol")
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "reachable ALPN rejection missing: {reports:?}"
        );
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(
        [a.state(), b.state()],
        before,
        "no remote publication on mixed-version rejection"
    );
    assert_eq!([before[0].revision, before[1].revision], [ar, br]);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    assert_eq!(core(root, "a").1, ar);
    assert_eq!(core(root, "b").1, br);
    event(
        &log,
        "PASS reachable mixed-version ALPN rejection, zero acknowledgments, local commits preserved",
    );
    let (mut a, mut b) = (
        start("a", "new-new", &new_binary),
        start("b", "new-new", &new_binary),
    );
    a.state().assert_values(23, 80, 45, &[1, 2, 3]);
    b.state().assert_values(25, 70, 20, &[1, 2, 4]);
    let revisions = converge(&mut a, &mut b, 36, 80, 20, &[1, 2, 3, 4]);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    let (mut a, mut b) = (
        start("a", "restart-new-new", &new_binary),
        start("b", "restart-new-new", &new_binary),
    );
    assert_eq!(
        converge(&mut a, &mut b, 36, 80, 20, &[1, 2, 3, 4]),
        revisions
    );
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    let states = [core(root, "a"), core(root, "b")];
    assert_eq!(states[0].0, states[1].0);
    for (i, (state, rev, historical)) in states.iter().enumerate() {
        assert_eq!(state.practice_minutes.total(), 36);
        assert_eq!(state.weekly_goal.values().collect::<Vec<_>>(), [80]);
        assert_eq!(state.session_limit.values().collect::<Vec<_>>(), [20]);
        assert_eq!(state.completed.iter().collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(state.planned, Default::default());
        assert_eq!(*rev, revisions[i]);
        let (record, revision, id) = historical.as_ref().unwrap();
        assert_eq!(record, &old_states[i]);
        assert_eq!((*revision, *id), retained[i]);
    }
    event(
        &log,
        format!(
            "PASS complete causal records total36 goal80 limit20 lessons1..4; both retained roots preserved; repeat/restart no commit {revisions:?}"
        ),
    );
}
