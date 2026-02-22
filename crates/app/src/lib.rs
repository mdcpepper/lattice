//! Shared application domain and persistence modules.

pub mod auth;
pub mod context;
pub mod database;
pub mod domain;

#[cfg(test)]
mod test;

mod uuids;
