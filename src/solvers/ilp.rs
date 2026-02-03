//! ILP Solver

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, Variable, variable};
use num_traits::ToPrimitive;
use rusty_money::{Money, iso::Currency};
use smallvec::{SmallVec, smallvec};

#[cfg(feature = "solver-highs")]
use good_lp::solvers::highs::highs as default_solver;
#[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
use good_lp::solvers::microlp::microlp as default_solver;

use crate::{
    items::groups::ItemGroup,
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
type AppliedPromotionState<'a> = (ItemIndexList, ItemUsageFlags, Money<'a, Currency>);
type FullPriceState<'a> = (ItemIndexList, Money<'a, Currency>);

/// Solver using Integer Linear Programming (ILP)
#[derive(Debug)]
pub struct ILPSolver;

impl Solver for ILPSolver {
    fn solve<'group>(
        promotions: &[Promotion<'_>],
        item_group: &'group ItemGroup<'_>,
    ) -> Result<SolverResult<'group>, SolverError> {
        // Return early if the item group is empty
        if item_group.is_empty() {
            return Ok(SolverResult {
                affected_items: SmallVec::with_capacity(0),
                unaffected_items: SmallVec::with_capacity(0),
                total: Money::from_minor(0, item_group.currency()),
                promotion_applications: SmallVec::with_capacity(0),
            });
        }

        // Build the optimization problem using ILPState to manage variables and objective.
        // The goal is to find the best combination of promotions that minimizes
        // total item group cost.
        //
        // We set up three things:
        //
        // 1. Presence variables: each item at full price (baseline option)
        // 2. Promotion variables: each item with each applicable promotion (discount options)
        // 3. Constraints: ensure each item is purchased exactly once (baseline full price OR one promotion discount applied)

        // Set up all possible promotion choices for the solver to consider.
        // For each promotion, we create decision variables that let the solver choose
        // whether to apply that promotion to each eligible item.
        let mut state = ILPState::with_presence_variables(item_group)?;

        let promotion_instances =
            PromotionInstances::from_promotions(promotions, item_group, &mut state)?;

        // Extract state for model creation
        let (pb, cost, item_presence) = state.into_parts();

        // Create the solver model
        let mut model = pb.minimise(cost).using(default_solver);

        ensure_presence_vars_len(item_presence.len(), item_group.len())?;

        // Ensure each item is purchased exactly once (either full price OR via one promotion).
        //
        // This prevents items from being:
        // - Omitted from the checkout entirely
        // - Selected by multiple promotions simultaneously
        //
        // Example: If "20% off" and "Buy-one-get-one" both target the same item,
        // the solver must choose one or neither, never both.
        for (item_idx, z_i) in item_presence.iter().copied().enumerate() {
            let constraint_expr =
                promotion_instances.add_item_presence_term(Expression::from(z_i), item_idx);

            model = model.with(constraint_expr.eq(1));
        }

        // Add constraints for all promotions
        model = promotion_instances.add_constraints(model, item_group);

        let solution = model.solve()?;

