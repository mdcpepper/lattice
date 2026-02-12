#![cfg_attr(windows, feature(abi_vectorcall))]

use ext_php_rs::prelude::*;

use crate::{
    discounts::{
        DiscountKind, InvalidDiscountException, SimpleDiscount,
        percentages::{InvalidPercentageException, Percentage, PercentageOutOfRangeException},
    },
    items::Item,
    money::Money,
    products::Product,
    qualification::{BoolOp, Qualification, Rule, RuleKind},
};

pub mod discounts;
pub mod items;
pub mod money;
pub mod products;
pub mod qualification;
pub mod reference_value;

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    module
        .class::<Money>()
        .class::<Product>()
        .class::<Item>()
        .enumeration::<BoolOp>()
        .enumeration::<RuleKind>()
        .class::<Qualification>()
        .class::<Rule>()
        .class::<InvalidPercentageException>()
        .class::<PercentageOutOfRangeException>()
        .class::<InvalidDiscountException>()
        .class::<Percentage>()
        .enumeration::<DiscountKind>()
        .class::<SimpleDiscount>()
}
