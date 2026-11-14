//! Mergeable state for two configured writers. Route writes to reserved rows
//! through these handles to preserve the merge rules.
use crate::{CollectionId, Database, Error, Result, Snapshot, Value};
use std::{
    io::{Read, Write},
    marker::PhantomData,
};

mod add_wins_set;
mod multi_value_register;
mod policy_register;
pub use add_wins_set::AddWinsSet;
pub use multi_value_register::MultiValueRegister;
pub use policy_register::{
    MaxRegister, MinRegister, PolicyRegister, PreferHigher, PreferLower, Resolution,
    ResolutionPolicy,
};
pub use weekend_db_macros::{Record, Resolution};

const HEADER: usize = 65;
const MAX_PAYLOAD: usize = 4096;
const MAX_FRAME: usize = HEADER + MAX_PAYLOAD;
const MAX_COMPONENT: u64 = u64::MAX / 2;

/// Shared enrollment for a replicated record. Actor IDs are nonzero and in
/// ascending order. Each actor owns one writable database. Use a fresh
/// collection ID when starting an independent history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub collection: [u8; 16],
    pub generation: u64,
    pub actors: [[u8; 16]; 2],
}

pub type CounterPolicy = Policy;

/// This writer's position in the policy's actor roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actor(usize);

impl Actor {
    pub fn index(self) -> usize {
        self.0
    }

    fn other(self) -> Self {
        Self(1 - self.0)
    }
}

