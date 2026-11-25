//! Durable client-side state for one assigned coordinator slot.
use super::{
    AppResult, Secret, USAGE, address,
    protocol::sequenced::{INITIAL_REVISION, MAX_SLOTS, Receipt, hex, unhex},
    request,
};
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    path::Path,
};
use weekend_db::{Database, Error, Limits, Result};

const KEY: &[u8] = b"coordinator-journal";
const MAGIC: &[u8; 8] = b"WDBJRN01";

#[derive(Clone, Copy)]
struct Pending {
    expected: u64,
    value: u64,
}

#[derive(Clone, Copy)]
struct State {
    identity: [u8; 16],
    slot: u16,
    last: Option<Receipt>,
    pending: Option<Pending>,
}

impl State {
    fn sequence(&self) -> Result<u64> {
        self.last
            .map_or(0, |receipt| receipt.sequence)
            .checked_add(1)
            .ok_or(Error::Exhausted)
    }

    fn floor(&self) -> u64 {
        self.last
            .map_or(INITIAL_REVISION, |receipt| receipt.revision)
    }

    fn encode(&self) -> [u8; 76] {
        let mut bytes = [0; 76];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..24].copy_from_slice(&self.identity);
        bytes[24..26].copy_from_slice(&self.slot.to_le_bytes());
        if let Some(receipt) = self.last {
            bytes[26] |= 1;
            bytes[27..60].copy_from_slice(&receipt.encode());
        }
        if let Some(pending) = self.pending {
            bytes[26] |= 2;
            bytes[60..68].copy_from_slice(&pending.expected.to_le_bytes());
            bytes[68..76].copy_from_slice(&pending.value.to_le_bytes());
        }
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 76 || &bytes[..8] != MAGIC || bytes[26] & !3 != 0 {
            return Err(Error::Corrupt);
        }
        let identity = bytes[8..24].try_into().unwrap();
        let slot = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
        if identity == [0; 16] || u64::from(slot) >= MAX_SLOTS {
            return Err(Error::Corrupt);
        }
        let last = if bytes[26] & 1 != 0 {
            Some(Receipt::from_bytes(&bytes[27..60], u64::MAX)?)
        } else {
            if bytes[27..60] != [0; 33] {
                return Err(Error::Corrupt);
            }
            None
        };
        let pending = if bytes[26] & 2 != 0 {
            Some(Pending {
                expected: u64::from_le_bytes(bytes[60..68].try_into().unwrap()),
                value: u64::from_le_bytes(bytes[68..76].try_into().unwrap()),
            })
        } else {
            if bytes[60..76] != [0; 16] {
                return Err(Error::Corrupt);
            }
            None
        };
        let state = Self {
            identity,
            slot,
            last,
            pending,
        };
        if let Some(pending) = pending
            && (pending.expected < state.floor()
                || pending.expected == u64::MAX
                || state.sequence().is_err())
        {
            return Err(Error::Corrupt);
        }
        Ok(state)
    }
}

struct Journal {
    db: Database,
    state: State,
}

impl Journal {
    fn load(db: Database) -> Result<Self> {
        let row = db
            .snapshot()
            .get(db.collection_id(1), KEY)?
            .ok_or(Error::UnsupportedFormat)?;
        let state = State::decode(row.as_bytes().map_err(|_| Error::Corrupt)?)?;
        Ok(Self { db, state })
    }

    fn open(path: &Path) -> Result<Self> {
        Self::load(Database::open_file(path, Limits::default())?)
    }

    fn create(db: Database, state: State) -> Result<Self> {
        assert_eq!(db.create_collection()?.as_u64(), 1);
        let mut journal = Self { db, state };
        journal.save(state)?;
        Ok(journal)
    }

    fn save(&mut self, state: State) -> Result<()> {
        let mut tx = self.db.begin();
        tx.put(
            self.db.collection_id(1),
            KEY,
            &self.db.bytes(&state.encode())?,
        )?;
        tx.commit()?;
        self.state = state;
        Ok(())
    }

