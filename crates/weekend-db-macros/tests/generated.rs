//! Runtime probes for emitted code using a deliberately tiny independent trait
//! implementation. Real engine compatibility/convergence is covered by tests/crdt
//! and the typed crate's model tests; this isolates field order and propagation.
use std::cell::RefCell;
use weekend_db_macros::{Record, Resolution};

mod api {
    #[derive(Debug, PartialEq, Eq)]
    pub enum Error {
        Buffer,
        Corrupt,
        Conflict,
    }
    pub type Result<T> = std::result::Result<T, Error>;
    pub mod crdt {
        pub type Actor = u8;
        pub trait Resolution {
            fn resolve_fields(&mut self, actor: Actor) -> super::Result<()>;
        }
        pub trait Record: Copy + Default + Eq {
            const KEY: &'static [u8];
            const FORMAT: &'static [u8; 8];
            const SIZE: usize;
            fn encode(self, bytes: &mut [u8]) -> super::Result<()>;
            fn decode(bytes: &[u8]) -> super::Result<Self>;
            fn merge(&mut self, other: Self);
            fn check_peer(self, incoming: Self, actor: Actor) -> super::Result<()>;
            fn resolve_conflicts(&mut self, _: Actor) -> super::Result<()> {
                Ok(())
            }
        }
    }
}
use api::crdt::{Record as _, Resolution as _};
thread_local! { static CALLS: RefCell<Vec<(&'static str,u8)>> = const {RefCell::new(Vec::new())}; }
fn call(op: &'static str, id: u8) {
    CALLS.with(|calls| calls.borrow_mut().push((op, id)));
}
fn calls() -> Vec<(&'static str, u8)> {
    CALLS.with(|calls| std::mem::take(&mut *calls.borrow_mut()))
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct Byte<const ID: u8>(u8);
impl<const ID: u8> api::crdt::Record for Byte<ID> {
    const KEY: &'static [u8] = b"byte";
    const FORMAT: &'static [u8; 8] = b"BYTE0001";
    const SIZE: usize = 1;
    fn encode(self, bytes: &mut [u8]) -> api::Result<()> {
        call("encode", ID);
        if self.0 == 250 {
            return Err(api::Error::Conflict);
        }
        bytes[0] = self.0;
        Ok(())
    }
    fn decode(bytes: &[u8]) -> api::Result<Self> {
        call("decode", ID);
        if bytes[0] == 251 {
            return Err(api::Error::Corrupt);
        }
        Ok(Self(bytes[0]))
    }
    fn merge(&mut self, other: Self) {
        call("merge", ID);
        self.0 = self.0.max(other.0);
    }
    fn check_peer(self, incoming: Self, _: u8) -> api::Result<()> {
        call("check", ID);
        if incoming.0 == 252 {
            Err(api::Error::Conflict)
        } else {
            Ok(())
        }
    }
}
impl<const ID: u8> api::crdt::Resolution for Byte<ID> {
    fn resolve_fields(&mut self, actor: u8) -> api::Result<()> {
        call("resolve", ID);
        if self.0 == 253 {
            return Err(api::Error::Conflict);
        }
        self.0 = actor;
        Ok(())
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Record, Resolution)]
#[record(crate="crate::api", key=b"composite",format=b"ORDER001",fields(second,first,ignored),resolve=AUTO)]
#[resolution(crate = "crate::api")]
struct Composite<const AUTO: bool> {
    #[resolve]
    first: Byte<1>,
    #[resolve]
    second: Byte<2>,
    ignored: Byte<3>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Record)]
#[record(crate = "crate::api", key = b"empty", format = b"EMPTY001", fields())]
struct Empty {}

#[test]
fn emitted_codec_has_explicit_wire_order_and_exact_length_guards() {
    let value = Composite::<true> {
        first: Byte(1),
        second: Byte(2),
        ignored: Byte(3),
    };
    assert_eq!(Composite::<true>::KEY, b"composite");
    assert_eq!(Composite::<true>::FORMAT, b"ORDER001");
    assert_eq!(Composite::<true>::SIZE, 3);
    for size in [0, 2, 4] {
        let mut bytes = vec![99; size];
        assert_eq!(value.encode(&mut bytes), Err(api::Error::Buffer));
        assert_eq!(bytes, vec![99; size]);
        assert_eq!(Composite::<true>::decode(&bytes), Err(api::Error::Corrupt));
        assert!(calls().is_empty());
    }
    let mut bytes = [0; 3];
    value.encode(&mut bytes).unwrap();
    assert_eq!(bytes, [2, 1, 3]);
    assert_eq!(calls(), [("encode", 2), ("encode", 1), ("encode", 3)]);
    assert_eq!(Composite::<true>::decode(&bytes), Ok(value));
    assert_eq!(calls(), [("decode", 2), ("decode", 1), ("decode", 3)]);
    let mut merged = Composite::<true>::default();
    merged.merge(value);
    assert_eq!(merged, value);
    assert_eq!(calls(), [("merge", 2), ("merge", 1), ("merge", 3)]);
    merged.check_peer(value, 0).unwrap();
    assert_eq!(calls(), [("check", 2), ("check", 1), ("check", 3)]);
    Empty {}.encode(&mut []).unwrap();
    assert_eq!(Empty::decode(&[]), Ok(Empty {}));
    assert!(calls().is_empty());
}
#[test]
fn emitted_calls_stop_at_the_first_field_error() {
    let value = Composite::<true> {
        first: Byte(250),
        second: Byte(2),
        ignored: Byte(3),
    };
    let mut bytes = [99; 3];
    assert_eq!(value.encode(&mut bytes), Err(api::Error::Conflict));
    assert_eq!(bytes, [2, 99, 99]);
    assert_eq!(calls(), [("encode", 2), ("encode", 1)]);
    assert_eq!(
        Composite::<true>::decode(&[2, 251, 3]),
        Err(api::Error::Corrupt)
    );
    assert_eq!(calls(), [("decode", 2), ("decode", 1)]);
    let incoming = Composite::<true> {
        first: Byte(252),
        second: Byte(2),
        ignored: Byte(3),
    };
    assert_eq!(value.check_peer(incoming, 0), Err(api::Error::Conflict));
    assert_eq!(calls(), [("check", 2), ("check", 1)]);
    let mut value = Composite::<true> {
        first: Byte(1),
        second: Byte(253),
        ignored: Byte(3),
    };
    assert_eq!(value.resolve_fields(7), Err(api::Error::Conflict));
    assert_eq!(value.first.0, 7);
    assert_eq!(value.ignored.0, 3);
    assert_eq!(calls(), [("resolve", 1), ("resolve", 2)]);
    assert_eq!(value.resolve_conflicts(7), Err(api::Error::Conflict));
    assert_eq!(calls(), [("resolve", 1), ("resolve", 2)]);
}
#[test]
fn automatic_resolution_flag_and_field_selection_are_independent() {
    let mut enabled = Composite::<true>::default();
    enabled.resolve_conflicts(7).unwrap();
    assert_eq!(enabled.first.0, 7);
    assert_eq!(enabled.second.0, 7);
    assert_eq!(enabled.ignored.0, 0);
    assert_eq!(calls(), [("resolve", 1), ("resolve", 2)]);
    let mut disabled = Composite::<false>::default();
    disabled.resolve_conflicts(7).unwrap();
    assert_eq!(disabled, Composite::<false>::default());
    assert!(calls().is_empty());
    Empty {}.resolve_conflicts(7).unwrap();
    assert!(calls().is_empty());
}
