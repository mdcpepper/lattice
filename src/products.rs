//! Products

use rusty_money::{Money, iso::Currency};
use slotmap::new_key_type;

use crate::tags::{collection::TagCollection, string::StringTagCollection};

new_key_type! {
    /// Product Key
    pub struct ProductKey;
}

/// Product
#[derive(Debug, Clone)]
pub struct Product<'a, T: TagCollection = StringTagCollection> {
    /// Product name
    pub name: String,

    /// Product tags
    pub tags: T,

    /// Product price
    pub price: Money<'a, Currency>,
}