    fn set(
        &mut self,
        intended: Pending,
        send: &mut impl FnMut(&str) -> AppResult<String>,
    ) -> AppResult<String> {
        if self.state.pending.is_some() {
            return Err("journal has a pending write; run journal resume".into());
        }
        self.state.sequence()?;
        if intended.expected < self.state.floor() || intended.expected == u64::MAX {
            return Err("expected revision is outside the journal's admissible range".into());
        }
        let reply = send(&format!("journal-get {}", hex(&self.state.identity)))?;
        let (revision, value) = parse_state(&reply, "value")?;
        if revision < self.state.floor() || intended.expected > revision {
            return Err(
                "coordinator is behind the expected revision; reconcile before writing".into(),
            );
        }
        if intended.expected < revision {
            return Ok(format!("conflict {revision} {value}"));
        }
        self.save(State {
            pending: Some(intended),
            ..self.state
        })?;
        self.resume(send)
    }

    fn resume(&mut self, send: &mut impl FnMut(&str) -> AppResult<String>) -> AppResult<String> {
        let Some(pending) = self.state.pending else {
            return Ok(self.state.last.map_or_else(
                || "idle".to_owned(),
                |receipt| format!("committed {}", receipt.revision),
            ));
        };
        let sequence = self.state.sequence()?;
        let previous = self
            .state
            .last
            .map_or_else(|| "-".to_owned(), |receipt| hex(&receipt.encode()));
        let command = format!(
            "journal-set {} {} {sequence} {previous} {} {}",
            hex(&self.state.identity),
            self.state.slot,
            pending.expected,
            pending.value
        );
        let reply = send(&command)?;
        let revision = pending.expected.checked_add(1).ok_or(Error::Exhausted)?;
        if reply == format!("committed {revision}") {
            let receipt = Receipt {
                sequence,
                expected: pending.expected,
                value: pending.value,
                revision,
            };
            self.save(State {
                last: Some(receipt),
                pending: None,
                ..self.state
            })?;
        } else if let Ok((current, _)) = parse_state(&reply, "conflict")
            && current > pending.expected
        {
            self.save(State {
                pending: None,
                ..self.state
            })?;
        } else {
            return Err("coordinator did not confirm the request or a newer conflict; inspect the pending journal before retry".into());
        }
        Ok(reply)
    }

    fn check(&self, send: &mut impl FnMut(&str) -> AppResult<String>) -> AppResult<String> {
        let reply = send(&format!(
            "journal-check {} {}",
            hex(&self.state.identity),
            self.state.slot
        ))?;
        if reply.starts_with("error ") {
            return Err(format!("journal check failed: {reply}").into());
        }
        let mut words = reply.split(' ');
        let (Some("checkpoint"), Some(revision), Some(value), Some(receipt), None) = (
            words.next(),
            words.next(),
            words.next(),
            words.next(),
            words.next(),
        ) else {
            return Err("invalid coordinator checkpoint response".into());
        };
        let revision = canonical_number(revision)?;
        let value = canonical_number(value)?;
        if revision < INITIAL_REVISION {
            return Err("invalid coordinator checkpoint revision".into());
        }
        let remote = if receipt == "-" {
            None
        } else {
            let receipt = Receipt::from_bytes(&unhex::<33>(receipt)?, revision)?;
            if receipt.revision == revision && receipt.value != value {
                return Err("coordinator checkpoint value disagrees with its head receipt".into());
            }
            Some(receipt)
        };
        let state = self.state;
        let required = state
            .pending
            .map_or_else(|| state.floor(), |intent| intent.expected);
        if revision < required {
            return Err(
                format!("behind {revision} {value}: journal requires revision {required}").into(),
            );
        }
        if let Some(intent) = state.pending {
            let intended = Receipt {
                sequence: state.sequence()?,
                expected: intent.expected,
                value: intent.value,
                revision: intent.expected.checked_add(1).ok_or(Error::Exhausted)?,
            };
            if remote == Some(intended) {
                return Ok(format!("pending-committed {revision} {value}"));
            }
        }
        if remote != state.last {
            let receipt = remote.map_or_else(|| "receipt 0".to_owned(), |receipt| receipt.reply());
            return Err(format!(
                "receipt-mismatch {revision} {value}: coordinator reports {receipt}"
            )
            .into());
        }
        let status = match state.pending {
            None => "aligned",
            Some(intent) if revision == intent.expected => "pending-eligible",
            Some(_) => "pending-stale",
        };
        Ok(format!("{status} {revision} {value}"))
    }

