//! Product Models

use jiff::Timestamp;
use uuid::Uuid;

/// Product Model
#[derive(Debug, Clone)]
pub struct Product {
    pub uuid: Uuid,
    pub price: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// New Product Model
#[derive(Debug, Clone, PartialEq)]
pub struct NewProduct {
    pub uuid: Uuid,
    pub price: u64,
}

/// Product Update Model
#[derive(Debug, Clone, PartialEq)]
pub struct ProductUpdate {
    pub price: u64,
}
