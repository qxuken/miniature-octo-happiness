//! Monotonic, process-local measurements of accepted authenticated polls.
use super::{Options, Record, State, observe_with_prior};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Monitor {
    last: Option<(Record, Duration)>,
    target: Option<(u64, Duration)>,
}

impl Monitor {
    pub(super) fn sample(
        &mut self,
        number: u8,
        elapsed: Duration,
        probe: Duration,
        record: Record,
    ) -> Value {
        let accepted = record.error.is_none();
        let mut gap = None;
        let mut caught_up = None;
        if accepted {
            let local = record.local.revision.unwrap();
            let follower = record.follower.revision;
            gap = follower.map(|revision| local - revision);
            if let Some((revision, first_seen)) = self.target
                && follower.is_some_and(|follower| follower >= revision)
            {
                caught_up = Some(json!({
                    "revision":revision, "observed_after_ms":(elapsed - first_seen).as_millis(),
                }));
                self.target = None;
            }
            if follower != Some(local) && self.target.is_none() {
                self.target = Some((local, elapsed));
            }
            self.last = Some((record.clone(), elapsed));
        }
        let last_durable = self.last.as_ref().and_then(|(last, seen)| {
            (last.follower.state == State::Durable).then(|| {
                json!({
                    "revision":last.follower.revision,
                    "digest":last.follower.digest,
                    "observed_at":last.observed_at,
                    "age_ms":(elapsed - *seen).as_millis(),
                })
            })
        });
        let waiting_for = if accepted {
            self.target.map(|(revision, first_seen)| {
                json!({
                    "revision":revision, "observed_for_ms":(elapsed - first_seen).as_millis()
                })
            })
        } else {
            None
        };
        json!({
            "schema":"weekend-replica-monitor", "version":1, "sample":number,
            "elapsed_ms":elapsed.as_millis(), "probe_ms":probe.as_millis(),
            "accepted":accepted, "revision_gap":gap,
            "last_durable":last_durable, "waiting_for":waiting_for,
            "caught_up":caught_up, "observation":record,
        })
    }
}

pub(super) fn run(options: &Options, samples: u8, mut output: impl Write) -> Result<()> {
    let origin = Instant::now();
    let interval = Duration::from_millis(options.interval_ms.unwrap_or(1_000));
    let mut monitor = Monitor::default();
    for number in 1..=samples {
        if number != 1 {
            thread::sleep(interval);
        }
        let start = origin.elapsed();
        let record = observe_with_prior(options, monitor.last.as_ref().map(|(r, _)| r))?;
        let error = record.error;
        let elapsed = origin.elapsed();
        let sample = monitor.sample(number, elapsed, elapsed - start, record);
        serde_json::to_writer(&mut output, &sample)?;
        output.write_all(b"\n")?;
        output.flush()?;
        if number == samples {
            ensure!(
                error.is_none(),
                "last replica observation failed: {}",
                sample["observation"]["error"]
            );
        }
    }
    Ok(())
}
