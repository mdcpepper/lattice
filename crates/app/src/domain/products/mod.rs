//! Products

pub mod data;
mod errors;
pub mod records;
mod repository;
mod service;

pub use errors::ProductsServiceError;
pub use service::*;
