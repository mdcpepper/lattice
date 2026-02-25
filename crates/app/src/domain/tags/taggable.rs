//! Taggable

use crate::uuids::TypedUuid;

pub(crate) trait Taggable {
    fn type_as_str() -> &'static str;
}

impl<T> Taggable for TypedUuid<T>
where
    T: Taggable,
{
    fn type_as_str() -> &'static str {
        T::type_as_str()
    }
}
