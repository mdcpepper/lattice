//! Tags

pub(crate) mod records;
mod repository;
mod taggable;

pub(crate) use repository::PgTagsRepository;
pub(crate) use taggable::Taggable;
