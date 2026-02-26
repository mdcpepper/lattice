//! Product Records

use jiff::Timestamp;

use crate::{domain::tags::Taggable, uuids::TypedUuid};

/// Product UUID
pub type ProductUuid = TypedUuid<ProductRecord>;

/// Product Record
#[derive(Debug, Clone)]
pub struct ProductRecord {
    pub uuid: ProductUuid,
    pub price: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// Product Details Record
pub struct ProductDetailsRecord;

/// Product Detail UUID
pub type ProductDetailsUuid = TypedUuid<ProductDetailsRecord>;

impl Taggable for ProductRecord {
    fn type_as_str() -> &'static str {
        "product"
    }
}
