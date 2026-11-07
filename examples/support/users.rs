//! Handwritten application layer for milestone 8; not engine-enforced schemas.
#![allow(dead_code)]
use weekend_db::{
    CollectionId, Database, Kind, Transaction, TransactionState, Value,
    schema::{Error, FieldSpec, ReadView, Record, Result, Schema},
};
pub const USER_TYPE: u64 = 0x75736572;
pub const NAME: u64 = 1;
pub const EMAIL: u64 = 2;
pub const AGE_YEARS: u64 = 3;
pub const NICKNAME: u64 = 4;
pub const AGE_MONTHS: u64 = 5;

fn spec(id: u64, name: &str, kind: Kind) -> FieldSpec<'_> {
    FieldSpec {
        id,
        name,
        kind,
        required: true,
        nullable: false,
        default: None,
    }
}
pub fn descriptor(db: &Database, version: u64) -> Result<Schema> {
    if !(1..=3).contains(&version) {
        return Err(Error::UnsupportedVersion);
    }
    let name = if version == 1 { "name" } else { "display_name" };
    let fields = [
        spec(NAME, name, Kind::Bytes),
        spec(EMAIL, "email", Kind::Bytes),
        spec(
            if version == 3 { AGE_MONTHS } else { AGE_YEARS },
            if version == 3 {
                "age_months"
            } else {
                "age_years"
            },
            Kind::Unsigned,
        ),
        FieldSpec {
            id: NICKNAME,
            name: "nickname",
            kind: Kind::Bytes,
            required: false,
            nullable: true,
            default: if version >= 2 {
                Some(db.bytes(b"guest")?)
            } else {
                None
            },
        },
    ];
    match version {
        1 => Schema::build(db, USER_TYPE, 1, &fields[..3], &[]),
        2 => Schema::build(db, USER_TYPE, 2, &fields, &[]),
        3 => Schema::build(db, USER_TYPE, 3, &fields, &[AGE_YEARS]),
        _ => Err(Error::UnsupportedVersion),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Age {
    Years(u64),
    Months(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nickname<'a> {
    Missing,
    None,
    Text(&'a str),
}
pub enum NicknameUpdate<'a> {
    Missing,
    None,
    Text(&'a str),
}
pub struct User {
    pub id: u64,
    record: Record,
    schema: Schema,
    name: Value,
    email: Value,
    age: Age,
    nickname: Option<Value>,
    effective_nickname: Option<Value>,
}
fn text(value: &Value) -> Result<&str> {
    std::str::from_utf8(value.as_bytes()?).map_err(|_| Error::InvalidValue)
}
impl User {
    fn decode(id: u64, record: Record, schema: Schema) -> Result<Self> {
        if id == 0 || record.type_id() != USER_TYPE || !(1..=3).contains(&record.version()) {
            return Err(Error::UnsupportedVersion);
        }
        schema.validate(&record)?;
        // These stable IDs/kinds are the handwritten wrapper's compatibility contract.
        for (id, kind) in [
            (NAME, Kind::Bytes),
            (EMAIL, Kind::Bytes),
            (
                if record.version() == 3 {
                    AGE_MONTHS
                } else {
                    AGE_YEARS
                },
                Kind::Unsigned,
            ),
        ] {
            if schema
                .field(id)?
                .is_none_or(|field| field.kind != kind || field.nullable || !field.required)
            {
                return Err(Error::Incompatible);
            }
        }
        let name = schema
            .read_field(&record, NAME)?
            .ok_or(Error::MissingField(NAME))?;
        let email = schema
            .read_field(&record, EMAIL)?
            .ok_or(Error::MissingField(EMAIL))?;
        if text(&name)?.is_empty() || text(&email)?.is_empty() {
            return Err(Error::InvalidValue);
        }
        let age = if record.version() == 3 {
            Age::Months(
                schema
                    .read_field(&record, AGE_MONTHS)?
                    .ok_or(Error::MissingField(AGE_MONTHS))?
                    .as_u64()?,
            )
        } else {
            Age::Years(
                schema
                    .read_field(&record, AGE_YEARS)?
                    .ok_or(Error::MissingField(AGE_YEARS))?
                    .as_u64()?,
            )
        };
        let (nickname, effective_nickname) = if record.version() >= 2 {
            let field = schema.field(NICKNAME)?.ok_or(Error::Incompatible)?;
            if field.kind != Kind::Bytes || !field.nullable || field.required {
                return Err(Error::Incompatible);
            }
            (
                record.field(NICKNAME)?,
                schema.read_field(&record, NICKNAME)?,
            )
        } else {
            (None, None)
        };
        for value in [&nickname, &effective_nickname].into_iter().flatten() {
            if value.kind() != Kind::None {
                text(value)?;
            }
        }
        Ok(Self {
            id,
            record,
            schema,
            name,
            email,
            age,
            nickname,
            effective_nickname,
        })
    }
    pub fn version(&self) -> u64 {
        self.record.version()
    }
    pub fn display_name(&self) -> &str {
        text(&self.name).expect("validated UTF-8")
    }
    pub fn email(&self) -> &str {
        text(&self.email).expect("validated UTF-8")
    }
    pub fn age(&self) -> Age {
        self.age
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn record(&self) -> &Record {
        &self.record
    }
    pub fn nickname(&self) -> Nickname<'_> {
        match &self.nickname {
            None => Nickname::Missing,
            Some(v) if v.kind() == Kind::None => Nickname::None,
            Some(v) => Nickname::Text(text(v).expect("validated UTF-8")),
        }
    }
    pub fn effective_nickname(&self) -> Option<&str> {
        self.effective_nickname
            .as_ref()
            .filter(|v| v.kind() != Kind::None)
            .map(|v| text(v).expect("validated UTF-8"))
    }
}

#[derive(Clone, Copy)]
pub struct Users {
    pub schemas: CollectionId,
    pub records: CollectionId,
    pub emails: CollectionId,
}
fn key(id: u64) -> Result<[u8; 8]> {
    if id == 0 {
        Err(Error::InvalidValue)
    } else {
        Ok(id.to_le_bytes())
    }
}
/// Fail closed even if one of several overlay assignments already succeeded.
/// The caller's whole draft is aborted on any write-method error.
fn write<T>(
    tx: &mut Transaction<'_>,
    action: impl FnOnce(&mut Transaction<'_>) -> Result<T>,
) -> Result<T> {
    let result = action(tx);
    if result.is_err() && tx.state() == TransactionState::Active {
        let _ = tx.abort();
    }
    result
}
impl Users {
    /// Collection creation is immediate. Descriptor/data setup is staged explicitly
    /// by install_v1/insert in a transaction. Persist the numeric IDs in the app.
    pub fn create(db: &Database) -> Result<Self> {
        Ok(Self {
            schemas: db.create_collection()?,
            records: db.create_collection()?,
            emails: db.create_collection()?,
        })
    }
    pub fn ids(&self) -> [u64; 3] {
        [
            self.schemas.as_u64(),
            self.records.as_u64(),
            self.emails.as_u64(),
        ]
    }
    pub fn from_ids(db: &Database, ids: [u64; 3]) -> Self {
        Self {
            schemas: db.collection_id(ids[0]),
            records: db.collection_id(ids[1]),
            emails: db.collection_id(ids[2]),
        }
    }
    pub fn install_v1(&self, db: &Database, tx: &mut Transaction<'_>) -> Result<()> {
        write(tx, |tx| descriptor(db, 1)?.publish(tx, self.schemas))
    }
    pub fn load(&self, view: &mut impl ReadView, id: u64) -> Result<Option<User>> {
        let Some(raw) = view.get_record(self.records, &key(id)?)? else {
            return Ok(None);
        };
        let record = Record::from_value(raw)?;
        if record.type_id() != USER_TYPE || !(1..=3).contains(&record.version()) {
            return Err(Error::UnsupportedVersion);
        }
        let schema = Schema::load(view, self.schemas, USER_TYPE, record.version())?;
        Ok(Some(User::decode(id, record, schema)?))
    }
    pub fn find_by_email(&self, view: &mut impl ReadView, email: &str) -> Result<Option<User>> {
        let Some(target) = view.get_record(self.emails, email.as_bytes())? else {
            return Ok(None);
        };
        let id = target.as_u64().map_err(|_| Error::IndexInconsistent)?;
        if id == 0 {
            return Err(Error::IndexInconsistent);
        }
        let user = self.load(view, id)?.ok_or(Error::IndexInconsistent)?;
        if user.email() != email {
            return Err(Error::IndexInconsistent);
        }
        Ok(Some(user))
    }
    fn check_index(&self, tx: &mut Transaction<'_>, user: &User) -> Result<()> {
        let id = tx
            .get(self.emails, user.email().as_bytes())?
            .ok_or(Error::IndexInconsistent)?;
        if id.as_u64().map_err(|_| Error::IndexInconsistent)? != user.id {
            return Err(Error::IndexInconsistent);
        }
        Ok(())
    }
    pub fn insert_v1(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        name: &str,
        email: &str,
        age_years: u64,
    ) -> Result<()> {
        write(tx, |tx| {
            let key = key(id)?;
            if name.is_empty() || email.is_empty() {
                return Err(Error::InvalidValue);
            }
            if tx.get(self.records, &key)?.is_some() {
                return Err(Error::Engine(weekend_db::Error::AlreadyExists));
            }
            if tx.get(self.emails, email.as_bytes())?.is_some() {
                return Err(Error::UniqueViolation);
            }
            let schema = Schema::load(tx, self.schemas, USER_TYPE, 1)?;
            let record = Record::new(db, &schema)?
                .set(NAME, &db.bytes(name.as_bytes())?)?
                .set(EMAIL, &db.bytes(email.as_bytes())?)?
                .set(AGE_YEARS, &db.unsigned(age_years)?)?;
            User::decode(id, record.clone(), schema)?;
            let target = db.unsigned(id)?;
            tx.put(self.records, &key, record.as_value())?;
            tx.put(self.emails, email.as_bytes(), &target)?;
            Ok(())
        })
    }
    pub fn set_display_name(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        name: &str,
    ) -> Result<()> {
        self.rename(db, tx, id, name, false)
    }
    /// An older wrapper understands stable IDs 1–3 on v1/v2. It preserves v2's
    /// nickname and every other unknown field; v3 requires a newer age contract.
    pub fn set_name_v1(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        name: &str,
    ) -> Result<()> {
        self.rename(db, tx, id, name, true)
    }
    fn rename(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        name: &str,
        legacy: bool,
    ) -> Result<()> {
        write(tx, |tx| {
            let user = self
                .load(tx, id)?
                .ok_or(Error::Engine(weekend_db::Error::NotFound))?;
            if legacy && user.version() > 2 {
                return Err(Error::UnsupportedVersion);
            }
            if name.is_empty() {
                return Err(Error::InvalidValue);
            }
            let record = user.record.set(NAME, &db.bytes(name.as_bytes())?)?;
            user.schema.validate(&record)?;
            tx.put(self.records, &key(id)?, record.as_value())?;
            Ok(())
        })
    }
    pub fn set_nickname(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        nickname: NicknameUpdate<'_>,
    ) -> Result<()> {
        write(tx, |tx| {
            let user = self
                .load(tx, id)?
                .ok_or(Error::Engine(weekend_db::Error::NotFound))?;
            if user.version() < 2 {
                return Err(Error::UnsupportedVersion);
            }
            let record = match nickname {
                NicknameUpdate::Missing => user.record.remove(NICKNAME)?,
                NicknameUpdate::None => user.record.set(NICKNAME, &db.none()?)?,
                NicknameUpdate::Text(text) => {
                    user.record.set(NICKNAME, &db.bytes(text.as_bytes())?)?
                }
            };
            user.schema.validate(&record)?;
            tx.put(self.records, &key(id)?, record.as_value())?;
            Ok(())
        })
    }
    pub fn set_email(
        &self,
        db: &Database,
        tx: &mut Transaction<'_>,
        id: u64,
        email: &str,
    ) -> Result<()> {
        write(tx, |tx| {
            if email.is_empty() {
                return Err(Error::InvalidValue);
            }
            let user = self
                .load(tx, id)?
                .ok_or(Error::Engine(weekend_db::Error::NotFound))?;
            self.check_index(tx, &user)?;
            if let Some(target) = tx.get(self.emails, email.as_bytes())?
                && target.as_u64().map_err(|_| Error::IndexInconsistent)? != id
            {
                return Err(Error::UniqueViolation);
            }
            let record = user.record.set(EMAIL, &db.bytes(email.as_bytes())?)?;
            user.schema.validate(&record)?;
            let target = db.unsigned(id)?;
            if user.email() != email {
                tx.delete(self.emails, user.email().as_bytes())?;
            }
            tx.put(self.emails, email.as_bytes(), &target)?;
            tx.put(self.records, &key(id)?, record.as_value())?;
            Ok(())
        })
    }
    pub fn delete(&self, tx: &mut Transaction<'_>, id: u64) -> Result<()> {
        write(tx, |tx| {
            let user = self
                .load(tx, id)?
                .ok_or(Error::Engine(weekend_db::Error::NotFound))?;
            self.check_index(tx, &user)?;
            tx.delete(self.emails, user.email().as_bytes())?;
            tx.delete(self.records, &key(id)?)?;
            Ok(())
        })
    }
    pub fn migrate_to_v2(&self, db: &Database, tx: &mut Transaction<'_>) -> Result<usize> {
        self.migrate(db, tx, 2)
    }
    pub fn migrate_to_v3(&self, db: &Database, tx: &mut Transaction<'_>) -> Result<usize> {
        self.migrate(db, tx, 3)
    }
    fn migrate(&self, db: &Database, tx: &mut Transaction<'_>, target: u64) -> Result<usize> {
        write(tx, |tx| {
            let target_schema = descriptor(db, target)?;
            target_schema.publish(tx, self.schemas)?;
            let rows = tx.scan(self.records)?;
            let mut changed = 0;
            for (raw_id, _) in rows {
                let id = u64::from_le_bytes(
                    raw_id
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::InvalidRecord)?,
                );
                let user = self.load(tx, id)?.ok_or(Error::InvalidRecord)?;
                self.check_index(tx, &user)?;
                if user.version() > target {
                    return Err(Error::UnsupportedVersion);
                }
                if user.version() == target {
                    continue;
                }
                let mut record = user.record.clone();
                if target == 3 {
                    if record.field(AGE_MONTHS)?.is_some() {
                        return Err(Error::MigrationCollision);
                    }
                    let Age::Years(years) = user.age() else {
                        return Err(Error::UnsupportedVersion);
                    };
                    let months = years.checked_mul(12).ok_or(Error::MigrationOverflow)?;
                    record = record
                        .remove(AGE_YEARS)?
                        .set(AGE_MONTHS, &db.unsigned(months)?)?;
                }
                record = record.with_schema(db, &target_schema)?;
                User::decode(id, record.clone(), target_schema.clone())?;
                tx.put(self.records, &key(id)?, record.as_value())?;
                changed += 1;
            }
            Ok(changed)
        })
    }
}
