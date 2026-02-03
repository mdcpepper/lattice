//! ILP Solver

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, Variable, variable};
use num_traits::ToPrimitive;
use rusty_money::{Money, iso};
use smallvec::{SmallVec, smallvec};

#[cfg(feature = "solver-highs")]
use good_lp::solvers::highs::highs as default_solver;
#[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
use good_lp::solvers::microlp::microlp as default_solver;

use crate::{
    basket::Basket,
    promotions::{Promotion, applications::PromotionApplication},
    solvers::{
        Solver, SolverError, SolverResult,
        ilp::{promotions::PromotionInstances, state::ILPState},
    },
};

pub mod promotions;
pub mod state;

/// Binary threshold for determining truthiness
pub const BINARY_THRESHOLD: f64 = 0.5;

type ItemIndexList = SmallVec<[usize; 10]>;
type ItemUsageFlags = SmallVec<[bool; 10]>;
type AppliedPromotionState<'a> = (ItemIndexList, ItemUsageFlags, Money<'a, iso::Currency>);
type FullPriceState<'a> = (ItemIndexList, Money<'a, iso::Currency>);

/// Solver using Integer Linear Programming (ILP)
#[derive(Debug)]
pub struct ILPSolver;

impl Solver for ILPSolver {
    fn solve<'a>(
        promotions: &'a [Promotion<'_>],
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> Result<SolverResult<'a>, SolverError> {
        // Return early if no items are selected
        if items.is_empty() {
            return Ok(SolverResult {
                affected_items: SmallVec::with_capacity(0),
                unaffected_items: SmallVec::with_capacity(0),
                total: Money::from_minor(0, basket.currency()),
                promotion_applications: SmallVec::with_capacity(0),
            });
        }

        // Build the optimization problem using ILPState to manage variables and objective.
        // The goal is to find the best combination of promotions that minimizes
        // total basket cost.
        //
        // We set up three things:
        //
        // 1. Presence variables: each item at full price (baseline option)
        // 2. Promotion variables: each item with each applicable promotion (discount options)
        // 3. Constraints: ensure each item is purchased exactly once (baseline full price OR one promotion discount applied)

        // Set up all possible promotion choices for the solver to consider.
        // For each promotion, we create decision variables that let the solver choose
        // whether to apply that promotion to each eligible item.
        let mut state = ILPState::with_presence_variables(basket, items)?;

        let promotion_instances =
            PromotionInstances::from_promotions(promotions, basket, items, &mut state)?;

        // Extract state for model creation
        let (pb, cost, item_presence) = state.into_parts();

        // Create the solver model
        let mut model = pb.minimise(cost).using(default_solver);

        ensure_presence_vars_len(item_presence.len(), items.len())?;

        // Ensure each item is purchased exactly once (either full price OR via one promotion).
        //
        // This prevents items from being:
        // - Omitted from the checkout entirely
        // - Selected by multiple promotions simultaneously
        //
        // Example: If "20% off" and "Buy-one-get-one" both target the same item,
        // the solver must choose one or neither, never both.
        for (i, &item_idx) in items.iter().enumerate() {
            let z_i = item_presence
                .get(i)
                .ok_or(SolverError::InvariantViolation {
                    message: "presence variable missing for item index",
                })?;

            let constraint_expr =
                promotion_instances.add_item_presence_term(Expression::from(*z_i), item_idx);

            model = model.with(constraint_expr.eq(1));
        }

        // Add constraints for all promotions
        model = promotion_instances.add_constraints(model, basket, items);

        let solution = model.solve()?;

        // Translate the solver's decisions back into business terms: which items got
        // discounted, by which promotions, and what their final prices are.
        let mut used_items: ItemUsageFlags = smallvec![false; items.len()];
        let mut total = Money::from_minor(0, basket.currency());
        let mut promotion_applications: SmallVec<[PromotionApplication<'a>; 10]> = SmallVec::new();
        let mut next_bundle_id: usize = 0;
        let mut affected_items: ItemIndexList = ItemIndexList::new();

        // Extract which items each promotion selected and their discounted prices
        for instance in promotion_instances.iter() {
            let apps =
                instance.calculate_item_applications(&solution, basket, items, &mut next_bundle_id);

            let (applied_items, updated_used_items, updated_total) =
                apply_promotion_applications(items, used_items, total, &apps)?;

            affected_items.extend(applied_items);
            used_items = updated_used_items;
            total = updated_total;

            promotion_applications.extend(apps);
        }

        let (unaffected_items, total) =
            collect_full_price_items(basket, &solution, &item_presence, items, used_items, total)?;

        Ok(SolverResult {
            affected_items,
            unaffected_items,
            total,
            promotion_applications,
        })
    }
}

