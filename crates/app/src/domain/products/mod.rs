//! Products

pub mod data;
pub mod errors;
pub mod records;
mod repository;
pub mod service;

pub use errors::ProductsServiceError;
pub use service::*;
