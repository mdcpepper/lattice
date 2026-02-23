//! Cart Models

use jiff::Timestamp;
use uuid::Uuid;

/// Cart Model
#[derive(Debug, Clone)]
pub struct Cart {
    pub uuid: Uuid,
    pub subtotal: u64,
    pub total: u64,
    pub items: Vec<CartItem>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// New Cart Model
#[derive(Debug, Clone, PartialEq)]
pub struct NewCart {
    pub uuid: Uuid,
}

/// CartItem Model
#[derive(Debug, Clone)]
pub struct CartItem {
    pub uuid: Uuid,
    pub base_price: u64,
    pub product_uuid: Uuid,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

// NewCartItem Model
pub struct NewCartItem {
    pub uuid: Uuid,
    pub product_uuid: Uuid,
}
