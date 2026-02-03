//! ILP Promotions

use std::fmt;

use good_lp::{Expression, Solution, SolverModel};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    items::groups::ItemGroup,
    promotions::{Promotion, PromotionKey, applications::PromotionApplication},
    solvers::{SolverError, ilp::state::ILPState},
};

mod simple_discount;

/// Collection of promotion instances for a solve operation
#[derive(Debug)]
pub struct PromotionInstances<'a> {
    instances: SmallVec<[PromotionInstance<'a>; 5]>,
}

impl<'a> PromotionInstances<'a> {
    /// Create Promotion Instances from a slice of promotions
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if any applicable promotion fails to add variables.
    pub fn from_promotions(
        promotions: &'a [Promotion<'_>],
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
    ) -> Result<Self, SolverError> {
        let mut instances = SmallVec::new();

        for promotion in promotions {
            let instance = PromotionInstance::new(promotion, item_group, state)?;
            instances.push(instance);
        }

        Ok(Self { instances })
    }

    /// Iterate over instances
    pub fn iter(&self) -> impl Iterator<Item = &PromotionInstance<'a>> {
        self.instances.iter()
    }

    /// Add presence terms for all promotion instances to the item constraint
    ///
    /// Contributes each promotion's decision variables for the given item to the
    /// presence/exclusivity constraint expression.
    pub fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for instance in &self.instances {
            updated_expr = instance.add_item_presence_term(updated_expr, item_idx);
        }

        updated_expr
    }

    /// Add constraints for all promotion instances
    pub fn add_constraints<S: SolverModel>(&self, mut model: S, item_group: &ItemGroup<'_>) -> S {
        for instance in &self.instances {
            model = instance.add_constraints(model, item_group);
        }

        model
    }
}

/// A promotion instance that pairs a promotion with its solver variables
#[derive(Debug)]
pub struct PromotionInstance<'a> {
    /// The promotion being solved
    promotion: &'a Promotion<'a>,

    /// The solver variables for this promotion instance
    vars: Box<dyn PromotionVars>,
}

impl<'a> PromotionInstance<'a> {
    /// Create a new promotion instance (promotion & its solver variables).
    ///
    /// If the promotion cannot apply to the provided item group, we can avoid adding
    /// any decision variables and instead store a no-op one. This keeps the global
    /// model smaller and prevents inapplicable promotions from contributing to the objective
    /// expression (`cost`), or any per-item usage sums used for the exclusivity constraints.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if the promotion fails to add variables (for example, due to
    /// invalid indices, discount errors, or non-representable coefficients).
    pub fn new(
        promotion: &'a Promotion<'a>,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
    ) -> Result<Self, SolverError> {
        let vars = match (promotion, promotion.is_applicable(item_group)) {
            (Promotion::SimpleDiscount(simple_discount), true) => {
                simple_discount.add_variables(item_group, state)?
            }
            (_promotion, false) => Box::new(NoopPromotionVars) as Box<dyn PromotionVars>,
        };

        Ok(Self { promotion, vars })
    }

    /// Contribute this promotion's presence term for `item_idx`.
    ///
    /// This is called while building the per-item presence/exclusivity constraint that
    /// enforces each item is either at full price or used by exactly one promotion.
    pub fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        self.vars.add_item_presence_term(expr, item_idx)
    }

    /// Add promotion-specific constraints for this instance.
    ///
    /// This delegates to the concrete promotion type, passing the instance's variable adapter
    /// (`self.vars`). For a no-op instance, this should behave like an identity and return the
    /// model unchanged.
    fn add_constraints<S: SolverModel>(&self, model: S, item_group: &ItemGroup<'_>) -> S {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => {
                simple.add_constraints(model, self.vars.as_ref(), item_group)
            }
        }
    }

    /// Post-solve interpretation for this promotion instance.
    ///
    /// Reads the solved variable values to determine which items this promotion selected and
    /// returns the per-item `(original_minor, discounted_minor)` pairs for those items.
    ///
    /// # Errors
    ///
    /// Returns `SolverError` if a selected item index is invalid (missing from the item group),
    /// or if the discount for a selected item cannot be computed.
    pub fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => {
                simple.calculate_item_discounts(solution, self.vars.as_ref(), item_group)
            }
        }
    }

    /// Post-solve interpretation returning full promotion applications.
    ///
    /// Reads the solved variable values to determine which items this promotion selected and
    /// returns [`PromotionApplication`] instances with bundle IDs and price details.
    pub fn calculate_item_applications<'group>(
        &self,
        solution: &dyn Solution,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> SmallVec<[PromotionApplication<'group>; 10]> {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => simple.calculate_item_applications(
                self.promotion.key(),
                solution,
                self.vars.as_ref(),
                item_group,
                next_bundle_id,
            ),
        }
    }
}

/// Per-promotion solver variables.
///
/// A promotion can expose a set of per-item decision variables (typically binary) that indicate
/// whether the promotion uses a given item.
pub trait PromotionVars: fmt::Debug + Send + Sync {
    /// Add this promotion's presence term for `item_idx` to the constraint expression.
    ///
    /// Contributes this promotion's per-item decision variable(s) to the presence/exclusivity
    /// constraint being built for a single item. The constraint ensures each item appears in
    /// the solution exactly once (either at full price or selected by one promotion).
    fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression;

    /// Return `true` if this promotion selected `item_idx` in the solved model.
    fn is_item_selected(&self, solution: &dyn Solution, item_idx: usize) -> bool;
}

