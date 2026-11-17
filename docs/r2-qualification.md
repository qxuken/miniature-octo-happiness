# R2 operational rehearsal refresh

Selected engine source is merged main
`6b657e516c13f678cd54c816db8a53ec5a11e395`. Each workload uses newly owned files
and synthetic identities. Local commits and follower durable acknowledgments
remain distinct. All Cargo manifests retain `publish = false`.

The operator and snapshot controllers are qualification tooling added after
that engine checkpoint. Their reports separately identify runner source,
scoped input hashes, actual executables and the selected engine archive;
subsequent documentation does not become a new runtime qualification.

## Maintained commands

- `cargo xtask incident-recovery` reuses the existing physical/logical recovery
  and coordinated-journal oracles. Windows now uses the existing Job-owned
  process supervisor, a one-MiB output cap, current-user directory ACLs and
  native executable suffixes. Actual selected binaries and the fresh consumer
  are retained and checked before/after the drill. The three-platform CI job
  builds selected CLIs from the exact engine checkpoint above; its controller
  and fresh consumer retain their separately reported current source.
- `cargo xtask study-peer-soak OUTPUT --source SOURCE --image IMAGE --rounds 30
  --restart-every 5` retains the existing two-host offline-edit, restart and
  repeat-sync protocol. Its output is private: the existing controller retains
  synthetic private identities inside the mode-0700 evidence directory.
- `cargo xtask snapshot-month --source SOURCE --image IMAGE --output OUTPUT`
  builds the existing live-month example and snapshot CLI from the same Git
  archive on macOS and Docker Linux. Linux build/runtime files use ext4. Only
  public certificates cross hosts; the native private key is scrubbed and the
  owned container/volume are verified absent before success. The CSV oracle
  independently checks local/captured/acknowledged revisions, four missing
  shipment replies and covering polls. Reopened current/retained data and the
  selected image's exact bytes are checked separately.

The snapshot schedule contains 30 logical days, 31 installations, 32 polls and
63 authenticated exchanges. The last receipt is revision 93 with zero gap;
prior daily local revisions are one ahead of the captured/follower revision.
A successful local heartbeat must not be reported as a follower acknowledgment.

## Results in progress

The frozen-main StudyProgress run passes 30 rounds and six paired process
interruptions on macOS/APFS and Docker Linux/ext4. Both peers finish at revision
64 with 1,395 minutes, weekly goal 230, daily limit 10 and lessons 2–61.
Repeat sync is stable, exact local revisions survive reopening, and the owned
Docker container and volume are absent. Evidence remains private under
`target/r2/study-6b657e5`; actual source, executable and report hashes will be
recorded with the completed batch.

The new snapshot CSV controls pass. Full snapshot/incident execution, independent
evidence review and hosted Windows execution remain pending. These are execution
gates, not conclusions inferred from source inspection or cross-compilation.

## Boundaries

This refresh covers named synthetic workflows and post-acknowledgment restarts.
It is not physical power-loss qualification, a wall-clock month, arbitrary
application schema/key coverage, all journal slots or deployment latency-budget
acceptance. Additional pending-intent/lost-reply and operator handoff profiles,
ordinary-account Windows deployment and the release support window retain their
explicit roadmap obligations. The R1 per-decision queue remains open.
