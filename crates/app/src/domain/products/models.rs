//! Product Models

use jiff::Timestamp;

use crate::uuids::TypedUuid;

/// Product UUID
pub type ProductUuid = TypedUuid<Product>;

/// Product Model
#[derive(Debug, Clone)]
pub struct Product {
    pub uuid: ProductUuid,
    pub price: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// New Product Model
#[derive(Debug, Clone, PartialEq)]
pub struct NewProduct {
    pub uuid: ProductUuid,
    pub price: u64,
}

/// Product Update Model
#[derive(Debug, Clone, PartialEq)]
pub struct ProductUpdate {
    pub uuid: Option<ProductDetailsUuid>,
    pub price: u64,
}

/// Product Detail
pub struct ProductDetails;

/// Product Detail UUID
pub type ProductDetailsUuid = TypedUuid<ProductDetails>;