/// Makes a [`Promotion`] usable by the ILP solver.
///
/// Implementations are responsible for translating a promotion into:
///
/// - A set of per-item decision variables,
/// - Any additional promotion-specific constraints, and
/// - A post-solve interpretation step that turns the chosen variables into concrete
///   per-item discounts.
///
/// Expected lifecycle for a promotion instance within a solve:
///
/// 1. [`ILPPromotion::is_applicable`] is used as a cheap pre-filter.
/// 2. [`ILPPromotion::add_variables`] creates the decision variables and contributes
///    to the objective (`cost`).
/// 3. [`ILPPromotion::add_constraints`] optionally adds constraints that link those
///    variables to each other and/or to the item group.
/// 4. After solving, [`ILPPromotion::calculate_item_discounts`] extracts the final
///    per-item discount amounts from the solution.
///
/// Notes:
/// - Implementations should be deterministic for a given item group input; ILP
///   solutions are already sensitive to tiny numeric differences.
/// - Promotions that are not applicable are modeled as a no-op by [`PromotionInstance::new`].
///   Implementations should therefore treat an "empty" `vars` as "selects nothing" and avoid
///   introducing constraints that would accidentally affect the global model.
pub trait ILPPromotion: Send + Sync {
    /// Return whether this promotion _might_ apply to the given item group.
    ///
    /// This is used as a fast pre-check to avoid allocating variables/constraints for
    /// promotions that cannot possibly contribute to the solution.
    ///
    /// Prefer no false negatives, returning `true` when unsure is safe (it just creates extra
    /// variables), while returning `false` can prevent a valid discount from ever being
    /// considered by the solver.
    ///
    /// Avoid expensive computations that can be deferred until
    /// [`ILPPromotion::add_variables`] / [`ILPPromotion::calculate_item_discounts`].
    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool;

    /// Create per-item binary variables and add them to the objective expression.
    ///
    /// Each eligible item gets a decision variable indicating whether this promotion applies.
    /// The discounted price contribution should be added to `cost` so the solver can trade off
    /// selecting discounts against other promotions under global constraints.
    ///
    /// Missing items should be surfaced to callers via [`SolverError::ItemGroup`].
    /// Discount calculation errors should be surfaced to callers via [`SolverError::Discount`].
    /// If a discounted minor unit amount cannot be represented exactly as a solver coefficient,
    /// return [`SolverError::MinorUnitsNotRepresentable`].
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if variable creation fails (e.g. invalid item index, discount error,
    /// or non-representable coefficients).
    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
    ) -> Result<Box<dyn PromotionVars>, SolverError>;

    /// Add promotion-specific constraints to the solver model.
    ///
    /// This is where promotions express structure that can't be represented by the
    /// global "each item used at most once" constraints (e.g., minimum quantity,
    /// bundle requirements, mutual exclusion between items, etc.).
    ///
    /// `vars` is whatever was returned from [`ILPPromotion::add_variables`]. When
    /// this promotion is not applicable, `vars` will be a no-op implementation that
    /// never selects an item; in that case, this method should just return `model`
    /// unchanged.
    fn add_constraints<S: SolverModel>(
        &self,
        model: S,
        vars: &dyn PromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> S;

    /// Calculate per-item discounts chosen for this promotion.
    ///
    /// This is the "interpretation" step: it inspects the solved decision variables
    /// and returns a mapping from item group index to `(original_minor, discounted_minor)`.
    ///
    /// Requirements:
    /// - Only include items that are selected in `solution` according to `vars`.
    /// - Values must be in minor units and must be consistent with the objective
    ///   coefficients used in [`ILPPromotion::add_variables`].
    /// - The returned map is later applied by the solver; incorrect values will
    ///   directly translate into incorrect totals.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if a selected item index is invalid (missing from the item group),
    /// or if the discount for a selected item cannot be computed.
    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError>;

    /// Calculate promotion applications for selected items.
    ///
    /// Similar to [`ILPPromotion::calculate_item_discounts`], but returns full
    /// [`PromotionApplication`] instances with bundle IDs and `Money` values.
    ///
    /// Each promotion type determines its own bundling semantics:
    /// - `SimpleDiscount`: Each item gets its own unique `bundle_id` (no bundling).
    /// - Future bundle promotions: Items in the same deal share one `bundle_id`.
    ///
    /// The `next_bundle_id` counter is passed mutably and should be incremented
    /// for each new bundle created. This ensures unique IDs across all promotions.
    fn calculate_item_applications<'group>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> SmallVec<[PromotionApplication<'group>; 10]>;
}

/// No-op promotion variables implementation
#[derive(Debug)]
struct NoopPromotionVars;

impl PromotionVars for NoopPromotionVars {
    fn add_item_presence_term(&self, expr: Expression, _item_idx: usize) -> Expression {
        expr
    }

    fn is_item_selected(&self, _solution: &dyn Solution, _item_idx: usize) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, ProblemVariables, Solution, SolutionStatus, Variable};
    use rusty_money::{Money, iso};
    use testresult::TestResult;

    use crate::{
        discounts::Discount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{Promotion, PromotionKey, simple_discount::SimpleDiscount},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    #[derive(Debug)]
    struct SelectAllSolution;

    impl Solution for SelectAllSolution {
        fn status(&self) -> SolutionStatus {
            SolutionStatus::Optimal
        }

        fn value(&self, _variable: Variable) -> f64 {
            1.0
        }
    }

    fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
        let currency = items
            .first()
            .map_or(iso::GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[test]
    fn promotion_instance_calculates_item_discounts_via_inner_promotion() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];
        let item_group = item_group_from_items(items);

        let promotion = Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ));

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let instance = PromotionInstance::new(&promotion, &item_group, &mut state)?;

        let discounts = instance.calculate_item_discounts(&SelectAllSolution, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

        Ok(())
    }
}
