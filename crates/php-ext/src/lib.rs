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
    promotions::{
        budgets::Budget, direct_discount::DirectDiscountPromotion, interface::PhpInterfacePromotion,
    },
    qualification::{BoolOp, Qualification, Rule, RuleKind},
    receipt::{Receipt, applications::PromotionApplication},
    stack::{
        InvalidStackException, Stack, StackBuilder,
        layers::{Layer, LayerOutput},
    },
};

pub mod discounts;
pub mod items;
pub mod money;
pub mod products;
pub mod promotions;
pub mod qualification;
pub mod receipt;
pub mod reference_value;
pub mod stack;

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
        .class::<Budget>()
        .interface::<PhpInterfacePromotion>()
        .class::<DirectDiscountPromotion>()
        .enumeration::<LayerOutput>()
        .class::<InvalidStackException>()
        .class::<Layer>()
        .class::<StackBuilder>()
        .class::<Stack>()
        .class::<PromotionApplication>()
        .class::<Receipt>()
}
