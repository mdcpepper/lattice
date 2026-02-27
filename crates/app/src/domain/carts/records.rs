//! Cart Records

use jiff::Timestamp;

use crate::{domain::products::records::ProductUuid, uuids::TypedUuid};

/// Cart UUID
pub type CartUuid = TypedUuid<CartRecord>;

/// Cart Record
#[derive(Debug, Clone)]
pub struct CartRecord {
    pub uuid: CartUuid,
    pub subtotal: u64,
    pub total: u64,
    pub items: Vec<CartItemRecord>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// Cart Item UUID
pub type CartItemUuid = TypedUuid<CartItemRecord>;

/// CartItem Record
#[derive(Debug, Clone)]
pub struct CartItemRecord {
    pub uuid: CartItemUuid,
    pub price: u64,
    pub product_uuid: ProductUuid,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}
