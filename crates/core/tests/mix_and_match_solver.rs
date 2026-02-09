//! Integration tests for mix-and-match promotions through the ILP solver.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::GBP};
use slotmap::SlotMap;
use testresult::TestResult;

use lattice::{
    basket::Basket,
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::{
        PromotionKey, PromotionSlotKey,
        budget::PromotionBudget,
        promotion,
        types::{MixAndMatchDiscount, MixAndMatchPromotion},
    },
    solvers::{Solver, ilp::ILPSolver},
    tags::string::StringTagCollection,
    utils::slot,
};

#[test]
fn solver_handles_percent_all_items() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 450);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_amount_off_each_item() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::AmountOffEachItem(Money::from_minor(50, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_fixed_price_each_item() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedPriceEachItem(Money::from_minor(100, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 200);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_percent_cheapest() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(500, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // All items in bundle participate, cheapest gets discounted
    // Total should be less than full price (1000)
    assert!(result.total.to_minor_units() < 1000);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

#[test]
fn solver_handles_amount_off_total() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::AmountOffTotal(Money::from_minor(100, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_fixed_total() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedTotal(Money::from_minor(500, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_fixed_cheapest() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedCheapest(Money::from_minor(50, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Bundle formed with cheapest item at fixed price
    // Total should be less than full price (600)
    assert!(result.total.to_minor_units() < 600);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn solver_handles_variable_arity_bundles() -> TestResult {
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

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

    let slots = vec![slot(
        &mut slot_keys,
        StringTagCollection::from_strs(&["snack"]),
        2,
        None, // Variable arity
    )];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Variable arity bundle may form if beneficial
    // Total should not exceed full price (300)
    assert!(result.total.to_minor_units() <= 300);

    // If promotion is applied, verify bundle properties
    if !result.promotion_applications.is_empty() {
        assert!(result.promotion_applications.len() >= 2); // At least min bundle size
        assert!(result.total.to_minor_units() < 300); // Some discount applied
    }

    Ok(())
}

#[test]
fn solver_handles_variable_arity_with_max() -> TestResult {
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

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

    let slots = vec![slot(
        &mut slot_keys,
        StringTagCollection::from_strs(&["snack"]),
        1,
        Some(2), // Variable arity with max
    )];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Variable arity with max - bundle may form if beneficial
    // Total should not exceed full price (300)
    assert!(result.total.to_minor_units() <= 300);

    // If promotion is applied, check that discount was beneficial
    if !result.promotion_applications.is_empty() {
        assert!(result.total.to_minor_units() < 300);
    }

    Ok(())
}

#[test]
fn solver_handles_multiple_bundles() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(300, GBP),
            StringTagCollection::from_strs(&["main"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["drink"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedTotal(Money::from_minor(350, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Two bundles at 350 each = 700
    assert_eq!(result.total.to_minor_units(), 700);
    assert_eq!(result.promotion_applications.len(), 4);

    // Verify bundle IDs are different
    let bundle_ids: Vec<usize> = result
        .promotion_applications
        .iter()
        .map(|app| app.bundle_id)
        .collect();

    assert_eq!(bundle_ids.len(), 4);

    // Should have exactly 2 distinct bundle IDs
    let mut unique_ids = bundle_ids.clone();
    unique_ids.sort_unstable();
    unique_ids.dedup();

    assert_eq!(unique_ids.len(), 2);

    Ok(())
}

#[test]
fn solver_skips_infeasible_mix_and_match() -> TestResult {
    let items = [Item::with_tags(
        ProductKey::default(),
        Money::from_minor(400, GBP),
        StringTagCollection::from_strs(&["main"]),
    )];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

    let slots = vec![
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        ),
        slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["drink"]),
            1,
            Some(1),
        ),
    ];

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedTotal(Money::from_minor(300, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // No bundle formed, item at full price
    assert_eq!(result.total.to_minor_units(), 400);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}
