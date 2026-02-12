#![cfg_attr(windows, feature(abi_vectorcall))]

use ext_php_rs::prelude::*;

use crate::{
    items::Item,
    money::Money,
    products::Product,
    qualification::{BoolOp, Qualification, Rule, RuleKind},
};

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
}
