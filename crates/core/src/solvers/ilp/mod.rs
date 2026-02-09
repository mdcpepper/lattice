//! ILP Solver

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, Variable, variable};
use rusty_money::{Money, iso::Currency};
use smallvec::{SmallVec, smallvec};

#[cfg(feature = "solver-highs")]
use good_lp::solvers::highs::highs as default_solver;
#[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
use good_lp::solvers::microlp::microlp as default_solver;

use crate::solvers::ilp::state::{ConstraintRelation, ILPConstraint};
use crate::{
    items::groups::ItemGroup,
    promotions::{Promotion, applications::PromotionApplication},
    solvers::{Solver, SolverError, SolverResult, ilp::promotions::PromotionInstances},
};

pub mod observer;
pub(crate) mod promotions;
pub mod renderers;
pub(crate) mod state;

pub use observer::{ILPObserver, NoopObserver};
pub use promotions::{ILPPromotion, ILPPromotionVars, PromotionVars, i64_to_f64_exact};
pub use state::ILPState;

/// Binary threshold for determining truthiness
pub const BINARY_THRESHOLD: f64 = 0.5;

type ItemIndexList = SmallVec<[usize; 10]>;
type ItemUsageFlags = SmallVec<[bool; 10]>;
type AppliedPromotionState<'a> = (ItemIndexList, ItemUsageFlags, Money<'a, Currency>);
type FullPriceState<'a> = (ItemIndexList, Money<'a, Currency>);

/// Solver using Integer Linear Programming (ILP)
#[derive(Debug)]
pub struct ILPSolver;

impl ILPSolver {
    /// Solve with an observer for capturing the ILP formulation.
    ///
    /// This method allows passing an observer that will receive callbacks as the
    /// ILP problem is constructed, enabling capture of variables, constraints, and
    /// the complete mathematical formulation.
    ///
    /// # Parameters
    ///
    /// - `promotions`: The promotions to apply
    /// - `item_group`: The items to optimize pricing for
    /// - `observer`: Observer to capture formulation
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if the solver encounters an error.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::solvers::ilp::{ILPSolver, renderers::typst::TypstRenderer};
    /// use std::path::PathBuf;
    /// # use lattice::{fixtures::Fixture, items::groups::ItemGroup};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let fixture = Fixture::from_set("example_direct_discounts")?;
    /// # let basket = fixture.basket(Some(10))?;
    /// # let item_group = ItemGroup::from(&basket);
    /// # let promotions = fixture.promotions();
    ///
    /// let mut renderer = TypstRenderer::new(PathBuf::from("formulation.typ"));
    /// let result = ILPSolver::solve_with_observer(promotions, &item_group, &mut renderer)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn solve_with_observer<'b>(
        promotions: &[Promotion<'_>],
        item_group: &ItemGroup<'b>,
        observer: &mut dyn ILPObserver,
    ) -> Result<SolverResult<'b>, SolverError> {
        let promotion_refs: SmallVec<[&dyn ILPPromotion; 5]> =
            promotions.iter().map(AsRef::as_ref).collect();

