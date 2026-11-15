//! Forward replay and byte digest independent of the Weekend DB fixture.
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct Work {
    pub commits: u64,
    pub keys: u64,
    pub batch: u64,
    pub payload: u64,
    pub retain_every: u64,
    pub reopens: u64,
}

pub type State = BTreeMap<u64, Vec<u8>>;
pub struct Expected {
    pub current: State,
    pub retained: BTreeMap<u64, State>,
    pub digest: u64,
}

impl Work {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (1..=512).contains(&self.commits)
                && (1..=256).contains(&self.keys)
                && (1..=self.keys).contains(&self.batch)
                && (16..=8192).contains(&self.payload)
                && (1..=128).contains(&self.reopens)
                && self.retain_every > 0
                && self.commits.div_ceil(self.retain_every) <= 64
                && self.keys * self.payload <= 1024 * 1024
                && self.commits * self.batch * self.payload <= 64 * 1024 * 1024,
            "workload exceeds the bounded supported range"
        );
        Ok(())
    }

    pub fn payload(self, key: u64, epoch: u64) -> Vec<u8> {
        let mut bytes = (0..self.payload)
            .map(|offset| ((epoch * 17 + key * 31 + offset) % 251) as u8)
            .collect::<Vec<_>>();
        bytes[..8].copy_from_slice(&epoch.to_le_bytes());
        bytes[8..16].copy_from_slice(&key.to_le_bytes());
        bytes
    }

    pub fn expected(self) -> Expected {
        let mut current = (0..self.keys)
            .map(|key| (key, self.payload(key, 0)))
            .collect::<State>();
        let mut retained = BTreeMap::new();
        for epoch in 1..=self.commits {
            for offset in 0..self.batch {
                let key = (epoch * self.batch + offset) % self.keys;
                current.insert(key, self.payload(key, epoch));
            }
            if epoch % self.retain_every == 0 || epoch == self.commits {
                retained.insert(epoch, current.clone());
            }
        }
        let mut digest = 14_695_981_039_346_656_037_u64;
        let mut feed = |data: &[u8]| {
            for byte in data {
                digest = (digest ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
            }
        };
        for (epoch, state) in std::iter::once((&self.commits, &current)).chain(&retained) {
            feed(&epoch.to_le_bytes());
            for (key, data) in state {
                feed(&key.to_le_bytes());
                feed(data);
            }
        }
        Expected {
            current,
            retained,
            digest,
        }
    }
}
