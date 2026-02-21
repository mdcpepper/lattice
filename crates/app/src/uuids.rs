//! Typed Uuids

use std::{
    cmp::Ordering,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use uuid::Uuid;

pub struct TypedUuid<T>(Uuid, PhantomData<T>);

impl<T> TypedUuid<T> {
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid, PhantomData)
    }

    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl<T> Clone for TypedUuid<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TypedUuid<T> {}

impl<T> Debug for TypedUuid<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self.0, f)
    }
}

impl<T> Display for TypedUuid<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self.0, f)
    }
}

impl<T> PartialEq for TypedUuid<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for TypedUuid<T> {}

impl<T> Hash for TypedUuid<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T> PartialOrd for TypedUuid<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for TypedUuid<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T> From<Uuid> for TypedUuid<T> {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl<T> From<TypedUuid<T>> for Uuid {
    fn from(value: TypedUuid<T>) -> Self {
        value.into_uuid()
    }
}
