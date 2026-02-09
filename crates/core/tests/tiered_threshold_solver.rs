//! Integration tests for tiered threshold promotions through the ILP solver.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::GBP};
use slotmap::SlotMap;
use testresult::TestResult;

use lattice::{
    basket::Basket,
    fixtures::Fixture,
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::{
        PromotionKey,
        budget::PromotionBudget,
        promotion,
        types::{ThresholdDiscount, ThresholdTier, TierThreshold, TieredThresholdPromotion},
    },
    solvers::{Solver, ilp::ILPSolver},
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// Example 1: "Spend £30 on wine, get 10% off cheese"
/// Wine total = £12 + £10 + £8 = £30 >= £30 threshold
/// Cheese gets 10% off: £5 -> £4.50, £4 -> £3.60, £6 -> £5.40
/// Cheese total discounted = 450 + 360 + 540 = 1350
/// Wine + cheese = 3000 + 1350 = 4350
/// Remaining (shampoo 350, toothpaste 250, bread 150, milk 120) = 870
/// Total = 4350 + 870 = 5220
#[test]
fn threshold_met_applies_discount_to_eligible_items() -> TestResult {
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
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(600, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(3000, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Wine at full price: 1200 + 1000 + 800 = 3000
    // Cheese at 10% off: 450 + 360 + 540 = 1350
    // Total: 4350
    assert_eq!(result.total.to_minor_units(), 4350);

    // All contribution and discount items participate in the promotion.
    assert_eq!(result.promotion_applications.len(), 6);

    Ok(())
}

/// Threshold not met: no discount applied
#[test]
fn threshold_not_met_no_discount_applied() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
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

    // Wine total £10, threshold £30 not met
    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(3000, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // All items at full price: 1000 + 500 = 1500
    assert_eq!(result.total.to_minor_units(), 1500);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

/// Item-count threshold not met: no discount applied even when spend threshold is met.
#[test]
fn item_count_threshold_not_met_no_discount_applied() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1500, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1500, GBP),
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

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_both_thresholds(Money::from_minor(3000, GBP), 3),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Spend threshold met (£30 on wine), but only 2 contributing items (< 3 required).
    assert_eq!(result.total.to_minor_units(), 3500);
    assert_eq!(result.promotion_applications.len(), 0);

    Ok(())
}

/// Item-count threshold met: discount applies when spend and count requirements are both met.
#[test]
fn item_count_threshold_met_applies_discount() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
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

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_both_thresholds(Money::from_minor(3000, GBP), 3),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Cheese gets 10% off: 500 -> 450
    assert_eq!(result.total.to_minor_units(), 3450);
    assert_eq!(result.promotion_applications.len(), 4);

    Ok(())
}

/// Item-count-only threshold: discount applies without a monetary threshold.
#[test]
fn item_count_only_threshold_met_applies_discount() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
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

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_item_count_threshold(2),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Cheese gets 10% off: 500 -> 450
    assert_eq!(result.total.to_minor_units(), 650);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

/// Upper threshold caps contribution/discountable value but does not deactivate the tier.
#[test]
fn upper_threshold_caps_discountable_value_without_disabling_tier() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(3000, GBP)),
            Some(TierThreshold::with_monetary_threshold(Money::from_minor(
                6000, GBP,
            ))),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Lower threshold met (>= £30). Upper threshold caps discountable value at £60,
    // so only two £30 items get discounted: 2700 + 2700 + 3000 = 8400.
    assert_eq!(result.total.to_minor_units(), 8400);
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}

/// Example 2: "Spend £50 get £5 off, spend £80 get £12 off"
/// Solver picks the best tier that minimises total cost.
#[test]
fn multiple_tiers_qualify_solver_picks_optimal() -> TestResult {
    // Total basket = 5 items at £20 each = £100
    let items: Vec<Item<'_>> = (0..5)
        .map(|_| Item::new(ProductKey::default(), Money::from_minor(2000, GBP)))
        .collect();

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Empty tags = basket-wide
    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![
            ThresholdTier::new(
                TierThreshold::with_monetary_threshold(Money::from_minor(5000, GBP)),
                None,
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                ThresholdDiscount::AmountOffEachItem(Money::from_minor(500, GBP)),
            ),
            ThresholdTier::new(
                TierThreshold::with_monetary_threshold(Money::from_minor(8000, GBP)),
                None,
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                ThresholdDiscount::AmountOffEachItem(Money::from_minor(1200, GBP)),
            ),
        ],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // With £12 off each of 5 items, total = 5 * (2000 - 1200) = 5 * 800 = 4000
    // vs £5 off each: 5 * 1500 = 7500
    // Solver picks £12 off tier
    assert_eq!(result.total.to_minor_units(), 4000);
    assert_eq!(result.promotion_applications.len(), 5);

    Ok(())
}

