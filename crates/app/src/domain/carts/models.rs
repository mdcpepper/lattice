//! Cart Models

use jiff::Timestamp;

use crate::{domain::products::models::ProductUuid, uuids::TypedUuid};

/// Cart UUID
pub type CartUuid = TypedUuid<Cart>;

/// Cart Model
#[derive(Debug, Clone)]
pub struct Cart {
    pub uuid: CartUuid,
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
    pub uuid: CartUuid,
}

/// Cart Item UUID
pub type CartItemUuid = TypedUuid<CartItem>;

/// CartItem Model
#[derive(Debug, Clone)]
pub struct CartItem {
    pub uuid: CartItemUuid,
    pub base_price: u64,
    pub product_uuid: ProductUuid,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

// NewCartItem Model
pub struct NewCartItem {
    pub uuid: CartItemUuid,
    pub product_uuid: ProductUuid,
}
