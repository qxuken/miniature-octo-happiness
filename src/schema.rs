//! Optional validated access conventions over ordinary records. Raw engine
//! writes can bypass schema immutability and validation; no storage format or
//! unconditional engine constraint is introduced by this module.
use crate::{CollectionId, Database, Kind, Snapshot, Transaction, TransactionState, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Engine(crate::Error),
    InvalidDescriptor,
    InvalidRecord,
    ImmutableDescriptor,
    Incompatible,
    UnsupportedVersion,
    MissingField(u64),
    WrongKind(u64),
    UnexpectedNone(u64),
    ReservedField(u64),
    UnknownField(u64),
    InvalidValue,
    UniqueViolation,
    IndexInconsistent,
    MigrationOverflow,
    MigrationCollision,
}
impl From<crate::Error> for Error {
    fn from(value: crate::Error) -> Self {
        Self::Engine(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Transaction implementations collect dependencies; snapshot implementations
/// are historical, untracked reads. A snapshot read is not a transaction read.
pub trait ReadView {
    fn get_record(&mut self, collection: CollectionId, key: &[u8]) -> crate::Result<Option<Value>>;
}
impl ReadView for Transaction<'_> {
    fn get_record(&mut self, collection: CollectionId, key: &[u8]) -> crate::Result<Option<Value>> {
        self.get(collection, key)
    }
}
impl ReadView for Snapshot<'_> {
    fn get_record(&mut self, collection: CollectionId, key: &[u8]) -> crate::Result<Option<Value>> {
        self.get(collection, key)
    }
}
pub fn descriptor_key(type_id: u64, version: u64) -> [u8; 16] {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&type_id.to_le_bytes());
    key[8..].copy_from_slice(&version.to_le_bytes());
    key
}
pub fn field_key(id: u64) -> [u8; 8] {
    id.to_le_bytes()
}
fn required(value: &Value, key: &[u8]) -> Result<Value> {
    value.map_get(key)?.ok_or(Error::InvalidDescriptor)
}
fn unsigned(value: &Value, key: &[u8]) -> Result<u64> {
    required(value, key)?
        .as_u64()
        .map_err(|_| Error::InvalidDescriptor)
}
fn field_id(bytes: &[u8]) -> Result<u64> {
    let id = u64::from_le_bytes(bytes.try_into().map_err(|_| Error::InvalidRecord)?);
    if id == 0 {
        Err(Error::InvalidRecord)
    } else {
        Ok(id)
    }
}
#[derive(Clone)]
pub struct FieldSpec<'a> {
    pub id: u64,
    pub name: &'a str,
    pub kind: Kind,
    pub required: bool,
    pub nullable: bool,
    pub default: Option<Value>,
}
#[derive(Clone)]
pub struct Field {
    name: Value,
    pub kind: Kind,
    pub required: bool,
    pub nullable: bool,
    pub default: Option<Value>,
}
impl Field {
    pub fn name(&self) -> &str {
        std::str::from_utf8(self.name.as_bytes().expect("validated name")).expect("validated UTF-8")
    }
    fn parse(value: Value) -> Result<Self> {
        let name = required(&value, b"name")?;
        let bytes = name.as_bytes().map_err(|_| Error::InvalidDescriptor)?;
        if bytes.is_empty() || std::str::from_utf8(bytes).is_err() {
            return Err(Error::InvalidDescriptor);
        }
        let kind = match unsigned(&value, b"kind")? {
            1 => Kind::Boolean,
            2 => Kind::Integer,
            3 => Kind::Unsigned,
            4 => Kind::Float,
            5 => Kind::Bytes,
            6 => Kind::Map,
            7 => Kind::List,
            _ => return Err(Error::InvalidDescriptor),
        };
        let required = required(&value, b"required")?
            .as_bool()
            .map_err(|_| Error::InvalidDescriptor)?;
        let nullable = required_value_bool(&value, b"nullable")?;
        let default = value.map_get(b"default")?;
        let field = Self {
            name,
            kind,
            required,
            nullable,
            default,
        };
        if let Some(default) = &field.default {
            field
                .check(0, default)
                .map_err(|_| Error::InvalidDescriptor)?;
        }
        Ok(field)
    }
    fn check(&self, id: u64, value: &Value) -> Result<()> {
        if value.kind() == Kind::None {
            if self.nullable {
                Ok(())
            } else {
                Err(Error::UnexpectedNone(id))
            }
        } else if value.kind() == self.kind {
            Ok(())
        } else {
            Err(Error::WrongKind(id))
        }
    }
}
fn required_value_bool(value: &Value, key: &[u8]) -> Result<bool> {
    required(value, key)?
        .as_bool()
        .map_err(|_| Error::InvalidDescriptor)
}

