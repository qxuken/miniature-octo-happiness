use super::{Actor, MultiValueRegister, Record};
use crate::Result;
use std::marker::PhantomData;

/// A deterministic policy for two distinct choices in actor order.
/// Use the same policy on both peers. Its format marker identifies the
/// policy and version when the register is enrolled as a standalone record.
/// When embedded in a record, version the enclosing policy if this rule changes.
pub trait ResolutionPolicy: Copy + Default + Eq {
    const FORMAT: &'static [u8; 8];

    fn choose(first: u64, second: u64) -> u64;
}

/// Apply selected field policies as local edits. Derive this on a named
/// struct and mark participating fields with `#[resolve]`.
pub trait Resolution {
    fn resolve_fields(&mut self, actor: Actor) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreferHigher;

impl ResolutionPolicy for PreferHigher {
    const FORMAT: &'static [u8; 8] = b"WDBMAXR\x01";

    fn choose(first: u64, second: u64) -> u64 {
        first.max(second)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreferLower;

impl ResolutionPolicy for PreferLower {
    const FORMAT: &'static [u8; 8] = b"WDBMINR\x01";

    fn choose(first: u64, second: u64) -> u64 {
        first.min(second)
    }
}

pub type MaxRegister = PolicyRegister<PreferHigher>;
pub type MinRegister = PolicyRegister<PreferLower>;

/// A multi-value register with a resolution policy selected per field.
/// Call `resolve` in a replica update to publish the policy's decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PolicyRegister<P> {
    register: MultiValueRegister,
    policy: PhantomData<P>,
}

impl<P: ResolutionPolicy> PolicyRegister<P> {
    pub const SIZE: usize = MultiValueRegister::SIZE;

    pub fn assign(&mut self, actor: Actor, value: u64) -> Result<()> {
        self.register.assign(actor, value)
    }

    pub fn values(self) -> impl Iterator<Item = u64> {
        self.register.values()
    }

    pub fn effective_value(self) -> Option<u64> {
        self.register.resolve_with(P::choose)
    }

    /// Replace conflicting choices with the policy's result as a new
    /// assignment by this actor. A register with fewer than two distinct
    /// choices needs no assignment.
    pub fn resolve(&mut self, actor: Actor) -> Result<()> {
        let mut choices = self.register.values();
        if let (Some(first), Some(second)) = (choices.next(), choices.next()) {
            self.assign(actor, P::choose(first, second))?;
        }
        Ok(())
    }

    pub fn join(&mut self, incoming: Self) {
        self.register.join(incoming.register);
    }
}

impl<P: ResolutionPolicy> Record for PolicyRegister<P> {
    const KEY: &'static [u8] = b"policy-register";
    const FORMAT: &'static [u8; 8] = P::FORMAT;
    const SIZE: usize = Self::SIZE;

    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        self.register.encode(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            register: MultiValueRegister::decode(bytes)?,
            policy: PhantomData,
        })
    }

    fn merge(&mut self, incoming: Self) {
        self.join(incoming);
    }

    fn check_peer(self, incoming: Self, local_actor: Actor) -> Result<()> {
        self.register.check_peer(incoming.register, local_actor)
    }
}

impl<P: ResolutionPolicy> Resolution for PolicyRegister<P> {
    fn resolve_fields(&mut self, actor: Actor) -> Result<()> {
        self.resolve(actor)
    }
}