/// Example 3: Basket-wide threshold with basket-wide discount (empty tag sets)
#[test]
fn basket_wide_threshold_and_discount() -> TestResult {
    let items = [
        Item::new(ProductKey::default(), Money::from_minor(1500, GBP)),
        Item::new(ProductKey::default(), Money::from_minor(1000, GBP)),
        Item::new(ProductKey::default(), Money::from_minor(500, GBP)),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    // Total = £30, threshold = £20
    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(2000, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::empty(),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::empty(),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.05)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // 5% off all: 1425 + 950 + 475 = 2850
    assert_eq!(result.total.to_minor_units(), 2850);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

/// When only the lower tier qualifies, it is selected
#[test]
fn only_lower_tier_qualifies() -> TestResult {
    // Total = 3 * £20 = £60
    let items: Vec<Item<'_>> = (0..3)
        .map(|_| Item::new(ProductKey::default(), Money::from_minor(2000, GBP)))
        .collect();

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![
            ThresholdTier::new(
                TierThreshold::with_monetary_threshold(Money::from_minor(5000, GBP)),
                None,
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                ThresholdDiscount::AmountOffEachItem(Money::from_minor(500, GBP)),
            ),
            ThresholdTier::new(
                TierThreshold::with_monetary_threshold(Money::from_minor(8000, GBP)),
                None,
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                lattice::promotions::qualification::Qualification::match_any(
                    StringTagCollection::empty(),
                ),
                ThresholdDiscount::AmountOffEachItem(Money::from_minor(1200, GBP)),
            ),
        ],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    // Only £50 tier qualifies (total is £60 < £80)
    // £5 off each of 3 items: 3 * 1500 = 4500
    assert_eq!(result.total.to_minor_units(), 4500);
    assert_eq!(result.promotion_applications.len(), 3);

    Ok(())
}

/// All items in a tier share the same bundle ID
#[test]
fn tier_items_share_bundle_id() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(3000, GBP),
            StringTagCollection::from_strs(&["wine"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(500, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(400, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let promo = promotion(TieredThresholdPromotion::new(
        PromotionKey::default(),
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(2000, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promo], &item_group)?;

    assert_eq!(result.promotion_applications.len(), 3);

    // Both cheese items should share the same bundle_id
    let bundle_ids: Vec<usize> = result
        .promotion_applications
        .iter()
        .map(|a| a.bundle_id)
        .collect();

    assert_eq!(bundle_ids.first(), bundle_ids.get(1));
    assert_eq!(bundle_ids.first(), bundle_ids.get(2));

    Ok(())
}

/// Fixture-based test: load the tiered-threshold fixtures
#[test]
fn fixture_based_tiered_threshold() -> TestResult {
    let fixture = Fixture::from_set("tiered-threshold")?;
    let basket = fixture.basket(None)?;
    let item_group = ItemGroup::from(&basket);

    // Run the graph evaluation (which exercises all three promotions)
    let result = fixture.graph()?.evaluate(&item_group)?;

    // Verify the result accounts for all items
    let total_items = result.item_applications.len() + result.full_price_items.len();

    assert_eq!(total_items, 10);

    Ok(())
}

/// Contribution items are exclusive participants and cannot be claimed by other
/// promotions in the same layer.
#[test]
fn contribution_items_are_exclusive_across_promotions() -> TestResult {
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
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["cheese"]),
        ),
    ];

    let basket = Basket::with_items(items, GBP)?;
    let item_group = ItemGroup::from(&basket);

    let mut keys = SlotMap::<PromotionKey, ()>::with_key();

    let wine_cheese_key = keys.insert(());
    let basket_key = keys.insert(());

    let wine_cheese = promotion(TieredThresholdPromotion::new(
        wine_cheese_key,
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(3000, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["wine"]),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::from_strs(&["cheese"]),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(1.0)),
        )],
        PromotionBudget::unlimited(),
    ));

    let basket_wide = promotion(TieredThresholdPromotion::new(
        basket_key,
        vec![ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(0, GBP)),
            None,
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::empty(),
            ),
            lattice::promotions::qualification::Qualification::match_any(
                StringTagCollection::empty(),
            ),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.05)),
        )],
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[wine_cheese, basket_wide], &item_group)?;

    // Wine & cheese wins only if all wine contributors participate in that promotion.
    assert_eq!(result.total.to_minor_units(), 3000);
    assert_eq!(result.promotion_applications.len(), 4);
    assert!(
        result
            .promotion_applications
            .iter()
            .all(|app| app.promotion_key == wine_cheese_key)
    );

    Ok(())
}
