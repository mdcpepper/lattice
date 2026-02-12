//! Items

use std::collections::HashSet;

use ext_php_rs::prelude::*;

use crate::{money::MoneyRef, products::ProductRef, reference_value::ReferenceValue};

#[derive(Debug)]
#[php_class]
#[php(name = "FeedCode\\Lattice\\Item")]
pub struct Item {
    #[php(prop)]
    id: ReferenceValue,

    #[php(prop)]
    name: String,

    #[php(prop)]
    price: MoneyRef,

    #[php(prop)]
    product: ProductRef,

    #[php(prop)]
    tags: HashSet<String>,
}

#[php_impl]
impl Item {
    pub fn __construct(
        id: ReferenceValue,
        name: String,
        price: MoneyRef,
        product: ProductRef,
        tags: Option<HashSet<String>>,
    ) -> Self {
        Self {
            id,
            name,
            price,
            product,
            tags: tags.unwrap_or_default(),
        }
    }

    #[php(name = "from_product")]
    pub fn from_product(reference: ReferenceValue, product: ProductRef) -> Self {
        Self {
            id: reference,
            name: product.name(),
            price: product.price(),
            tags: product.tags(),
            product,
        }
    }
}