/// Ensure that the number of presence variables matches the number of selected items.
fn ensure_presence_vars_len(z_len: usize, items_len: usize) -> Result<(), SolverError> {
    if z_len != items_len {
        return Err(SolverError::InvariantViolation {
            message: "presence variable count does not match number of selected items",
        });
    }

    Ok(())
}

/// Build presence variables and objective function for the ILP solver.
///
/// # Errors
///
/// Returns a [`SolverError`] if any items contains a Money amount in minor units
/// that cannot be represented exactly as a solver coefficient.
fn build_presence_variables_and_objective<'a>(
    basket: &'a Basket<'a>,
    items: &[usize],
    pb: &mut ProblemVariables,
) -> Result<(SmallVec<[Variable; 10]>, Expression), SolverError> {
    // Each item must be present in the solution whether a promotion is applied or not.
    // Create a presence variable for each item representing the full-price option.
    let presence: SmallVec<[Variable; 10]> = (0..items.len())
        .map(|_| pb.add(variable().binary()))
        .collect();

    // Create expression for total cost. This is what we are trying to minimise.
    let mut cost = Expression::default();

    // Add the full-price option for each item to the objective.
    // These are the baseline costs if no promotions are applied. When we add promotion
    // variables later, they'll offer alternative (discounted) costs. The solver will
    // compare full-price vs. discounted options and choose what minimizes the total.
    presence
        .iter()
        .copied()
        .zip(items.iter().copied())
        .try_for_each(|(var, item_idx)| -> Result<(), SolverError> {
            let minor_units = basket.get_item(item_idx)?.price().to_minor_units();

            // `good_lp` stores coefficients as `f64`. Only integers with absolute value <= 2^53
            // can be represented exactly in an IEEE-754 `f64` mantissa; enforce that via a
            // round-trip check so we never silently change the objective.
            cost += var
                * i64_to_f64_exact(minor_units)
                    .ok_or(SolverError::MinorUnitsNotRepresentable { minor_units })?;

            Ok(())
        })?;

    Ok((presence, cost))
}

/// Collect unaffected items and their total price.
///
/// # Errors
///
/// Returns a [`SolverError`] if any items contains a Money amount in minor units
/// that cannot be represented exactly as a solver coefficient.
fn collect_full_price_items<'a>(
    basket: &'a Basket<'a>,
    solution: &impl Solution,
    z: &[Variable],
    items: &[usize],
    mut used_items: ItemUsageFlags,
    mut total: Money<'a, iso::Currency>,
) -> Result<FullPriceState<'a>, SolverError> {
    let mut unaffected_items = SmallVec::new();

    // Any item that wasn't claimed by a promotion is treated as an unaffected
    // full-price item and contributes its full price to the total.
    z.iter()
        .copied()
        .zip(items.iter().copied())
        .zip(used_items.iter_mut())
        .try_for_each(|((var, item_idx), used)| -> Result<(), SolverError> {
            // `var` is a binary decision variable; the solver return floats, so treat values
            // greater than 0.5 as "selected" (i.e. 1) to tolerate tiny numerical noise.
            if solution.value(var) > BINARY_THRESHOLD && !*used {
                // Add the item to the list of unaffected items.
                unaffected_items.push(item_idx);

                // Add the item's full price to the result total.
                total = total.add(*basket.get_item(item_idx)?.price())?;

                // Mark the item as used.
                *used = true;
            }

            Ok(())
        })?;

    Ok((unaffected_items, total))
}

/// Apply promotion applications to track affected items and accumulate total.
///
/// This function processes [`PromotionApplication`] instances, ensuring each position
/// in `items` is used at most once (via `used_items`), records affected item indices,
/// and adds the final prices to `total`.
///
/// Note: `used_items` is indexed by the *position* in `items` (not the item index
/// itself). This keeps it aligned with other per-variable/per-position arrays used
/// by the ILP formulation.
///
/// # Errors
///
/// Returns a [`SolverError`] if adding a final price to `total` fails.
fn apply_promotion_applications<'a>(
    items: &[usize],
    mut used_items: ItemUsageFlags,
    mut total: Money<'a, iso::Currency>,
    applications: &[PromotionApplication<'a>],
) -> Result<AppliedPromotionState<'a>, SolverError> {
    // The indexes of items that are being affected by promotions
    let mut affected_items: ItemIndexList = ItemIndexList::new();

    for app in applications {
        // Find the position of this item in the items slice
        let Some(pos) = items.iter().position(|&idx| idx == app.item_idx) else {
            continue;
        };

        // If this position is already claimed, skip it to avoid double-counting.
        if let Some(used) = used_items.get(pos)
            && *used
        {
            continue;
        }

        // Commit to consuming this item position as soon as we apply its discount.
        if let Some(used) = used_items.get_mut(pos) {
            *used = true;
        }

        // Track that this item was included in a promotion.
        affected_items.push(app.item_idx);

        // Add the final price to the running total.
        total = total.add(app.final_price)?;
    }

    Ok((affected_items, used_items, total))
}