    fn status(&self) -> AppResult<()> {
        let mut stdout = io::stdout().lock();
        writeln!(
            stdout,
            "journal {} {}",
            hex(&self.state.identity),
            self.state.slot
        )?;
        writeln!(
            stdout,
            "{}",
            self.state
                .last
                .map_or_else(|| "receipt 0".into(), |receipt| receipt.reply())
        )?;
        if let Some(pending) = self.state.pending {
            writeln!(
                stdout,
                "pending {} {} {}",
                self.state.sequence()?,
                pending.expected,
                pending.value
            )?;
        }
        Ok(())
    }
}

fn parse_state(reply: &str, kind: &str) -> AppResult<(u64, u64)> {
    let mut words = reply.split(' ');
    let (Some(actual), Some(revision), Some(value), None) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return Err("invalid coordinator response".into());
    };
    if actual != kind {
        return Err("unexpected coordinator response".into());
    }
    Ok((canonical_number(revision)?, canonical_number(value)?))
}

fn canonical_number(word: &str) -> AppResult<u64> {
    let number: u64 = word.parse()?;
    if word != number.to_string() {
        return Err("invalid coordinator number".into());
    }
    Ok(number)
}

fn number(word: &OsStr) -> AppResult<u64> {
    Ok(word.to_str().ok_or("expected a decimal number")?.parse()?)
}

fn sender(port: &OsStr, secret: &OsStr) -> AppResult<impl FnMut(&str) -> AppResult<String>> {
    let address = address(port)?;
    let secret = Secret::load(Path::new(secret))?;
    Ok(move |command: &str| request(address, Some(&secret), command))
}

