//! Receipt and promotion application results

use ext_php_rs::prelude::*;

use crate::{items::ItemRef, money::MoneyRef, receipt::applications::PromotionApplicationRef};

pub mod applications;

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Receipt")]
pub struct Receipt {
    #[php(prop)]
    subtotal: MoneyRef,

    #[php(prop)]
    total: MoneyRef,

    #[php(prop)]
    full_price_items: Vec<ItemRef>,

    #[php(prop)]
    promotion_applications: Vec<PromotionApplicationRef>,
}

#[php_impl]
impl Receipt {
    pub fn __construct(
        subtotal: MoneyRef,
        total: MoneyRef,
        full_price_items: Vec<ItemRef>,
        promotion_applications: Vec<PromotionApplicationRef>,
    ) -> Self {
        Self {
            subtotal,
            total,
            full_price_items,
            promotion_applications,
        }
    }
}