impl Policy {
    fn validate(self) -> Result<()> {
        if self.collection == [0; 16]
            || self.generation == 0
            || self.actors[0] == [0; 16]
            || self.actors[0] >= self.actors[1]
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    fn matches(self, other: Self) -> Result<()> {
        if other.collection != self.collection {
            return Err(Error::ForeignStore);
        }
        if other != self {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

/// A grow-only counter with one component per actor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrowCounter([u64; 2]);

impl GrowCounter {
    pub const SIZE: usize = 16;

    pub fn total(self) -> u64 {
        self.0[0] + self.0[1]
    }

    pub fn join(&mut self, other: Self) {
        for (local, incoming) in self.0.iter_mut().zip(other.0) {
            *local = (*local).max(incoming);
        }
    }

    pub fn increment(&mut self, actor: Actor, amount: u64) -> Result<()> {
        if amount == 0 {
            return Err(Error::Invalid);
        }
        self.0[actor.0] = self.0[actor.0]
            .checked_add(amount)
            .filter(|&n| n <= MAX_COMPONENT)
            .ok_or(Error::Limit)?;
        Ok(())
    }

    pub fn to_le_bytes(self) -> [u8; Self::SIZE] {
        let mut bytes = [0; Self::SIZE];
        bytes[..8].copy_from_slice(&self.0[0].to_le_bytes());
        bytes[8..].copy_from_slice(&self.0[1].to_le_bytes());
        bytes
    }

    pub fn from_le_bytes(bytes: [u8; Self::SIZE]) -> Result<Self> {
        let components = [
            u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..].try_into().unwrap()),
        ];
        if components.iter().any(|&n| n > MAX_COMPONENT) {
            return Err(Error::Corrupt);
        }
        Ok(Self(components))
    }

    /// The incoming state may echo this actor's component up to its local total.
    pub fn check_peer(self, incoming: Self, local_actor: Actor) -> Result<()> {
        if incoming.0[local_actor.0] > self.0[local_actor.0] {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

/// A grow-only set of IDs in 0..64. A fixed ID domain keeps every union bounded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrowSet(u64);

impl GrowSet {
    pub const SIZE: usize = 8;

    pub fn insert(&mut self, id: u8) -> Result<bool> {
        let bit = 1u64.checked_shl(u32::from(id)).ok_or(Error::Bounds)?;
        let previous = self.0;
        self.0 |= bit;
        Ok(self.0 != previous)
    }

    pub fn contains(self, id: u8) -> bool {
        1u64.checked_shl(u32::from(id))
            .is_some_and(|bit| self.0 & bit != 0)
    }

    pub fn iter(self) -> impl Iterator<Item = u8> {
        (0..64).filter(move |&id| self.contains(id))
    }

    pub fn join(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn to_le_bytes(self) -> [u8; Self::SIZE] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; Self::SIZE]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }
}

/// The responding actor's published state covers the offered components.
/// Its durability follows the local database's byte-store contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeAck {
    pub policy: CounterPolicy,
    pub actor: [u8; 16],
    pub components: [u64; 2],
}

/// Application-defined state for a replicated row. Default is the empty state.
/// Use a distinct format marker for each codec and version. Payloads are fixed
/// width and capped at 4096 bytes. The replica passes exactly `SIZE` bytes to
/// the codec, and encoding must round-trip through decoding.
///
/// A merge must be deterministic, associative, commutative, and idempotent,
/// for compatible states from the same enrolled history, and must preserve
/// validity for those states. Individually decodable states need not have
/// compatible causal histories; reject detectable incompatibilities in
/// `check_peer` before merging them. Validate actor-owned fields in `check_peer`
/// by checking that the incoming state cannot advance the given local actor.
/// Compose these checks for every actor-owned field in the record. The replica
/// also uses this check with the opposite actor to validate local edits.
pub trait Record: Copy + Default + Eq {
    const KEY: &'static [u8];
    const FORMAT: &'static [u8; 8];
    const SIZE: usize;

    fn encode(self, bytes: &mut [u8]) -> Result<()>;
    fn decode(bytes: &[u8]) -> Result<Self>;
    fn merge(&mut self, other: Self);
    fn check_peer(self, incoming: Self, local_actor: Actor) -> Result<()>;

    /// Optionally resolve conflicts after a sync merge advances this record.
    /// This is a local edit by `actor`, committed together with the merge.
    /// Leave fields alone when they have no conflict to resolve.
    fn resolve_conflicts(&mut self, _actor: Actor) -> Result<()> {
        Ok(())
    }
}

impl Record for GrowSet {
    const KEY: &'static [u8] = b"grow-set";
    const FORMAT: &'static [u8; 8] = b"WDBGSET\x01";
    const SIZE: usize = GrowSet::SIZE;

    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        let bytes: &mut [u8; Self::SIZE] = bytes.try_into().map_err(|_| Error::Buffer)?;
        *bytes = self.to_le_bytes();
        Ok(())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        Ok(Self::from_le_bytes(
            bytes.try_into().map_err(|_| Error::Corrupt)?,
        ))
    }

    fn merge(&mut self, other: Self) {
        self.join(other);
    }

    fn check_peer(self, _incoming: Self, _local_actor: Actor) -> Result<()> {
        Ok(())
    }
}

impl Record for GrowCounter {
    const KEY: &'static [u8] = b"counter";
    const FORMAT: &'static [u8; 8] = b"WDBGCTR\x01";
    const SIZE: usize = GrowCounter::SIZE;

    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        let bytes: &mut [u8; Self::SIZE] = bytes.try_into().map_err(|_| Error::Buffer)?;
        *bytes = self.to_le_bytes();
        Ok(())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        Self::from_le_bytes(bytes.try_into().map_err(|_| Error::Corrupt)?)
    }

    fn merge(&mut self, other: Self) {
        self.join(other);
    }

    fn check_peer(self, other: Self, actor: Actor) -> Result<()> {
        GrowCounter::check_peer(self, other, actor)
    }
}

/// The responding actor's published state covers the offer. Its durability
/// follows the responding database's byte-store contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplicaAck<P> {
    pub policy: Policy,
    pub actor: [u8; 16],
    pub state: P,
}