        Self::solve_internal(&promotion_refs, item_group, observer)
    }

    /// Internal solve implementation that supports an observer.
    fn solve_internal<'b>(
        promotions: &[&dyn ILPPromotion],
        item_group: &ItemGroup<'b>,
        observer: &mut dyn ILPObserver,
    ) -> Result<SolverResult<'b>, SolverError> {
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
        let mut state = ILPState::with_presence_variables_and_observer(item_group, observer)?;

        let promotion_instances =
            PromotionInstances::from_promotions(promotions, item_group, &mut state, observer)?;

        // Extract state for model creation
        let (pb, cost, item_presence, constraints) = state.into_parts_with_constraints();

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

            // Notify observer before adding constraint
            observer.on_exclusivity_constraint(item_idx, &constraint_expr);

            model = model.with(constraint_expr.eq(1));
        }

        // Add all recorded promotion constraints.
        model = apply_recorded_constraints(model, constraints);

        let solution = model.solve()?;

        // Translate the solver's decisions back into business terms: which items got
        // discounted, by which promotions, and what their final prices are.
        let mut used_items: ItemUsageFlags = smallvec![false; item_group.len()];
        let mut total = Money::from_minor(0, item_group.currency());
        let mut promotion_applications: SmallVec<[PromotionApplication<'b>; 10]> = SmallVec::new();
        let mut next_bundle_id: usize = 0;
        let mut affected_items: ItemIndexList = ItemIndexList::new();

        // Extract which items each promotion selected and their discounted prices
        for instance in promotion_instances.iter() {
            let apps =
                instance.calculate_item_applications(&solution, item_group, &mut next_bundle_id)?;

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

impl Solver for ILPSolver {
    fn solve<'b>(
        promotions: &[Promotion<'_>],
        item_group: &ItemGroup<'b>,
    ) -> Result<SolverResult<'b>, SolverError> {
        let mut observer = NoopObserver;

        let promotion_refs: SmallVec<[&dyn ILPPromotion; 5]> =
            promotions.iter().map(AsRef::as_ref).collect();

        Self::solve_internal(&promotion_refs, item_group, &mut observer)
    }
}

fn apply_recorded_constraints<S: SolverModel>(mut model: S, constraints: Vec<ILPConstraint>) -> S {
    for constraint in constraints {
        model = match constraint.relation {
            ConstraintRelation::Eq => model.with(constraint.lhs.eq(constraint.rhs)),
            ConstraintRelation::Leq => model.with(constraint.lhs.leq(constraint.rhs)),
            ConstraintRelation::Geq => model.with(constraint.lhs.geq(constraint.rhs)),
        };
    }

    model
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
fn build_presence_variables_and_objective<O: ILPObserver + ?Sized>(
    item_group: &ItemGroup<'_>,
    pb: &mut ProblemVariables,
    observer: &mut O,
) -> Result<(SmallVec<[Variable; 10]>, Expression), SolverError> {
    // Each item must be present in the solution whether participating in a promotion or not.
    // Create a presence variable for each item representing the full-price option.
    let mut presence: SmallVec<[Variable; 10]> = SmallVec::new();

    // Create expression for total cost. This is what we are trying to minimise.
    let mut cost = Expression::default();

    // Add the full-price option for each item to the objective.
    // These are the baseline costs if no promotions are applied. When we add promotion
    // variables later, they'll offer alternative (discounted) costs. The solver will
    // compare full-price vs. discounted options and choose what minimizes the total.
    for (item_idx, item) in item_group.iter().enumerate() {
        let var = pb.add(variable().binary());
        let minor_units = item.price().to_minor_units();

        // `good_lp` stores coefficients as `f64`. Only integers with absolute value <= 2^53
        // can be represented exactly in an IEEE-754 `f64` mantissa; enforce that via a
        // round-trip check so we never silently change the objective.
        let coeff = i64_to_f64_exact(minor_units)
            .ok_or(SolverError::MinorUnitsNotRepresentable(minor_units))?;

        cost += var * coeff;
        presence.push(var);

        observer.on_presence_variable(item_idx, var, minor_units);
        observer.on_objective_term(var, coeff);
    }

    Ok((presence, cost))
}

/// Collect unaffected items and their total price.
///
/// # Errors
///
/// Returns a [`SolverError`] if any item in the group contains a Money amount in minor units
/// that cannot be represented exactly as a solver coefficient.
fn collect_full_price_items<'b>(
    item_group: &ItemGroup<'b>,
    solution: &impl Solution,
    z: &[Variable],
    used_items: ItemUsageFlags,
    total: Money<'b, Currency>,
) -> Result<FullPriceState<'b>, SolverError> {
    let mut unaffected_items = SmallVec::new();
    let mut used_items = used_items;
    let mut total = total;

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
fn apply_promotion_applications<'b>(
    item_count: usize,
    used_items: ItemUsageFlags,
    total: Money<'b, Currency>,
    applications: &[PromotionApplication<'b>],
) -> Result<AppliedPromotionState<'b>, SolverError> {
    // The indexes of items that are being affected by promotions
    let mut affected_items: ItemIndexList = ItemIndexList::new();

    let mut used_items = used_items;
    let mut total = total;

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use decimal_percentage::Percentage;
    use rustc_hash::FxHashMap;
    use rusty_money::iso::GBP;
    use smallvec::{SmallVec, smallvec};
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            PromotionKey, applications::PromotionApplication, budget::PromotionBudget,
            types::DirectDiscountPromotion,
        },
        solvers::ilp::promotions::{ILPPromotion, ILPPromotionVars, PromotionVars},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
        ]
    }

    fn test_items_with_tags<'a>() -> [Item<'a>; 3] {
        [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["a"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["b"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, GBP),
                StringTagCollection::from_strs(&["a", "b"]),
            ),
        ]
    }

    fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
        let currency = items.first().map_or(GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[derive(Debug)]
    struct TestCustomPromotion {
        key: PromotionKey,
        final_minor: i64,
    }

    #[derive(Debug)]
    struct TestCustomPromotionVars {
        key: PromotionKey,
        final_minor: i64,
        item_participation: SmallVec<[(usize, Variable); 10]>,
    }

    impl ILPPromotionVars for TestCustomPromotionVars {
        fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
            let mut updated = expr;

            for &(idx, var) in &self.item_participation {
                if idx == item_idx {
                    updated += var;
                }
            }

            updated
        }

        fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
            self.item_participation
                .iter()
                .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
        }

        fn add_constraints(
            &self,
            _promotion_key: PromotionKey,
            _item_group: &ItemGroup<'_>,
            state: &mut ILPState,
            observer: &mut dyn ILPObserver,
        ) -> Result<(), SolverError> {
            let expr: Expression = self.item_participation.iter().map(|(_, var)| *var).sum();
            observer.on_promotion_constraint(self.key, "test custom limit", &expr, "<=", 1.0);
            state.add_leq_constraint(expr, 1.0);

            Ok(())
        }

        fn calculate_item_discounts(
            &self,
            solution: &dyn Solution,
            item_group: &ItemGroup<'_>,
        ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
            let mut discounts = FxHashMap::default();

            for (item_idx, item) in item_group.iter().enumerate() {
                if self.is_item_participating(solution, item_idx) {
                    discounts.insert(
                        item_idx,
                        (item.price().to_minor_units(), self.final_minor.max(0)),
                    );
                }
            }

            Ok(discounts)
        }

        fn calculate_item_applications<'b>(
            &self,
            promotion_key: PromotionKey,
            solution: &dyn Solution,
            item_group: &ItemGroup<'b>,
            next_bundle_id: &mut usize,
        ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
            let mut applications = SmallVec::new();
            let currency = item_group.currency();

            for item_idx in 0..item_group.len() {
                let item = item_group.get_item(item_idx)?;

                if !self.is_item_participating(solution, item_idx) {
                    continue;
                }

                let bundle_id = *next_bundle_id;
                *next_bundle_id += 1;

                applications.push(PromotionApplication {
                    promotion_key,
                    item_idx,
                    bundle_id,
                    original_price: *item.price(),
                    final_price: Money::from_minor(self.final_minor.max(0), currency),
                });
            }

            Ok(applications)
        }
    }

    impl ILPPromotion for TestCustomPromotion {
        fn key(&self) -> PromotionKey {
            self.key
        }

        fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
            !item_group.is_empty()
        }

        fn add_variables(
            &self,
            item_group: &ItemGroup<'_>,
            state: &mut ILPState,
            observer: &mut dyn ILPObserver,
        ) -> Result<PromotionVars, SolverError> {
            let mut item_participation = SmallVec::new();
            let coeff = i64_to_f64_exact(self.final_minor)
                .ok_or(SolverError::MinorUnitsNotRepresentable(self.final_minor))?;

            for (item_idx, _item) in item_group.iter().enumerate() {
                let var = state.problem_variables_mut().add(variable().binary());
                item_participation.push((item_idx, var));
                state.add_to_objective(var, coeff);
                observer.on_promotion_variable(
                    self.key,
                    item_idx,
                    var,
                    self.final_minor,
                    Some("test"),
                );
                observer.on_objective_term(var, coeff);
            }

            Ok(Box::new(TestCustomPromotionVars {
                key: self.key,
                final_minor: self.final_minor,
                item_participation,
            }))
        }
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
        let total = Money::from_minor(0, GBP);

        // Provide an application for item index 1, but the corresponding position is pre-used above.
        let applications = [PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 1,
            bundle_id: 0,
            original_price: Money::from_minor(200, GBP),
            final_price: Money::from_minor(150, GBP),
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
        let total = Money::from_minor(0, GBP);

        let applications = [PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 99,
            bundle_id: 0,
            original_price: Money::from_minor(200, GBP),
            final_price: Money::from_minor(150, GBP),
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

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
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

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
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

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["missing"]),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
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

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(400, GBP)),
            PromotionBudget::unlimited(),
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

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
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
        assert_eq!(first_app.original_price, Money::from_minor(100, GBP));
        assert_eq!(first_app.final_price, Money::from_minor(50, GBP));

        // Second application (item 2)
        let second_app = sorted_apps.get(1);
        assert!(second_app.is_some());

        let second_app = second_app.ok_or("Expected second application")?;
        assert_eq!(second_app.item_idx, 2);
        assert_eq!(second_app.original_price, Money::from_minor(300, GBP));
        assert_eq!(second_app.final_price, Money::from_minor(50, GBP));

        // Each item should have a unique bundle_id (DirectDiscountPromotion doesn't bundle)
        assert_ne!(first_app.bundle_id, second_app.bundle_id);

        Ok(())
    }

    #[test]
    fn solver_with_no_items_returns_empty_result() -> TestResult {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

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
        let mut observer = NoopObserver;
        let (z, objective) =
            build_presence_variables_and_objective(&item_group, &mut pb, &mut observer)?;

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
        let mut observer = NoopObserver;
        let (z, _objective) =
            build_presence_variables_and_objective(&item_group, &mut pb, &mut observer)?;

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
        let mut observer = NoopObserver;
        let (z, _objective) =
            build_presence_variables_and_objective(&item_group, &mut pb, &mut observer)?;

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

    #[test]
    fn solve_with_observer_noop_produces_same_results_as_trait_solve() -> TestResult {
        let items = test_items_with_tags();
        let item_group = item_group_from_items(items);

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        ))];

        // Solve using trait method
        let result1 = ILPSolver::solve(&promotions, &item_group)?;

        // Solve using solve_with_observer(NoopObserver)
        let mut observer = NoopObserver;
        let result2 = ILPSolver::solve_with_observer(&promotions, &item_group, &mut observer)?;

        // Results should be identical
        assert_eq!(result1.total, result2.total);
        assert_eq!(result1.affected_items, result2.affected_items);
        assert_eq!(result1.unaffected_items, result2.unaffected_items);
        assert_eq!(
            result1.promotion_applications.len(),
            result2.promotion_applications.len()
        );

        Ok(())
    }

    #[test]
    fn solve_accepts_custom_promotion_trait_objects() -> TestResult {
        let items = test_items();
        let item_group = item_group_from_items(items);
        let promotion = crate::promotions::promotion(TestCustomPromotion {
            key: PromotionKey::default(),
            final_minor: 1,
        });
        let promotions = [promotion];

        let result = ILPSolver::solve(&promotions, &item_group)?;

        assert_eq!(result.total.to_minor_units(), 301);
        assert_eq!(result.affected_items.as_slice(), &[2]);
        assert_eq!(result.unaffected_items.as_slice(), &[0, 1]);
        assert_eq!(result.promotion_applications.len(), 1);
        assert_eq!(result.promotion_applications[0].item_idx, 2);
        assert_eq!(
            result.promotion_applications[0].final_price,
            Money::from_minor(1, GBP)
        );

        Ok(())
    }

    #[test]
    fn solve_with_observer_calls_observer_methods() -> TestResult {
        use std::sync::{Arc, Mutex};

        use good_lp::Expression;

        use crate::promotions::PromotionKey;

        // Mock observer that tracks calls
        #[derive(Default)]
        #[expect(
            clippy::struct_field_names,
            reason = "Test observer tracking call counts"
        )]
        struct MockObserver {
            presence_vars_count: Arc<Mutex<usize>>,
            promotion_vars_count: Arc<Mutex<usize>>,
            exclusivity_constraints_count: Arc<Mutex<usize>>,
        }

        impl ILPObserver for MockObserver {
            fn on_presence_variable(&mut self, _: usize, _: Variable, _: i64) {
                if let Ok(mut count) = self.presence_vars_count.lock() {
                    *count += 1;
                }
            }

            fn on_promotion_variable(
                &mut self,
                _: PromotionKey,
                _: usize,
                _: Variable,
                _: i64,
                _: Option<&str>,
            ) {
                if let Ok(mut count) = self.promotion_vars_count.lock() {
                    *count += 1;
                }
            }

            fn on_exclusivity_constraint(&mut self, _: usize, _: &Expression) {
                if let Ok(mut count) = self.exclusivity_constraints_count.lock() {
                    *count += 1;
                }
            }

            fn on_promotion_constraint(
                &mut self,
                _: PromotionKey,
                _: &str,
                _: &Expression,
                _: &str,
                _: f64,
            ) {
                // No-op for this test
            }
        }

        let items = test_items_with_tags();
        let item_group = item_group_from_items(items);

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        ))];

        let mut observer = MockObserver::default();
        let presence_count = Arc::clone(&observer.presence_vars_count);
        let promotion_count = Arc::clone(&observer.promotion_vars_count);
        let exclusivity_count = Arc::clone(&observer.exclusivity_constraints_count);

        let _result = ILPSolver::solve_with_observer(&promotions, &item_group, &mut observer)?;

        // Verify observer methods were called
        assert_eq!(
            presence_count.lock().map_or(0, |c| *c),
            3,
            "Expected 3 presence variables (one per item)"
        );
        assert!(
            promotion_count.lock().map_or(0, |c| *c) > 0,
            "Expected promotion variables to be created"
        );
        assert_eq!(
            exclusivity_count.lock().map_or(0, |c| *c),
            3,
            "Expected 3 exclusivity constraints (one per item)"
        );

        Ok(())
    }

    #[test]
    fn solve_with_observer_does_not_affect_results() -> TestResult {
        use std::sync::{Arc, Mutex};

        use good_lp::Expression;

        use crate::promotions::PromotionKey;

        // Observer that tracks everything
        #[derive(Default)]
        struct TrackingObserver {
            called: Arc<Mutex<bool>>,
        }

        impl ILPObserver for TrackingObserver {
            fn on_presence_variable(&mut self, _: usize, _: Variable, _: i64) {
                if let Ok(mut called) = self.called.lock() {
                    *called = true;
                }
            }

            fn on_promotion_variable(
                &mut self,
                _: PromotionKey,
                _: usize,
                _: Variable,
                _: i64,
                _: Option<&str>,
            ) {
            }

            fn on_exclusivity_constraint(&mut self, _: usize, _: &Expression) {}

            fn on_promotion_constraint(
                &mut self,
                _: PromotionKey,
                _: &str,
                _: &Expression,
                _: &str,
                _: f64,
            ) {
            }
        }

        let items = test_items_with_tags();
        let item_group = item_group_from_items(items);

        let promotions = [crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ))];

        // Solve without observer
        let result_no_observer = ILPSolver::solve(&promotions, &item_group)?;

        // Solve with observer
        let mut observer = TrackingObserver::default();
        let called = Arc::clone(&observer.called);
        let result_with_observer =
            ILPSolver::solve_with_observer(&promotions, &item_group, &mut observer)?;

        // Verify observer was called
        assert!(
            called.lock().is_ok_and(|c| *c),
            "Observer should have been called"
        );

        // Verify results are identical
        assert_eq!(result_no_observer.total, result_with_observer.total);
        assert_eq!(
            result_no_observer.affected_items,
            result_with_observer.affected_items
        );
        assert_eq!(
            result_no_observer.unaffected_items,
            result_with_observer.unaffected_items
        );
        assert_eq!(
            result_no_observer.promotion_applications.len(),
            result_with_observer.promotion_applications.len()
        );

        Ok(())
    }
}
