//! Products Data

use crate::domain::products::records::{ProductDetailsUuid, ProductUuid};

/// New Product Data
#[derive(Debug, Clone, PartialEq)]
pub struct NewProduct {
    pub uuid: ProductUuid,
    pub price: u64,
}

/// Product Update Data
#[derive(Debug, Clone, PartialEq)]
pub struct ProductUpdate {
    pub uuid: Option<ProductDetailsUuid>,
    pub price: u64,
}