#[derive(Clone, Copy)]
struct State<P> {
    policy: Policy,
    actor: Actor,
    payload: P,
}
impl<P: Record> State<P> {
    fn encode(self, buffer: &mut [u8; MAX_FRAME]) -> Result<&[u8]> {
        let bytes = &mut buffer[..HEADER + P::SIZE];
        bytes[..8].copy_from_slice(P::FORMAT);
        bytes[8..24].copy_from_slice(&self.policy.collection);
        bytes[24..32].copy_from_slice(&self.policy.generation.to_le_bytes());
        bytes[32..48].copy_from_slice(&self.policy.actors[0]);
        bytes[48..64].copy_from_slice(&self.policy.actors[1]);
        bytes[64] = self.actor.0 as u8;
        self.payload.encode(&mut bytes[HEADER..])?;
        if P::decode(&bytes[HEADER..])? != self.payload {
            return Err(Error::Invalid);
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != HEADER + P::SIZE {
            return Err(Error::Corrupt);
        }
        if &bytes[..8] != P::FORMAT {
            return Err(Error::UnsupportedFormat);
        }
        let policy = Policy {
            collection: bytes[8..24].try_into().unwrap(),
            generation: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            actors: [
                bytes[32..48].try_into().unwrap(),
                bytes[48..64].try_into().unwrap(),
            ],
        };
        policy.validate().map_err(|_| Error::Corrupt)?;
        let actor = usize::from(bytes[64]);
        if actor >= policy.actors.len() {
            return Err(Error::Corrupt);
        }
        let payload = P::decode(&bytes[HEADER..])?;
        Ok(Self {
            policy,
            actor: Actor(actor),
            payload,
        })
    }

    fn read(channel: &mut impl Read) -> Result<Self> {
        let mut buffer = [0; MAX_FRAME];
        let bytes = &mut buffer[..HEADER + P::SIZE];
        channel.read_exact(&mut bytes[..8]).map_err(|_| Error::Io)?;
        if &bytes[..8] != P::FORMAT {
            return Err(Error::UnsupportedFormat);
        }
        channel.read_exact(&mut bytes[8..]).map_err(|_| Error::Io)?;
        Self::decode(bytes)
    }

    fn write(self, channel: &mut impl Write) -> Result<()> {
        channel
            .write_all(self.encode(&mut [0; MAX_FRAME])?)
            .and_then(|_| channel.flush())
            .map_err(|_| Error::Io)
    }
}

/// One record on a configured actor's database. Use a dedicated collection
/// and route its reserved row's writes through this handle.
pub struct Replica<'db, P> {
    db: &'db Database,
    collection: CollectionId,
    policy: Policy,
    actor: Actor,
    payload: PhantomData<P>,
}
impl<'db, P: Record> Replica<'db, P> {
    pub fn enroll(
        db: &'db Database,
        collection: CollectionId,
        policy: Policy,
        actor: [u8; 16],
    ) -> Result<Self> {
        let replica = Self::configured(db, collection, policy, actor)?;
        let mut tx = db.begin();
        if tx.get(collection, P::KEY)?.is_some() {
            return Err(Error::AlreadyExists);
        }
        let state = State {
            policy,
            actor: replica.actor,
            payload: P::default(),
        };
        tx.put(collection, P::KEY, &replica.value(state)?)?;
        tx.commit()?;
        Ok(replica)
    }

    pub fn open(
        db: &'db Database,
        collection: CollectionId,
        policy: Policy,
        actor: [u8; 16],
    ) -> Result<Self> {
        let replica = Self::configured(db, collection, policy, actor)?;
        replica.read()?;
        Ok(replica)
    }

    fn configured(
        db: &'db Database,
        collection: CollectionId,
        policy: Policy,
        actor: [u8; 16],
    ) -> Result<Self> {
        policy.validate()?;
        if P::SIZE > MAX_PAYLOAD {
            return Err(Error::Limit);
        }
        let actor = policy
            .actors
            .iter()
            .position(|&id| id == actor)
            .ok_or(Error::ForeignStore)?;
        Ok(Self {
            db,
            collection,
            policy,
            actor: Actor(actor),
            payload: PhantomData,
        })
    }

    pub fn read(&self) -> Result<P> {
        self.read_at(&self.db.snapshot())
    }

    pub fn read_at(&self, snapshot: &Snapshot<'_>) -> Result<P> {
        Ok(self.state_at(snapshot)?.payload)
    }

    fn state_at(&self, snapshot: &Snapshot<'_>) -> Result<State<P>> {
        self.local_state(snapshot.get(self.collection, P::KEY)?)
    }

    fn local_state(&self, value: Option<Value>) -> Result<State<P>> {
        let value = value.ok_or(Error::NotFound)?;
        let state = State::<P>::decode(value.as_bytes().map_err(|_| Error::Corrupt)?)?;
        self.policy.matches(state.policy)?;
        if state.actor != self.actor {
            return Err(Error::ForeignStore);
        }
        Ok(state)
    }

    fn value(&self, state: State<P>) -> Result<Value> {
        self.db.bytes(state.encode(&mut [0; MAX_FRAME])?)
    }

    fn check_peer(&self, peer: State<P>, local: State<P>) -> Result<()> {
        self.policy.matches(peer.policy)?;
        if peer.actor == self.actor {
            return Err(Error::ForeignStore);
        }
        local.payload.check_peer(peer.payload, self.actor)
    }

    fn require_healthy(&self) -> Result<()> {
        if self.db.requires_recovery() {
            return Err(Error::RecoveryRequired);
        }
        Ok(())
    }

