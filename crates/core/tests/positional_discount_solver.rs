//! Integration tests for positional discount promotions through the ILP solver.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::GBP};
use smallvec::SmallVec;
use testresult::TestResult;

use lattice::{
    basket::Basket,
    discounts::SimpleDiscount,
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::{
        PromotionKey, budget::PromotionBudget, promotion, types::PositionalDiscountPromotion,
    },
    solvers::{Solver, ilp::ILPSolver},
    tags::string::StringTagCollection,
};

#[test]
fn solver_handles_buy_one_get_one_free() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // BOGOF: size=2, discount position 1 (second item) at 100% off
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["fruit"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // One item full price, one free: 100 + 0 = 100
    assert_eq!(result.total.to_minor_units(), 100);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_three_for_two() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // 3-for-2: size=3, discount position 2 (third item) at 100% off
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["snack"]),
        3,
        SmallVec::from_vec(vec![2]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Two items full price, one free: 100 + 100 + 0 = 200
    assert_eq!(result.total.to_minor_units(), 200);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

#[test]
fn solver_handles_buy_two_get_one_half_off() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["book"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["book"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["book"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Buy 2 get 1 half off: size=3, discount position 2 at 50% off
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["book"]),
        3,
        SmallVec::from_vec(vec![2]),
        SimpleDiscount::PercentageOff(Percentage::from(0.5)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Two at full price, one at half: 200 + 200 + 100 = 500
    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

#[test]
fn solver_handles_multiple_discount_positions() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Size=4, discount positions 1 and 3 (2nd and 4th items) at 50% off
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["item"]),
        4,
        SmallVec::from_vec(vec![1, 3]),
        SimpleDiscount::PercentageOff(Percentage::from(0.5)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // 2 at full price, 2 at half: 100 + 50 + 100 + 50 = 300
    assert_eq!(result.total.to_minor_units(), 300);
    assert_eq!(result.promotion_applications.len(), 4);

    Ok(())
}

#[test]
fn solver_handles_insufficient_items_for_bundle() -> TestResult {
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

    // Requires 3 items but only 1 matches
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["fruit"]),
        3,
        SmallVec::from_vec(vec![2]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Not enough items to form bundle, all at full price
    assert_eq!(result.total.to_minor_units(), 300);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn solver_handles_multiple_bundles() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(50, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(50, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(50, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(50, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // BOGOF - can form 2 bundles
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["snack"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // 2 bundles: 50 + 0 + 50 + 0 = 100
    assert_eq!(result.total.to_minor_units(), 100);
    assert_eq!(result.promotion_applications.len(), 4);

    Ok(())
}

#[test]
fn solver_handles_fixed_price_discount() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["premium"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(250, GBP),
            StringTagCollection::from_strs(&["premium"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // BOGOF with fixed price override
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["premium"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::AmountOverride(Money::from_minor(100, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Bundle should be formed with second item at fixed price
    // Total should be less than full price (550)
    assert!(result.total.to_minor_units() < 550);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_mixed_prices_in_bundle() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(150, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["item"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // 3-for-2 (cheapest item free)
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["item"]),
        3,
        SmallVec::from_vec(vec![2]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Solver optimizes to discount the cheapest: 150 + 100 + 0 = 250
    // Or could be: 200 + 150 + 0 = 350, but solver picks cheapest
    assert!(result.total.to_minor_units() <= 350);
    assert_eq!(result.promotion_applications.len(), 3);

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
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["vegetable"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["snack"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // No items match, all at full price
    assert_eq!(result.total.to_minor_units(), 200);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn solver_handles_amount_off_in_bundle() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["candy"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["candy"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // BOGOF with 30 off instead of free
    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["candy"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::AmountOff(Money::from_minor(30, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // One at full price, one with 30 off: 100 + 70 = 170
    assert_eq!(result.total.to_minor_units(), 170);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}
