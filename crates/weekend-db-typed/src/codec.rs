use crate::{
    Error, Result,
    engine::{Database, Kind, Value},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Invalid(&'static str),
    OutOfMemory,
}
impl From<&'static str> for DecodeError {
    fn from(reason: &'static str) -> Self {
        Self::Invalid(reason)
    }
}
/// Owned Rust field encoding. Custom implementations must keep KIND stable.
pub trait FieldValue: Sized {
    const KIND: Kind;
    const OPTIONAL: bool = false;
    const NULLABLE: bool = false;
    const PRESERVE_MISSING: bool = false;
    fn encode(&self, db: &Database) -> Result<Value>;
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError>;
    fn encode_field(&self, db: &Database) -> Result<Option<Value>> {
        Ok(Some(self.encode(db)?))
    }
    fn missing() -> std::result::Result<Self, DecodeError> {
        Err("missing required field".into())
    }
    fn validate_type() -> Result<()> {
        Ok(())
    }
}
/// Marker for scalar fields eligible for exact unique lookups.
pub trait UniqueValue: FieldValue {}
/// Borrowed exact lookup values; String indexes accept str without allocating
/// a temporary String, and Bytes indexes accept byte slices.
pub trait LookupValue<T: FieldValue> {
    fn encode_lookup(&self, db: &Database) -> Result<Value>;
}
impl<T: FieldValue> LookupValue<T> for T {
    fn encode_lookup(&self, db: &Database) -> Result<Value> {
        self.encode(db)
    }
}
impl LookupValue<String> for str {
    fn encode_lookup(&self, db: &Database) -> Result<Value> {
        Ok(db.bytes(self.as_bytes())?)
    }
}
impl LookupValue<Bytes> for [u8] {
    fn encode_lookup(&self, db: &Database) -> Result<Value> {
        Ok(db.bytes(self)?)
    }
}
macro_rules! scalar {
    ($ty:ty, $kind:ident, $make:ident, $read:ident) => {
        impl FieldValue for $ty {
            const KIND: Kind = Kind::$kind;
            fn encode(&self, db: &Database) -> Result<Value> {
                Ok(db.$make(*self)?)
            }
            fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
                value
                    .$read()
                    .map_err(|_| DecodeError::Invalid("wrong scalar kind"))
            }
        }
        impl UniqueValue for $ty {}
    };
}
scalar!(bool, Boolean, boolean, as_bool);
scalar!(i64, Integer, integer, as_i64);
scalar!(u64, Unsigned, unsigned, as_u64);
scalar!(f64, Float, float, as_f64);
macro_rules! integer {
    ($($ty:ty => $wide:ty),*) => {$ (
        impl FieldValue for $ty {
            const KIND: Kind = <$wide as FieldValue>::KIND;
            fn encode(&self, db: &Database) -> Result<Value> {(*self as $wide).encode(db)}
            fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
                <$wide>::decode(value)?.try_into().map_err(|_| DecodeError::Invalid("integer outside Rust type range"))
            }
        }
        impl UniqueValue for $ty {}
    )*};
}
integer!(i8=>i64, i16=>i64, i32=>i64, u8=>u64, u16=>u64, u32=>u64);
impl FieldValue for String {
    const KIND: Kind = Kind::Bytes;
    fn encode(&self, db: &Database) -> Result<Value> {
        Ok(db.bytes(self.as_bytes())?)
    }
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
        let text = std::str::from_utf8(value.as_bytes().map_err(|_| "expected UTF-8 bytes")?)
            .map_err(|_| "invalid UTF-8")?;
        let mut out = String::new();
        out.try_reserve_exact(text.len())
            .map_err(|_| DecodeError::OutOfMemory)?;
        out.push_str(text);
        Ok(out)
    }
}
impl UniqueValue for String {}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);
impl FieldValue for Bytes {
    const KIND: Kind = Kind::Bytes;
    fn encode(&self, db: &Database) -> Result<Value> {
        Ok(db.bytes(&self.0)?)
    }
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
        let bytes = value.as_bytes().map_err(|_| "expected bytes")?;
        let mut out = Vec::new();
        out.try_reserve_exact(bytes.len())
            .map_err(|_| DecodeError::OutOfMemory)?;
        out.extend_from_slice(bytes);
        Ok(Self(out))
    }
}
impl UniqueValue for Bytes {}
impl<T: FieldValue> FieldValue for Vec<T> {
    const KIND: Kind = Kind::List;
    fn encode(&self, db: &Database) -> Result<Value> {
        let mut list = db.list()?;
        for item in self {
            list = list.list_push(&item.encode(db)?)?;
        }
        Ok(list)
    }
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
        if value.kind() != Kind::List {
            return Err("expected list".into());
        }
        let len = value.len().map_err(|_| "invalid list")?;
        let mut out = Vec::new();
        out.try_reserve_exact(len)
            .map_err(|_| DecodeError::OutOfMemory)?;
        for i in 0..len {
            out.push(T::decode(
                &value.list_get(i).map_err(|_| "invalid list item")?,
            )?);
        }
        Ok(out)
    }
    fn validate_type() -> Result<()> {
        T::validate_type()
    }
}
impl<T: FieldValue> FieldValue for Option<T> {
    const KIND: Kind = T::KIND;
    const OPTIONAL: bool = true;
    const NULLABLE: bool = true;
    fn encode(&self, db: &Database) -> Result<Value> {
        match self {
            Some(v) => v.encode(db),
            None => Ok(db.none()?),
        }
    }
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
        if value.kind() == Kind::None {
            Ok(None)
        } else {
            Ok(Some(T::decode(value)?))
        }
    }
    fn missing() -> std::result::Result<Self, DecodeError> {
        Ok(None)
    }
    fn validate_type() -> Result<()> {
        if T::OPTIONAL || T::NULLABLE {
            return Err(Error::InvalidField {
                id: 0,
                reason: "nested nullable fields are ambiguous",
            });
        }
        T::validate_type()
    }
}
impl<T: UniqueValue> UniqueValue for Option<T> {}
/// Preserve absence vs explicit null when Option's collapse is inappropriate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Presence<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}
impl<T: FieldValue> FieldValue for Presence<T> {
    const KIND: Kind = T::KIND;
    const OPTIONAL: bool = true;
    const NULLABLE: bool = true;
    const PRESERVE_MISSING: bool = true;
    fn encode(&self, db: &Database) -> Result<Value> {
        match self {
            Self::Value(v) => v.encode(db),
            Self::Null => Ok(db.none()?),
            Self::Missing => Err(Error::InvalidField {
                id: 0,
                reason: "missing is not a list value",
            }),
        }
    }
    fn encode_field(&self, db: &Database) -> Result<Option<Value>> {
        match self {
            Self::Missing => Ok(None),
            _ => Ok(Some(self.encode(db)?)),
        }
    }
    fn decode(value: &Value) -> std::result::Result<Self, DecodeError> {
        if value.kind() == Kind::None {
            Ok(Self::Null)
        } else {
            Ok(Self::Value(T::decode(value)?))
        }
    }
    fn missing() -> std::result::Result<Self, DecodeError> {
        Ok(Self::Missing)
    }
    fn validate_type() -> Result<()> {
        <Option<T>>::validate_type()
    }
}
impl<T: UniqueValue> UniqueValue for Presence<T> {}
