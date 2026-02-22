//! Cart Models

use jiff::Timestamp;
use uuid::Uuid;

/// Cart Model
#[derive(Debug, Clone)]
pub struct Cart {
    pub uuid: Uuid,
    pub subtotal: u64,
    pub total: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// New Cart Model
#[derive(Debug, Clone, PartialEq)]
pub struct NewCart {
    pub uuid: Uuid,
}
