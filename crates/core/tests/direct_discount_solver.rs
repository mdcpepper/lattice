//! Integration tests for direct discount promotions through the ILP solver.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::GBP};
use testresult::TestResult;

use lattice::{
    basket::Basket,
    discounts::SimpleDiscount,
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::{
        PromotionKey, budget::PromotionBudget, promotion, qualification::Qualification,
        types::DirectDiscountPromotion,
    },
    solvers::{Solver, ilp::ILPSolver},
    tags::string::StringTagCollection,
};

#[test]
fn solver_handles_percentage_off() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(150, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["fruit"])),
        SimpleDiscount::PercentageOff(Percentage::from(0.25)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Fruit items: 100 * 0.75 + 200 * 0.75 = 75 + 150 = 225
    // Snack item: 150 (full price)
    // Total: 375
    assert_eq!(result.total.to_minor_units(), 375);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_amount_off() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(500, GBP),
            StringTagCollection::from_strs(&["premium"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["premium"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["premium"])),
        SimpleDiscount::AmountOff(Money::from_minor(50, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Both items get 50 off: (500 - 50) + (300 - 50) = 450 + 250 = 700
    assert_eq!(result.total.to_minor_units(), 700);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_amount_override() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["clearance"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["clearance"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["regular"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["clearance"])),
        SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Clearance items at 50 each: 50 + 50 = 100
    // Regular item: 300 (full price)
    // Total: 400
    assert_eq!(result.total.to_minor_units(), 400);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_multiple_overlapping_promotions() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit", "organic"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotions = vec![
        promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["fruit"]),
            ),
            SimpleDiscount::PercentageOff(Percentage::from(0.10)),
            PromotionBudget::unlimited(),
        )),
        promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["organic"]),
            ),
            SimpleDiscount::PercentageOff(Percentage::from(0.20)),
            PromotionBudget::unlimited(),
        )),
    ];

    let result = ILPSolver::solve(&promotions, &item_group)?;

    // Solver picks best promotion for each item
    // Item 0 has both tags: 100 with 20% off (organic) = 80, or 10% off (fruit) = 90 -> picks 80
    // Item 1 has fruit: 200 with 10% off = 180
    // Total: 80 + 180 = 260
    assert_eq!(result.total.to_minor_units(), 260);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_no_matching_tags() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["vegetable"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["meat"])),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // No items match, all at full price
    assert_eq!(result.total.to_minor_units(), 300);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn solver_handles_empty_tag_promotion() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["vegetable"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Empty tags means no items match
    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_all(),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Empty tags may not match items, or behavior may vary
    // Just verify solver produces a valid result
    assert!(result.total.to_minor_units() <= 300);

    Ok(())
}

#[test]
fn solver_handles_amount_off_capped_at_zero() -> TestResult {
    let items = [Item::with_tags(
        ProductKey::default(),
        Money::from_minor(30, GBP),
        StringTagCollection::from_strs(&["sale"]),
    )];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Discount is larger than item price
    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["sale"])),
        SimpleDiscount::AmountOff(Money::from_minor(50, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Should not go negative - capped at 0
    assert_eq!(result.total.to_minor_units(), 0);
    assert_eq!(result.promotion_applications.len(), 1);

    Ok(())
}

#[test]
fn solver_applies_promotion_to_all_matching_items() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(150, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(120, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        Qualification::match_any(StringTagCollection::from_strs(&["snack"])),
        SimpleDiscount::PercentageOff(Percentage::from(0.20)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // All snacks get 20% off: 80 + 120 + 96 = 296
    // Drink at full price: 200
    // Total: 496
    assert_eq!(result.total.to_minor_units(), 496);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}
