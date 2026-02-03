//! Simple Discounts Example
//!
//! This example demonstrates simple percentage discounts applied to items.
//! Two promotions are configured, one that applies a 20% discount to items
//! tagged "20-off", and one that applies a 40% discount to items tagged
//! "40-off".
//!
//! Run with: `cargo run --example simple_discounts`

use std::time::Instant;

use anyhow::{Result, anyhow};
use decimal_percentage::Percentage;
use rusty_money::{Money, iso};
use slotmap::SlotMap;

use dante::{
    basket::Basket,
    discounts::Discount,
    items::{Item, groups::ItemGroup},
    products::{Product, ProductKey},
    promotions::{Promotion, PromotionKey, PromotionMeta, simple_discount::SimpleDiscount},
    receipt::Receipt,
    solvers::{Solver, ilp::ILPSolver},
    tags::string::StringTagCollection,
};

/// Simple Discounts Example
#[expect(clippy::print_stdout, reason = "Example code")]
pub fn main() -> Result<()> {
    // Create Products
    let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();

    let sandwich_product = product_meta.insert(Product {
        name: "Sandwich".to_string(),
        tags: StringTagCollection::from_strs(&[]),
        price: Money::from_minor(2_99, iso::GBP),
    });

    let sandwich = product_meta
        .get(sandwich_product)
        .ok_or(anyhow!("sandwich product not found"))?;

    let sandwich_item = Item::with_tags(sandwich_product, sandwich.price, sandwich.tags.clone());

    let drink_key = product_meta.insert(Product {
        name: "Drink".to_string(),
        tags: StringTagCollection::from_strs(&["20-off"]),
        price: Money::from_minor(1_29, iso::GBP),
    });

    let drink = product_meta
        .get(drink_key)
        .ok_or(anyhow!("drink product not found"))?;

    let drink_item = Item::with_tags(drink_key, drink.price, drink.tags.clone());

    let snack_key = product_meta.insert(Product {
        name: "Snack".to_string(),
        tags: StringTagCollection::from_strs(&["20-off", "40-off"]),
        price: Money::from_minor(79, iso::GBP),
    });

    let snack = product_meta
        .get(snack_key)
        .ok_or(anyhow!("snack product not found"))?;

    let snack_item = Item::with_tags(snack_key, snack.price, snack.tags.clone());

    // Create promotions
    let mut promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

    let promo_20_key = promotion_meta.insert(PromotionMeta {
        name: "20% off".to_string(),
    });

    let promo_20 = Promotion::SimpleDiscount(SimpleDiscount::new(
        promo_20_key,
        StringTagCollection::from_strs(&["20-off"]),
        Discount::PercentageOffBundleTotal(Percentage::from(0.20)),
    ));

    let promo_40_key = promotion_meta.insert(PromotionMeta {
        name: "40% off".to_string(),
    });

    let promo_40 = Promotion::SimpleDiscount(SimpleDiscount::new(
        promo_40_key,
        StringTagCollection::from_strs(&["40-off"]),
        Discount::PercentageOffBundleTotal(Percentage::from(0.40)),
    ));

    let items = [sandwich_item, drink_item, snack_item];

    let promotions = [promo_20, promo_40];

    let basket = Basket::with_items(&items, iso::GBP)?;

    let item_group = ItemGroup::from(&basket);

    let start = Instant::now();

    let result = ILPSolver::solve(&promotions, &item_group)?;

    let elapsed = start.elapsed().as_secs_f32();

    let receipt = Receipt::from_solver_result(&basket, result)?;

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    receipt.write_to(&mut handle, &basket, &product_meta, &promotion_meta)?;

    println!("\nSolution: {elapsed}s");

    Ok(())
}
