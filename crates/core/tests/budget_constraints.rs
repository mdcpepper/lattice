//! Integration tests for promotion budget constraints

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::GBP};
use slotmap::SlotMap;
use smallvec::SmallVec;
use testresult::TestResult;

use lattice::{
    basket::Basket,
    discounts::SimpleDiscount,
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::{
        PromotionKey, PromotionSlotKey,
        budget::PromotionBudget,
        promotion,
        types::{
            DirectDiscountPromotion, MixAndMatchDiscount, MixAndMatchPromotion,
            PositionalDiscountPromotion, ThresholdDiscount, ThresholdTier,
            TieredThresholdPromotion,
        },
    },
    solvers::{Solver, ilp::ILPSolver},
    tags::string::StringTagCollection,
    utils::slot,
};

#[test]
fn direct_discount_respects_application_limit() -> TestResult {
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
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Budget: maximum 2 applications
    let budget = PromotionBudget {
        application_limit: Some(2),
        monetary_limit: None,
    };

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["fruit"]),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Only 2 items should get the discount: 50 + 50 + 100 = 200
    assert_eq!(result.total.to_minor_units(), 200);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn direct_discount_respects_monetary_limit() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["sale"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["sale"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["sale"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Budget: maximum 75 pence in total discount (50% off = 50p per item)
    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(75, GBP)),
    };

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["sale"]),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Can discount at most 1.5 items worth (75p total discount allowed)
    // So 1 item at 50, 1 item at 75 (partial), 1 at 100 = at least 225
    assert!(result.total.to_minor_units() >= 225);

    Ok(())
}

#[test]
fn mix_and_match_respects_application_limit() -> TestResult {
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

    // Budget: maximum 1 bundle
    let budget = PromotionBudget {
        application_limit: Some(1),
        monetary_limit: None,
    };

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::FixedTotal(Money::from_minor(350, GBP)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Only one bundle at 350, other two items at full price: 350 + 300 + 100 = 750
    assert_eq!(result.total.to_minor_units(), 750);
    assert_eq!(result.promotion_applications.len(), 2); // Only one bundle

    Ok(())
}

#[test]
fn mix_and_match_respects_monetary_limit() -> TestResult {
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

    // Budget: maximum 50 pence discount
    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(50, GBP)),
    };

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Normal discount would be 150 total, but limited to 50
    // So total should be 600 - 50 = 550
    assert!(result.total.to_minor_units() >= 550);

    Ok(())
}

#[test]
fn mix_and_match_cheapest_budget_uses_exact_target_discount() -> TestResult {
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

    // Cheapest is 200; 50% off cheapest = 100 total discount.
    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(100, GBP)),
    };

    let promotion = promotion(MixAndMatchPromotion::new(
        PromotionKey::default(),
        slots,
        MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

#[test]
fn positional_discount_respects_application_limit() -> TestResult {
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
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["snack"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Budget: maximum 1 bundle (size=2)
    let budget = PromotionBudget {
        application_limit: Some(1),
        monetary_limit: None,
    };

    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["snack"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Only 1 bundle can be formed: 100 + 0 + 100 + 100 = 300
    assert_eq!(result.total.to_minor_units(), 300);
    assert_eq!(result.promotion_applications.len(), 2); // Only one bundle

    Ok(())
}

#[test]
fn positional_discount_respects_monetary_limit() -> TestResult {
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

    // Budget: maximum 75 pence discount (can't do 2 full bundles)
    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(75, GBP)),
    };

    let promotion = promotion(PositionalDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["item"]),
        2,
        SmallVec::from_vec(vec![1]),
        SimpleDiscount::PercentageOff(Percentage::from(1.0)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Normal would be 2 bundles saving 200, but limited to 75 savings
    // So total should be 400 - 75 = 325
    assert!(result.total.to_minor_units() >= 325);

    Ok(())
}

#[test]
fn tiered_threshold_application_limit_counts_tiers_not_items() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1200, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(800, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(500, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let budget = PromotionBudget {
        application_limit: Some(1),
        monetary_limit: None,
    };

    let promotion = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
            StringTagCollection::from_strs(&["cheese"]),
            ThresholdDiscount::PercentEachItem(Percentage::from(1.0)),
        )],
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // One tier application is allowed, so all contributing/discounted items can participate.
    assert_eq!(result.total.to_minor_units(), 3000);
    assert_eq!(result.promotion_applications.len(), 4);

    Ok(())
}

#[test]
fn tiered_threshold_zero_application_limit_prevents_all_applications() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1200, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(800, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(500, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let budget = PromotionBudget {
        application_limit: Some(0),
        monetary_limit: None,
    };

    let promotion = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
            StringTagCollection::from_strs(&["cheese"]),
            ThresholdDiscount::PercentEachItem(Percentage::from(1.0)),
        )],
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 3500);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn tiered_threshold_cheapest_budget_uses_exact_target_discount() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["sale"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["sale"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(100, GBP)),
    };

    let promotion = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["sale"]),
            StringTagCollection::from_strs(&["sale"]),
            ThresholdDiscount::PercentCheapest(Percentage::from(0.50)),
        )],
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    assert_eq!(result.total.to_minor_units(), 500);
    assert_eq!(result.promotion_applications.len(), 1);

    Ok(())
}

#[test]
fn budget_zero_application_limit_prevents_all_applications() -> TestResult {
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

    // Budget: 0 applications allowed
    let budget = PromotionBudget {
        application_limit: Some(0),
        monetary_limit: None,
    };

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["fruit"]),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // No promotion applications
    assert_eq!(result.total.to_minor_units(), 200);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn budget_zero_monetary_limit_prevents_all_applications() -> TestResult {
    let items = [Item::with_tags(
        ProductKey::default(),
        Money::from_minor(100, GBP),
        StringTagCollection::from_strs(&["sale"]),
    )];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Budget: 0 monetary discount allowed
    let budget = PromotionBudget {
        application_limit: None,
        monetary_limit: Some(Money::from_minor(0, GBP)),
    };

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["sale"]),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // No promotion applications
    assert_eq!(result.total.to_minor_units(), 100);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

#[test]
fn budget_both_limits_enforced() -> TestResult {
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
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Budget: max 2 applications AND max 50 pence discount
    let budget = PromotionBudget {
        application_limit: Some(2),
        monetary_limit: Some(Money::from_minor(50, GBP)),
    };

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        StringTagCollection::from_strs(&["item"]),
        SimpleDiscount::PercentageOff(Percentage::from(0.50)),
        budget,
    ));

    let result = ILPSolver::solve(&[promotion], &item_group)?;

    // Both constraints should be respected
    // At most 2 applications and at most 50p discount
    assert!(result.promotion_applications.len() <= 2);
    assert!(result.total.to_minor_units() >= 250); // 300 - 50 = 250

    Ok(())
}
