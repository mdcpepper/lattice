//! Tenants

pub mod data;
pub mod errors;
pub mod records;
mod repository;
pub mod service;

pub use errors::TenantsServiceError;
pub use service::*;