/// Convert an `i64` to an `f64` if it can be represented exactly.
fn i64_to_f64_exact(v: i64) -> Option<f64> {
    let f = v.to_f64()?;

    (f.to_i64() == Some(v)).then_some(f)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use decimal_percentage::Percentage;
    use rusty_money::iso;
    use testresult::TestResult;

    use crate::{
        discounts::Discount,
        items::Item,
        products::ProductKey,
        promotions::{
            Promotion, PromotionKey, applications::PromotionApplication,
            simple_discount::SimpleDiscount,
        },
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(ProductKey::default(), Money::from_minor(100, iso::GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, iso::GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, iso::GBP)),
        ]
    }

    fn test_items_with_tags<'a>() -> [Item<'a>; 3] {
        [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, iso::GBP),
                StringTagCollection::from_strs(&["a"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, iso::GBP),
                StringTagCollection::from_strs(&["b"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, iso::GBP),
                StringTagCollection::from_strs(&["a", "b"]),
            ),
        ]
    }

    #[test]
    fn solver_returns_all_items_full_price_with_no_promotions() -> TestResult {
        let items = test_items();

        let subtotal = items
            .iter()
            .map(|item| item.price().to_minor_units())
            .sum::<i64>();

        let basket = Basket::with_items(items, iso::GBP)?;

        let result = ILPSolver::solve(&[], &basket, &[0, 1, 2])?;

        assert_eq!(subtotal, result.total.to_minor_units());
        assert_eq!(0, result.affected_items.len());
        assert_eq!(3, result.unaffected_items.len());
        assert!(result.promotion_applications.is_empty());

        Ok(())
    }

    #[test]
    fn apply_applications_skips_pre_used_positions() -> TestResult {
        // `apply_applications` uses `used_items` (indexed by position in the `items` slice)
        // to prevent an item position from being claimed by more than one promotion.
        let mut used_items: ItemUsageFlags = smallvec![false; 3];

        // Simulate a different promotion already consuming the middle position.
        if let Some(used) = used_items.get_mut(1) {
            *used = true;
        }

        // Start from zero so any applied discount would be visible in the result total.
        let total = Money::from_minor(0, iso::GBP);

        // Provide an application for item index 1, but the corresponding position is pre-used above.
        let applications = [PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 1,
            bundle_id: 0,
            original_price: Money::from_minor(200, iso::GBP),
            final_price: Money::from_minor(150, iso::GBP),
        }];

        let (affected_items, _used_items, total) =
            apply_promotion_applications(&[0, 1, 2], used_items, total, &applications)?;

        // Because the only discounted item was already marked "used", nothing should be applied.
        assert!(affected_items.is_empty());
        assert_eq!(total.to_minor_units(), 0);

        Ok(())
    }

    #[test]
    fn apply_applications_skips_items_not_in_selection() -> TestResult {
        let used_items: ItemUsageFlags = smallvec![false; 2];
        let total = Money::from_minor(0, iso::GBP);

        let applications = [PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 99,
            bundle_id: 0,
            original_price: Money::from_minor(200, iso::GBP),
            final_price: Money::from_minor(150, iso::GBP),
        }];

        let (affected_items, _used_items, total) =
            apply_promotion_applications(&[0, 1], used_items, total, &applications)?;

        assert!(affected_items.is_empty());
        assert_eq!(total.to_minor_units(), 0);

        Ok(())
    }

    #[test]
    fn solver_applies_percentage_discount_to_tagged_items() -> TestResult {
        let items = test_items_with_tags();
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            Discount::PercentageOffBundleTotal(Percentage::from(0.25)),
        ))];

        let result = ILPSolver::solve(&promotions, &basket, &[0, 1, 2])?;

        assert_eq!(result.total.to_minor_units(), 500);

        let mut affected = result.affected_items.clone();
        affected.sort_unstable();
        assert_eq!(affected.as_slice(), &[0, 2]);

        let mut unaffected = result.unaffected_items.clone();
        unaffected.sort_unstable();
        assert_eq!(unaffected.as_slice(), &[1]);

        Ok(())
    }

    #[test]
    fn solver_applies_price_override_to_all_items_with_empty_tag_promotion() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &basket, &[0, 1, 2])?;

        assert_eq!(result.total.to_minor_units(), 150);

        let mut affected = result.affected_items.clone();
        affected.sort_unstable();
        assert_eq!(affected.as_slice(), &[0, 1, 2]);

        assert!(result.unaffected_items.is_empty());

        Ok(())
    }

    #[test]
    fn solver_ignores_discount_when_no_items_match() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["missing"]),
            Discount::PercentageOffBundleTotal(Percentage::from(0.25)),
        ))];

        let result = ILPSolver::solve(&promotions, &basket, &[0, 1, 2])?;

        assert_eq!(result.total.to_minor_units(), 600);
        assert!(result.affected_items.is_empty());

        let mut unaffected = result.unaffected_items.clone();
        unaffected.sort_unstable();
        assert_eq!(unaffected.as_slice(), &[0, 1, 2]);

        Ok(())
    }

    #[test]
    fn solver_prefers_full_price_when_discount_is_worse() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(400, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &basket, &[0, 1, 2])?;

        assert_eq!(result.total.to_minor_units(), 600);
        assert!(result.affected_items.is_empty());

        let mut unaffected = result.unaffected_items.clone();
        unaffected.sort_unstable();
        assert_eq!(unaffected.as_slice(), &[0, 1, 2]);

        Ok(())
    }

    #[test]
    fn solver_populates_promotion_applications_with_correct_details() -> TestResult {
        let items = test_items_with_tags();
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &basket, &[0, 1, 2])?;

        // Items 0 and 2 have tag "a", so should be discounted
        assert_eq!(result.promotion_applications.len(), 2);

        // Sort by item_idx to get deterministic ordering for assertions
        let mut sorted_apps: Vec<_> = result.promotion_applications.iter().collect();
        sorted_apps.sort_by_key(|a| a.item_idx);

        // First application (item 0)
        let first_app = sorted_apps.first();
        assert!(first_app.is_some());
        let first_app = first_app.ok_or("Expected first application")?;
        assert_eq!(first_app.item_idx, 0);
        assert_eq!(first_app.original_price, Money::from_minor(100, iso::GBP));
        assert_eq!(first_app.final_price, Money::from_minor(50, iso::GBP));

        // Second application (item 2)
        let second_app = sorted_apps.get(1);
        assert!(second_app.is_some());
        let second_app = second_app.ok_or("Expected second application")?;
        assert_eq!(second_app.item_idx, 2);
        assert_eq!(second_app.original_price, Money::from_minor(300, iso::GBP));
        assert_eq!(second_app.final_price, Money::from_minor(50, iso::GBP));

        // Each item should have a unique bundle_id (SimpleDiscount doesn't bundle)
        assert_ne!(first_app.bundle_id, second_app.bundle_id);

        Ok(())
    }

    #[test]
    fn solver_with_no_items_returns_empty_result() -> TestResult {
        let basket = Basket::with_items([], iso::GBP)?;

        let result = ILPSolver::solve(&[], &basket, &[])?;

        assert_eq!(result.total.to_minor_units(), 0);
        assert!(result.affected_items.is_empty());
        assert!(result.unaffected_items.is_empty());
        assert!(result.promotion_applications.is_empty());

        Ok(())
    }

    #[test]
    fn presence_vars_len_mismatch_returns_invariant_error() {
        let err = ensure_presence_vars_len(1, 2).err();

        assert!(matches!(
            err,
            Some(SolverError::InvariantViolation { message })
                if message == "presence variable count does not match number of selected items"
        ));
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "This is a test case for exact conversion"
    )]
    fn i64_to_f64_exact_accepts_exactly_representable_integers() {
        let cases: [i64; 5] = [0, 1, -1, 123, 9_007_199_254_740_992]; // 2^53

        for v in cases {
            assert_eq!(i64_to_f64_exact(v), Some(v as f64));
        }
    }

    #[test]
    fn i64_to_f64_exact_rejects_nonrepresentable_integers() {
        let cases: [i64; 2] = [9_007_199_254_740_993, -9_007_199_254_740_993]; // 2^53 + 1

        for v in cases {
            assert_eq!(i64_to_f64_exact(v), None);
        }
    }

    #[test]
    fn base_objective_matches_sum_of_item_minor_units() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let mut pb = ProblemVariables::new();
        let (z, objective) = build_presence_variables_and_objective(&basket, &[0, 1, 2], &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let expected = 600.0_f64;
        let actual = solution.eval(&objective);

        assert!((actual - expected).abs() <= f64::EPSILON);

        Ok(())
    }

    #[test]
    fn unaffected_items_collection_skips_pre_used_items() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let mut pb = ProblemVariables::new();
        let (z, _objective) = build_presence_variables_and_objective(&basket, &[0, 1, 2], &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let mut used_items: ItemUsageFlags = smallvec![false; 3];
        used_items[1] = true; // pretend item 1 was claimed by a promotion

        let total = Money::from_minor(0, basket.currency());

        let (unaffected_items, total) =
            collect_full_price_items(&basket, &solution, &z, &[0, 1, 2], used_items, total)?;

        assert_eq!(unaffected_items.as_slice(), &[0, 2]);
        assert_eq!(total.to_minor_units(), 400);

        Ok(())
    }
}
