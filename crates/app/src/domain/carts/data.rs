//! Cart Data

use crate::domain::{
    carts::records::{CartItemUuid, CartUuid},
    products::records::ProductUuid,
};

/// New Cart Data
#[derive(Debug, Clone, PartialEq)]
pub struct NewCart {
    pub uuid: CartUuid,
}

/// New Cart Item Data
pub struct NewCartItem {
    pub uuid: CartItemUuid,
    pub product_uuid: ProductUuid,
}