/// Immutable descriptor convention. Names are labels; stable IDs identify fields.
#[derive(Clone)]
pub struct Schema {
    raw: Value,
    type_id: u64,
    version: u64,
}
impl Schema {
    pub fn build(
        db: &Database,
        type_id: u64,
        version: u64,
        fields: &[FieldSpec<'_>],
        reserved: &[u64],
    ) -> Result<Self> {
        if type_id == 0 || version == 0 {
            return Err(Error::InvalidDescriptor);
        }
        let mut map = db.map()?;
        for spec in fields {
            if spec.id == 0 || map.map_get(&field_key(spec.id))?.is_some() {
                return Err(Error::InvalidDescriptor);
            }
            let mut field = db
                .map()?
                .map_set(b"name", &db.bytes(spec.name.as_bytes())?)?
                .map_set(b"kind", &db.unsigned(spec.kind as u64)?)?
                .map_set(b"required", &db.boolean(spec.required)?)?
                .map_set(b"nullable", &db.boolean(spec.nullable)?)?;
            if let Some(default) = &spec.default {
                field = field.map_set(b"default", default)?;
            }
            map = map.map_set(&field_key(spec.id), &field)?;
        }
        let mut retired = db.list()?;
        for id in reserved {
            retired = retired.list_push(&db.unsigned(*id)?)?;
        }
        Self::from_value(
            db.map()?
                .map_set(b"type", &db.unsigned(type_id)?)?
                .map_set(b"version", &db.unsigned(version)?)?
                .map_set(b"fields", &map)?
                .map_set(b"reserved", &retired)?,
        )
    }
    pub fn from_value(raw: Value) -> Result<Self> {
        let type_id = unsigned(&raw, b"type")?;
        let version = unsigned(&raw, b"version")?;
        if type_id == 0 || version == 0 {
            return Err(Error::InvalidDescriptor);
        }
        let fields = required(&raw, b"fields")?;
        if fields.kind() != Kind::Map {
            return Err(Error::InvalidDescriptor);
        }
        let entries = fields.map_entries()?;
        let reserved = required(&raw, b"reserved")?;
        if reserved.kind() != Kind::List {
            return Err(Error::InvalidDescriptor);
        }
        for i in 0..reserved.len()? {
            let id = reserved
                .list_get(i)?
                .as_u64()
                .map_err(|_| Error::InvalidDescriptor)?;
            if id == 0 || fields.map_get(&field_key(id))?.is_some() {
                return Err(Error::InvalidDescriptor);
            }
            for j in 0..i {
                if reserved.list_get(j)?.as_u64()? == id {
                    return Err(Error::InvalidDescriptor);
                }
            }
        }
        for (i, (key, value)) in entries.iter().enumerate() {
            field_id(key).map_err(|_| Error::InvalidDescriptor)?;
            let field = Field::parse(value.clone())?;
            for (_, previous) in &entries[..i] {
                if Field::parse(previous.clone())?.name() == field.name() {
                    return Err(Error::InvalidDescriptor);
                }
            }
        }
        Ok(Self {
            raw,
            type_id,
            version,
        })
    }
    pub fn type_id(&self) -> u64 {
        self.type_id
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn as_value(&self) -> &Value {
        &self.raw
    }
    pub fn field(&self, id: u64) -> Result<Option<Field>> {
        required(&self.raw, b"fields")?
            .map_get(&field_key(id))?
            .map(Field::parse)
            .transpose()
    }
    fn reserved(&self, id: u64) -> Result<bool> {
        let ids = required(&self.raw, b"reserved")?;
        for i in 0..ids.len()? {
            if ids.list_get(i)?.as_u64()? == id {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn successor_of(&self, old: &Schema) -> Result<()> {
        if self.type_id != old.type_id || old.version.checked_add(1) != Some(self.version) {
            return Err(Error::Incompatible);
        }
        for (key, old_value) in required(&old.raw, b"fields")?.map_entries()? {
            let id = field_id(&key)?;
            match self.field(id)? {
                Some(new) if new.kind == Field::parse(old_value)?.kind => {}
                None if self.reserved(id)? => {}
                _ => return Err(Error::Incompatible),
            }
        }
        let reserved = required(&old.raw, b"reserved")?;
        for i in 0..reserved.len()? {
            if !self.reserved(reserved.list_get(i)?.as_u64()?)? {
                return Err(Error::Incompatible);
            }
        }
        Ok(())
    }
    pub fn load(
        view: &mut impl ReadView,
        collection: CollectionId,
        type_id: u64,
        version: u64,
    ) -> Result<Self> {
        let value = view
            .get_record(collection, &descriptor_key(type_id, version))?
            .ok_or(Error::Engine(crate::Error::NotFound))?;
        let schema = Self::from_value(value)?;
        if schema.type_id != type_id || schema.version != version {
            return Err(Error::InvalidDescriptor);
        }
        Ok(schema)
    }
    /// Publish once, or accept an identical existing descriptor. Errors abort the
    /// draft so a caller cannot commit preceding writes after failed validation.
    pub fn publish(&self, tx: &mut Transaction<'_>, collection: CollectionId) -> Result<()> {
        let result = (|| {
            let key = descriptor_key(self.type_id, self.version);
            if let Some(existing) = tx.get(collection, &key)? {
                return if existing == self.raw {
                    Ok(())
                } else {
                    Err(Error::ImmutableDescriptor)
                };
            }
            if self.version > 1 {
                let previous = Self::load(tx, collection, self.type_id, self.version - 1)?;
                self.successor_of(&previous)?;
            }
            tx.put(collection, &key, &self.raw)?;
            Ok(())
        })();
        if result.is_err() && tx.state() == TransactionState::Active {
            let _ = tx.abort();
        }
        result
    }
    fn envelope(&self, record: &Record) -> Result<()> {
        if self.type_id != record.type_id || self.version != record.version {
            Err(Error::Incompatible)
        } else {
            Ok(())
        }
    }
    pub fn read_field(&self, record: &Record, id: u64) -> Result<Option<Value>> {
        self.envelope(record)?;
        let field = self.field(id)?.ok_or(Error::UnknownField(id))?;
        if let Some(value) = record.field(id)? {
            field.check(id, &value)?;
            return Ok(Some(value));
        }
        if let Some(default) = field.default {
            return Ok(Some(default));
        }
        if field.required {
            Err(Error::MissingField(id))
        } else {
            Ok(None)
        }
    }
    pub fn validate(&self, record: &Record) -> Result<()> {
        self.envelope(record)?;
        for (key, _) in required(&self.raw, b"fields")?.map_entries()? {
            self.read_field(record, field_id(&key)?)?;
        }
        for (key, _) in record.fields()?.map_entries()? {
            let id = field_id(&key)?;
            if self.reserved(id)? {
                return Err(Error::ReservedField(id));
            }
        }
        Ok(())
    }
}

/// Owns the full envelope and unknown fields. Setters patch persistent maps and
/// preserve every key they do not explicitly replace/remove.
#[derive(Clone)]
pub struct Record {
    raw: Value,
    type_id: u64,
    version: u64,
}
impl Record {
    pub fn new(db: &Database, schema: &Schema) -> Result<Self> {
        Self::from_value(
            db.map()?
                .map_set(b"type", &db.unsigned(schema.type_id)?)?
                .map_set(b"version", &db.unsigned(schema.version)?)?
                .map_set(b"fields", &db.map()?)?,
        )
    }
    pub fn from_value(raw: Value) -> Result<Self> {
        let type_id = unsigned(&raw, b"type").map_err(|_| Error::InvalidRecord)?;
        let version = unsigned(&raw, b"version").map_err(|_| Error::InvalidRecord)?;
        let fields = raw.map_get(b"fields")?.ok_or(Error::InvalidRecord)?;
        if type_id == 0 || version == 0 || fields.kind() != Kind::Map {
            return Err(Error::InvalidRecord);
        }
        for (key, _) in fields.map_entries()? {
            field_id(&key)?;
        }
        Ok(Self {
            raw,
            type_id,
            version,
        })
    }
    pub fn type_id(&self) -> u64 {
        self.type_id
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn as_value(&self) -> &Value {
        &self.raw
    }
    fn fields(&self) -> Result<Value> {
        self.raw.map_get(b"fields")?.ok_or(Error::InvalidRecord)
    }
    pub fn field(&self, id: u64) -> Result<Option<Value>> {
        if id == 0 {
            return Err(Error::InvalidRecord);
        }
        Ok(self.fields()?.map_get(&field_key(id))?)
    }
    pub fn set(&self, id: u64, value: &Value) -> Result<Self> {
        if id == 0 {
            return Err(Error::InvalidRecord);
        }
        Self::from_value(
            self.raw
                .map_set(b"fields", &self.fields()?.map_set(&field_key(id), value)?)?,
        )
    }
    pub fn remove(&self, id: u64) -> Result<Self> {
        if id == 0 {
            return Err(Error::InvalidRecord);
        }
        Self::from_value(
            self.raw
                .map_set(b"fields", &self.fields()?.map_remove(&field_key(id))?)?,
        )
    }
    pub fn with_schema(&self, db: &Database, schema: &Schema) -> Result<Self> {
        if self.type_id != schema.type_id {
            return Err(Error::Incompatible);
        }
        Self::from_value(
            self.raw
                .map_set(b"version", &db.unsigned(schema.version)?)?,
        )
    }
}