pub(super) fn run(args: &[OsString]) -> AppResult<()> {
    match args {
        [action, path] if action == "status" => Journal::open(Path::new(path))?.status(),
        [action, path, port, slot, flag, secret] if action == "init" && flag == "--secret-file" => {
            let slot = number(slot)?;
            if slot >= MAX_SLOTS {
                return Err(Error::Invalid.into());
            }
            let mut send = sender(port, secret)?;
            let reply = send("identity")?;
            let identity = reply
                .strip_prefix("identity ")
                .ok_or("invalid coordinator identity response")
                .and_then(|data| {
                    unhex::<16>(data).map_err(|_| "invalid coordinator identity response")
                })?;
            if identity == [0; 16]
                || send(&format!("journal-receipt {} {slot}", hex(&identity)))? != "receipt 0"
            {
                return Err("journal initialization requires an empty assigned slot and a valid coordinator identity".into());
            }
            let db = Database::create_file(Path::new(path), Limits::default())?;
            Journal::create(
                db,
                State {
                    identity,
                    slot: slot as u16,
                    last: None,
                    pending: None,
                },
            )?
            .status()
        }
        [action, path, port, flag, secret] if action == "resume" && flag == "--secret-file" => {
            let mut journal = Journal::open(Path::new(path))?;
            let reply = journal.resume(&mut sender(port, secret)?)?;
            writeln!(io::stdout().lock(), "{reply}")?;
            Ok(())
        }
        [action, path, port, flag, secret] if action == "check" && flag == "--secret-file" => {
            let journal = Journal::open(Path::new(path))?;
            let reply = journal.check(&mut sender(port, secret)?)?;
            writeln!(io::stdout().lock(), "{reply}")?;
            Ok(())
        }
        [action, path, port, expected, value, flag, secret]
            if action == "set" && flag == "--secret-file" =>
        {
            let intended = Pending {
                expected: number(expected)?,
                value: number(value)?,
            };
            let mut journal = Journal::open(Path::new(path))?;
            let reply = journal.set(intended, &mut sender(port, secret)?)?;
            writeln!(io::stdout().lock(), "{reply}")?;
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::protocol::sequenced;
    use weekend_db::storage::{Crash, Fault, IoKind, SimulatedDisk};

    const IDENTITY: [u8; 16] = [42; 16];
    const INTENT: Pending = Pending {
        expected: 3,
        value: 20,
    };

    fn create(disk: &SimulatedDisk) -> Journal {
        let db = Database::create_on(disk.clone(), [33; 16], Limits::default()).unwrap();
        Journal::create(
            db,
            State {
                identity: IDENTITY,
                slot: 7,
                last: None,
                pending: None,
            },
        )
        .unwrap()
    }

    fn open(disk: &SimulatedDisk) -> Journal {
        Journal::load(Database::open_on(disk.clone(), Limits::default()).unwrap()).unwrap()
    }

    #[test]
    fn journal_decode_rejects_each_noncanonical_field() {
        let state = State {
            identity: IDENTITY,
            slot: (MAX_SLOTS - 1) as u16,
            last: Some(Receipt {
                sequence: 1,
                expected: 3,
                value: 12,
                revision: 4,
            }),
            pending: Some(Pending {
                expected: 4,
                value: 20,
            }),
        };
        let full = state.encode();
        assert_eq!(State::decode(&full).unwrap().encode(), full);
        let empty = State {
            last: None,
            pending: None,
            ..state
        }
        .encode();
        assert_eq!(State::decode(&empty).unwrap().encode(), empty);

        let mut oversized = full.to_vec();
        oversized.push(0);
        assert!(matches!(State::decode(&oversized), Err(Error::Corrupt)));
        assert!(matches!(State::decode(&full[..75]), Err(Error::Corrupt)));

        for (label, original, offset, replacement) in [
            ("magic", full, 0, &[0][..]),
            ("reserved flag", full, 26, &[0b111]),
            ("zero identity", full, 8, &[0; 16]),
            (
                "slot past capacity",
                full,
                24,
                &(MAX_SLOTS as u16).to_le_bytes(),
            ),
            ("unknown receipt version", full, 27, &[2]),
            ("absent receipt has payload", empty, 27, &[1]),
            ("absent intent has payload", empty, 60, &[1]),
            ("intent below receipt floor", full, 60, &3_u64.to_le_bytes()),
            (
                "exhausted intent revision",
                full,
                60,
                &u64::MAX.to_le_bytes(),
            ),
        ] {
            let mut bytes = original;
            bytes[offset..offset + replacement.len()].copy_from_slice(replacement);
            assert!(
                matches!(State::decode(&bytes), Err(Error::Corrupt)),
                "{label}"
            );
        }
    }

    #[test]
    fn journal_check_observes_without_resolving_an_intent() {
        let local = SimulatedDisk::new().unwrap();
        let mut journal = create(&local);
        journal
            .save(State {
                pending: Some(INTENT),
                ..journal.state
            })
            .unwrap();
        let before = local.durable_image().unwrap();
        let mut remote =
            Database::create_on(SimulatedDisk::new().unwrap(), IDENTITY, Limits::default())
                .unwrap();
        sequenced::initialize(&remote).unwrap();
        assert_eq!(
            journal
                .check(&mut |command| Ok(sequenced::handle(&mut remote, command)?))
                .unwrap(),
            "pending-eligible 3 0"
        );
        assert_eq!(
            sequenced::handle(&mut remote, "set-seq 6 1 3 14").unwrap(),
            "committed 4"
        );
        assert_eq!(
            journal
                .check(&mut |command| Ok(sequenced::handle(&mut remote, command)?))
                .unwrap(),
            "pending-stale 4 14"
        );
        assert_eq!(local.durable_image().unwrap(), before);
        assert_eq!(
            sequenced::handle(&mut remote, "receipt 7").unwrap(),
            "receipt 0"
        );
    }

    #[test]
    fn journal_check_rejects_invalid_checkpoint_responses() {
        let local = SimulatedDisk::new().unwrap();
        let journal = create(&local);
        let receipt = hex(&Receipt {
            sequence: 1,
            expected: 3,
            value: 20,
            revision: 4,
        }
        .encode());
        for reply in [
            "value 3 0".into(),
            "checkpoint 3 0".into(),
            "checkpoint 3 0 - extra".into(),
            "checkpoint 2 0 -".into(),
            "checkpoint 03 0 -".into(),
            "checkpoint 3 +0 -".into(),
            "checkpoint 3 0 bad".into(),
            format!("checkpoint 3 20 {receipt}"),
            format!("checkpoint 4 21 {receipt}"),
        ] {
            assert!(
                journal.check(&mut |_| Ok(reply.clone())).is_err(),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_failed_journal_stage_never_sends_the_mutating_request() {
        let medium = SimulatedDisk::new().unwrap();
        let mut control = create(&medium);
        let base = medium.durable_image().unwrap();
        medium.arm(usize::MAX, Fault::Error);
        control
            .save(State {
                pending: Some(INTENT),
                ..control.state
            })
            .unwrap();
        let trace = medium.trace().unwrap();
        let root = root_sync(&trace);
        for step in 0..trace.len() {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let medium = SimulatedDisk::from_image(&base).unwrap();
                let mut journal = open(&medium);
                medium.arm(step, Fault::Error);
                assert!(
                    journal
                        .set(INTENT, &mut |command| {
                            assert_eq!(command, format!("journal-get {}", hex(&IDENTITY)));
                            Ok("value 3 0".to_owned())
                        })
                        .is_err()
                );
                assert_eq!(medium.trace().unwrap(), trace[..=step]);
                drop(journal);
                medium.clear_fault();
                medium.crash(crash).unwrap();
                let journal = open(&medium);
                let saved = step > root || (step == root && matches!(crash, Crash::PersistAll));
                assert_eq!(journal.state.pending.is_some(), saved);
                assert!(journal.state.last.is_none());
            }
        }
    }

    fn root_sync(trace: &[weekend_db::storage::IoEvent]) -> usize {
        let syncs: Vec<_> = trace
            .iter()
            .enumerate()
            .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
            .collect();
        let [_, root, _]: [usize; 3] = syncs.try_into().unwrap();
        root
    }

    #[test]
    fn busy_keeps_the_intent_for_exact_retry_after_reopen() {
        let local = SimulatedDisk::new().unwrap();
        let mut journal = create(&local);
        let mut rejected = String::new();
        assert!(
            journal
                .set(INTENT, &mut |command| {
                    if command.starts_with("journal-get ") {
                        Ok("value 3 0".to_owned())
                    } else {
                        command.clone_into(&mut rejected);
                        Ok("error Busy".to_owned())
                    }
                })
                .is_err()
        );
        drop(journal);

        let medium = SimulatedDisk::new().unwrap();
        let mut remote = Database::create_on(medium, IDENTITY, Limits::default()).unwrap();
        sequenced::initialize(&remote).unwrap();
        let mut journal = open(&local);
        let pending = journal.state.pending.unwrap();
        assert_eq!(
            (pending.expected, pending.value),
            (INTENT.expected, INTENT.value)
        );
        assert!(journal.state.last.is_none());
        let reply = journal
            .resume(&mut |command| {
                assert_eq!(command, rejected);
                Ok(sequenced::handle(&mut remote, command)?)
            })
            .unwrap();
        assert_eq!(reply, "committed 4");
        assert!(journal.state.pending.is_none());
        assert_eq!(sequenced::handle(&mut remote, "get").unwrap(), "value 4 20");
    }

    #[test]
    fn failed_local_acknowledgment_reopens_pending_or_receipt_after_remote_commit() {
        let remote_medium = SimulatedDisk::new().unwrap();
        let mut remote =
            Database::create_on(remote_medium.clone(), IDENTITY, Limits::default()).unwrap();
        sequenced::initialize(&remote).unwrap();
        let local = SimulatedDisk::new().unwrap();
        let mut journal = create(&local);
        let failure = journal.set(INTENT, &mut |command| {
            let reply = sequenced::handle(&mut remote, command)?;
            if command.starts_with("journal-set ") {
                assert_eq!(reply, "committed 4");
                return Err("lost reply".into());
            }
            Ok(reply)
        });
        assert!(failure.is_err());
        drop(journal);
        local.crash(Crash::DropAll).unwrap();
        drop(remote);
        remote_medium.crash(Crash::DropAll).unwrap();
        let mut remote = Database::open_on(remote_medium, Limits::default()).unwrap();
        let pending = local.durable_image().unwrap();

        let mut control = open(&local);
        local.arm(usize::MAX, Fault::Error);
        assert_eq!(
            control
                .resume(&mut |command| Ok(sequenced::handle(&mut remote, command)?))
                .unwrap(),
            "committed 4"
        );
        let trace = local.trace().unwrap();
        let root = root_sync(&trace);
        for step in 0..trace.len() {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let local = SimulatedDisk::from_image(&pending).unwrap();
                let mut journal = open(&local);
                local.arm(step, Fault::Error);
                assert!(
                    journal
                        .resume(&mut |command| Ok(sequenced::handle(&mut remote, command)?))
                        .is_err()
                );
                assert_eq!(local.trace().unwrap(), trace[..=step]);
                drop(journal);
                local.clear_fault();
                local.crash(crash).unwrap();
                let mut journal = open(&local);
                let saved = step > root || (step == root && matches!(crash, Crash::PersistAll));
                assert_eq!(journal.state.pending.is_none(), saved);
                assert_eq!(journal.state.last.is_some(), saved);
                let mut requests = 0;
                assert_eq!(
                    journal
                        .resume(&mut |command| {
                            requests += 1;
                            Ok(sequenced::handle(&mut remote, command)?)
                        })
                        .unwrap(),
                    "committed 4"
                );
                assert_eq!(requests, usize::from(!saved));
                assert_eq!(remote.snapshot().revision(), 4);
                assert_eq!(
                    sequenced::handle(&mut remote, "receipt 7").unwrap(),
                    "receipt 1 3 20 4"
                );
                drop(journal);
                local.crash(Crash::DropAll).unwrap();
                let journal = open(&local);
                assert!(journal.state.pending.is_none());
                assert_eq!(journal.state.last.unwrap().revision, 4);
            }
        }
    }

    #[test]
    fn only_an_exact_acknowledgment_or_newer_conflict_clears_pending() {
        let medium = SimulatedDisk::new().unwrap();
        let mut journal = create(&medium);
        let staged = State {
            pending: Some(INTENT),
            ..journal.state
        };
        journal.save(staged).unwrap();
        for response in [
            "committed 5",
            "conflict 3 0",
            "conflict 2 0",
            "conflict +4 8",
            "retired 1",
            "error Conflict",
            "malformed",
        ] {
            assert!(journal.resume(&mut |_| Ok(response.into())).is_err());
            assert_eq!(journal.state.encode(), staged.encode());
        }
        assert_eq!(
            journal.resume(&mut |_| Ok("conflict 4 8".into())).unwrap(),
            "conflict 4 8"
        );
        drop(journal);
        medium.crash(Crash::DropAll).unwrap();
        let journal = open(&medium);
        assert!(journal.state.pending.is_none());
        assert_eq!(journal.state.sequence(), Ok(1));
    }

    #[test]
    fn lost_reply_after_commit_replays_the_receipt_after_another_writer_advances() {
        let local = SimulatedDisk::new().unwrap();
        let medium = SimulatedDisk::new().unwrap();
        let mut journal = create(&local);
        let mut remote = Database::create_on(medium.clone(), IDENTITY, Limits::default()).unwrap();
        sequenced::initialize(&remote).unwrap();
        let result = journal.set(INTENT, &mut |command| {
            let reply = sequenced::handle(&mut remote, command)?;
            if command.starts_with("journal-set ") {
                assert_eq!(reply, "committed 4");
                return Err("reply lost".into());
            }
            Ok(reply)
        });
        assert_eq!(result.unwrap_err().to_string(), "reply lost");
        let check = |reply: &str| journal.check(&mut |_| Ok(reply.to_owned()));
        let receipt = hex(&Receipt {
            sequence: 1,
            expected: 3,
            value: 20,
            revision: 4,
        }
        .encode());
        assert_eq!(
            check(&format!("checkpoint 4 20 {receipt}")).unwrap(),
            "pending-committed 4 20"
        );
        assert_eq!(
            check(&format!("checkpoint 4 99 {receipt}"))
                .unwrap_err()
                .to_string(),
            "coordinator checkpoint value disagrees with its head receipt"
        );
        assert_eq!(
            sequenced::handle(&mut remote, "set-seq 6 1 4 99").unwrap(),
            "committed 5"
        );
        drop(journal);
        drop(remote);
        local.crash(Crash::DropAll).unwrap();
        medium.crash(Crash::DropAll).unwrap();

        let mut journal = open(&local);
        let mut remote = Database::open_on(medium.clone(), Limits::default()).unwrap();
        let before_local = local.durable_image().unwrap();
        let before_remote = medium.durable_image().unwrap();
        let mut send =
            |command: &str| -> AppResult<String> { Ok(sequenced::handle(&mut remote, command)?) };
        assert_eq!(journal.check(&mut send).unwrap(), "pending-committed 5 99");
        assert_eq!(local.durable_image().unwrap(), before_local);
        assert_eq!(journal.resume(&mut send).unwrap(), "committed 4");
        assert_eq!(journal.check(&mut send).unwrap(), "aligned 5 99");
        assert_eq!(medium.durable_image().unwrap(), before_remote);

        drop(journal);
        local.crash(Crash::DropAll).unwrap();
        let mut journal = open(&local);
        let receipt = journal.state.last.unwrap();
        assert_eq!(
            (
                receipt.sequence,
                receipt.expected,
                receipt.value,
                receipt.revision
            ),
            (1, 3, 20, 4)
        );
        assert!(journal.state.pending.is_none());
        assert_eq!(
            journal
                .resume(&mut |_| panic!("completed intent was resent"))
                .unwrap(),
            "committed 4"
        );
    }

    #[test]
    fn guarded_request_fits_the_authentication_and_line_limits() {
        let local = SimulatedDisk::new().unwrap();
        let mut journal = create(&local);
        journal.state.slot = 1023;
        journal.state.last = Some(Receipt {
            sequence: u64::MAX - 5,
            expected: u64::MAX - 4,
            value: u64::MAX,
            revision: u64::MAX - 3,
        });
        journal.state.pending = Some(Pending {
            expected: u64::MAX - 3,
            value: u64::MAX,
        });
        journal
            .resume(&mut |command| {
                let wire = format!("auth {} {command}\n", "a".repeat(64));
                let framed =
                    crate::native::protocol::read_line_from(&mut wire.as_bytes(), || Ok(()))?;
                assert_eq!(
                    crate::native::protocol::authorize(&[b'a'; 64], &framed),
                    Some(command)
                );
                Ok(format!("committed {}", u64::MAX - 2))
            })
            .unwrap();
    }
}
