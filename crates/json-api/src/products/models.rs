//! Product Models

use jiff::Timestamp;
use uuid::Uuid;

/// New Product Model
#[derive(Debug, Clone)]
pub(crate) struct NewProduct {
    pub uuid: Uuid,
    pub price: u64,
}

/// Product Model
#[derive(Debug, Clone)]
pub(crate) struct Product {
    pub uuid: Uuid,
    pub price: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}