        // Translate the solver's decisions back into business terms: which items got
        // discounted, by which promotions, and what their final prices are.
        let mut used_items: ItemUsageFlags = smallvec![false; item_group.len()];
        let mut total = Money::from_minor(0, item_group.currency());
        let mut promotion_applications: SmallVec<[PromotionApplication<'group>; 10]> =
            SmallVec::new();
        let mut next_bundle_id: usize = 0;
        let mut affected_items: ItemIndexList = ItemIndexList::new();

        // Extract which items each promotion selected and their discounted prices
        for instance in promotion_instances.iter() {
            let apps =
                instance.calculate_item_applications(&solution, item_group, &mut next_bundle_id);

            let (applied_items, updated_used_items, updated_total) =
                apply_promotion_applications(item_group.len(), used_items, total, &apps)?;

            affected_items.extend(applied_items);
            used_items = updated_used_items;
            total = updated_total;

            promotion_applications.extend(apps);
        }

        let (unaffected_items, total) =
            collect_full_price_items(item_group, &solution, &item_presence, used_items, total)?;

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
/// Returns a [`SolverError`] if adding a full-price item to the total fails.
fn build_presence_variables_and_objective(
    item_group: &ItemGroup<'_>,
    pb: &mut ProblemVariables,
) -> Result<(SmallVec<[Variable; 10]>, Expression), SolverError> {
    // Each item must be present in the solution whether a promotion is applied or not.
    // Create a presence variable for each item representing the full-price option.
    let presence: SmallVec<[Variable; 10]> = (0..item_group.len())
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
        .zip(item_group.iter())
        .try_for_each(|(var, item)| -> Result<(), SolverError> {
            let minor_units = item.price().to_minor_units();

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
/// Returns a [`SolverError`] if any item in the group contains a Money amount in minor units
/// that cannot be represented exactly as a solver coefficient.
fn collect_full_price_items<'group>(
    item_group: &'group ItemGroup<'_>,
    solution: &impl Solution,
    z: &[Variable],
    mut used_items: ItemUsageFlags,
    mut total: Money<'group, Currency>,
) -> Result<FullPriceState<'group>, SolverError> {
    let mut unaffected_items = SmallVec::new();

    // Any item that wasn't claimed by a promotion is treated as an unaffected
    // full-price item and contributes its full price to the total.
    for (item_idx, (var, item)) in z.iter().copied().zip(item_group.iter()).enumerate() {
        let Some(used) = used_items.get_mut(item_idx) else {
            continue;
        };

        // `var` is a binary decision variable; the solver return floats, so treat values
        // greater than 0.5 as "selected" (i.e. 1) to tolerate tiny numerical noise.
        if solution.value(var) > BINARY_THRESHOLD && !*used {
            // Add the item to the list of unaffected items.
            unaffected_items.push(item_idx);

            // Add the item's full price to the result total.
            total = total.add(Money::from_minor(
                item.price().to_minor_units(),
                item_group.currency(),
            ))?;

            // Mark the item as used.
            *used = true;
        }
    }

    Ok((unaffected_items, total))
}

/// Apply promotion applications to track affected items and accumulate total.
///
/// This function processes [`PromotionApplication`] instances, ensuring each item
/// group position is used at most once (via `used_items`), records affected item indices,
/// and adds the final prices to `total`.
///
/// Note: `used_items` is indexed by item group position (item index). This keeps it
/// aligned with other per-variable/per-position arrays used by the ILP formulation.
///
/// # Errors
///
/// Returns a [`SolverError`] if adding a final price to `total` fails.
fn apply_promotion_applications<'a>(
    item_count: usize,
    mut used_items: ItemUsageFlags,
    mut total: Money<'a, Currency>,
    applications: &[PromotionApplication<'a>],
) -> Result<AppliedPromotionState<'a>, SolverError> {
    // The indexes of items that are being affected by promotions
    let mut affected_items: ItemIndexList = ItemIndexList::new();

    for app in applications {
        if app.item_idx >= item_count {
            continue;
        }

        // If this position is already claimed, skip it to avoid double-counting.
        if let Some(used) = used_items.get(app.item_idx)
            && *used
        {
            continue;
        }

        // Commit to consuming this item position as soon as we apply its discount.
        if let Some(used) = used_items.get_mut(app.item_idx) {
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
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::Discount,
        items::{Item, groups::ItemGroup},
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

    fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
        let currency = items
            .first()
            .map_or(iso::GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[test]
    fn solver_returns_all_items_full_price_with_no_promotions() -> TestResult {
        let items = test_items();

        let subtotal = items
            .iter()
            .map(|item| item.price().to_minor_units())
            .sum::<i64>();

        let item_group = item_group_from_items(items);

        let result = ILPSolver::solve(&[], &item_group)?;

        assert_eq!(subtotal, result.total.to_minor_units());
        assert_eq!(0, result.affected_items.len());
        assert_eq!(3, result.unaffected_items.len());
        assert!(result.promotion_applications.is_empty());

        Ok(())
    }

    #[test]
    fn apply_applications_skips_pre_used_positions() -> TestResult {
        // `apply_applications` uses `used_items` (indexed by item group position)
        // to prevent an item from being claimed by more than one promotion.
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
            apply_promotion_applications(3, used_items, total, &applications)?;

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
            apply_promotion_applications(2, used_items, total, &applications)?;

        assert!(affected_items.is_empty());
        assert_eq!(total.to_minor_units(), 0);

        Ok(())
    }

    #[test]
    fn solver_applies_percentage_discount_to_tagged_items() -> TestResult {
        let items = test_items_with_tags();
        let item_group = item_group_from_items(items);

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            Discount::PercentageOffBundleTotal(Percentage::from(0.25)),
        ))];

        let result = ILPSolver::solve(&promotions, &item_group)?;

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
        let item_group = item_group_from_items(items);

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &item_group)?;

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
        let item_group = item_group_from_items(items);

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["missing"]),
            Discount::PercentageOffBundleTotal(Percentage::from(0.25)),
        ))];

        let result = ILPSolver::solve(&promotions, &item_group)?;

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
        let item_group = item_group_from_items(items);

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(400, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &item_group)?;

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
        let item_group = item_group_from_items(items);

        let promotions = [Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ))];

        let result = ILPSolver::solve(&promotions, &item_group)?;

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
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), iso::GBP);

        let result = ILPSolver::solve(&[], &item_group)?;

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
        let item_group = item_group_from_items(items);

        let mut pb = ProblemVariables::new();
        let (z, objective) = build_presence_variables_and_objective(&item_group, &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let expected = 600.0_f64;
        let actual = solution.eval(&objective);

        assert!((actual - expected).abs() <= f64::EPSILON);

        Ok(())
    }

    #[test]
    fn unaffected_items_collection_skips_pre_used_items() -> TestResult {
        let items = test_items();
        let item_group = item_group_from_items(items);

        let mut pb = ProblemVariables::new();
        let (z, _objective) = build_presence_variables_and_objective(&item_group, &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let mut used_items: ItemUsageFlags = smallvec![false; 3];
        used_items[1] = true; // pretend item 1 was claimed by a promotion

        let total = Money::from_minor(0, item_group.currency());

        let (unaffected_items, total) =
            collect_full_price_items(&item_group, &solution, &z, used_items, total)?;

        assert_eq!(unaffected_items.as_slice(), &[0, 2]);
        assert_eq!(total.to_minor_units(), 400);

        Ok(())
    }

    #[test]
    fn unaffected_items_collection_skips_missing_usage_entries() -> TestResult {
        let items = test_items();
        let item_group = item_group_from_items(items);

        let mut pb = ProblemVariables::new();
        let (z, _objective) = build_presence_variables_and_objective(&item_group, &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        // Deliberately shorter than the item group to exercise the guard path.
        let used_items: ItemUsageFlags = smallvec![false; 1];
        let total = Money::from_minor(0, item_group.currency());

        let (unaffected_items, total) =
            collect_full_price_items(&item_group, &solution, &z, used_items, total)?;

        assert_eq!(unaffected_items.as_slice(), &[0]);
        assert_eq!(total.to_minor_units(), 100);

        Ok(())
    }
}