    /// Publish one monotone edit and return its local revision. Actor-owned
    /// fields may only advance for the actor supplied to the closure.
    pub fn update(&self, edit: impl FnOnce(&mut P, Actor) -> Result<()>) -> Result<u64> {
        self.require_healthy()?;
        let mut tx = self.db.begin();
        let mut state = self.local_state(tx.get(self.collection, P::KEY)?)?;
        let previous = state.payload;
        edit(&mut state.payload, self.actor)?;
        self.check_local_edit(previous, state.payload)?;
        tx.put(self.collection, P::KEY, &self.value(state)?)?;
        tx.commit()
    }

    fn check_local_edit(&self, previous: P, edited: P) -> Result<()> {
        previous.check_peer(edited, self.actor.other())?;
        Self::check_covers(edited, previous)
    }

    fn check_covers(current: P, offered: P) -> Result<()> {
        let mut joined = current;
        joined.merge(offered);
        if joined != current {
            return Err(Error::Conflict);
        }
        Ok(())
    }

    /// Offer state over a fresh duplex channel authenticated to the other actor.
    /// Set deadlines and close the channel after the exchange.
    pub fn push_authenticated(&self, mut channel: impl Read + Write) -> Result<ReplicaAck<P>> {
        self.require_healthy()?;
        let offered = self.state_at(&self.db.snapshot())?;
        offered.write(&mut channel)?;
        let peer = State::read(&mut channel)?;
        self.check_peer(peer, self.state_at(&self.db.snapshot())?)?;
        Self::check_covers(peer.payload, offered.payload)?;
        Ok(ReplicaAck {
            policy: peer.policy,
            actor: peer.policy.actors[peer.actor.0],
            state: peer.payload,
        })
    }

    /// Serve one authenticated offer and reply after any advance is committed.
    pub fn serve_authenticated(&self, mut channel: impl Read + Write) -> Result<()> {
        let peer = State::read(&mut channel)?;
        self.require_healthy()?;
        let mut tx = self.db.begin();
        let mut local = self.local_state(tx.get(self.collection, P::KEY)?)?;
        self.check_peer(peer, local)?;
        let previous = local.payload;
        local.payload.merge(peer.payload);
        if local.payload != previous {
            let merged = local.payload;
            local.payload.resolve_conflicts(self.actor)?;
            if local.payload != merged {
                self.check_local_edit(merged, local.payload)?;
            }
            tx.put(self.collection, P::KEY, &self.value(local)?)?;
            tx.commit()?;
        }
        local.write(&mut channel)
    }
}

/// Access to one counter on its actor's database. Reopen an actor on its
/// existing database. A restored older copy requires a new identity and an
/// explicit migration before it can become a writer.
pub struct Counter<'db>(Replica<'db, GrowCounter>);

impl<'db> Counter<'db> {
    /// Initialize the reserved row in an existing dedicated collection.
    pub fn enroll(
        db: &'db Database,
        collection: CollectionId,
        policy: CounterPolicy,
        actor: [u8; 16],
    ) -> Result<Self> {
        Replica::enroll(db, collection, policy, actor).map(Self)
    }

    pub fn open(
        db: &'db Database,
        collection: CollectionId,
        policy: CounterPolicy,
        actor: [u8; 16],
    ) -> Result<Self> {
        Replica::open(db, collection, policy, actor).map(Self)
    }

    pub fn count(&self) -> Result<u64> {
        self.count_at(&self.0.db.snapshot())
    }

    pub fn count_at(&self, snapshot: &Snapshot<'_>) -> Result<u64> {
        Ok(self.0.read_at(snapshot)?.total())
    }

    /// Commit a positive increment on this actor and return its local revision.
    /// Recover an uncertain commit before deciding whether to retry it.
    pub fn increment(&self, amount: u64) -> Result<u64> {
        self.0
            .update(|counter, actor| counter.increment(actor, amount))
    }

    /// Offer the current state over a fresh duplex channel authenticated to the
    /// other actor. Set deadlines and close the channel after each exchange.
    pub fn push_authenticated(&self, channel: impl Read + Write) -> Result<MergeAck> {
        let peer = self.0.push_authenticated(channel)?;
        Ok(MergeAck {
            policy: peer.policy,
            actor: peer.actor,
            components: peer.state.0,
        })
    }

    /// Serve one authenticated offer and reply after any advance is committed.
    pub fn serve_authenticated(&self, channel: impl Read + Write) -> Result<()> {
        self.0.serve_authenticated(channel)
    }
}

#[cfg(test)]
mod decisions;
