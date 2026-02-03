//! ILP Promotions

use std::fmt;

use good_lp::{Expression, ProblemVariables, Solution, SolverModel};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    basket::Basket,
    promotions::{Promotion, PromotionKey, applications::PromotionApplication},
    solvers::SolverError,
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
        basket: &'a Basket<'a>,
        items: &[usize],
        pb: &mut ProblemVariables,
        cost: &mut Expression,
    ) -> Result<Self, SolverError> {
        let mut instances = SmallVec::new();

        for promotion in promotions {
            instances.push(PromotionInstance::new(promotion, basket, items, pb, cost)?);
        }

        Ok(Self { instances })
    }

    /// Iterate over instances
    pub fn iter(&self) -> impl Iterator<Item = &PromotionInstance<'a>> {
        self.instances.iter()
    }

    /// Add usage constraints for all instances to the model
    pub fn add_item_usage(&self, usage: Expression, item_idx: usize) -> Expression {
        let mut total_usage = usage;

        for instance in &self.instances {
            total_usage = instance.add_item_usage(total_usage, item_idx);
        }

        total_usage
    }

    /// Add constraints for all promotion instances
    pub fn add_constraints<S: SolverModel>(
        &self,
        mut model: S,
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> S {
        for instance in &self.instances {
            model = instance.add_constraints(model, basket, items);
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
    /// If the promotion cannot apply to the provided `basket`/`items`, we can avoid adding
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
        basket: &'a Basket<'a>,
        items: &[usize],
        pb: &mut ProblemVariables,
        cost: &mut Expression,
    ) -> Result<Self, SolverError> {
        let vars = match (promotion, promotion.is_applicable(basket, items)) {
            (Promotion::SimpleDiscount(simple_discount), true) => {
                simple_discount.add_variables(basket, items, pb, cost)?
            }
            (_promotion, false) => Box::new(NoopPromotionVars),
        };

        Ok(Self { promotion, vars })
    }

    /// Contribute this promotion's per-item usage term for `item_idx`.
    ///
    /// This is called while building the global per-item equality that enforces that
    /// an item is either full price, or used by exactly one promotion.
    pub fn add_item_usage(&self, usage: Expression, item_idx: usize) -> Expression {
        self.vars.add_item_usage(usage, item_idx)
    }

    /// Add promotion-specific constraints for this instance.
    ///
    /// This delegates to the concrete promotion type, passing the instance's variable adapter
    /// (`self.vars`). For a no-op instance, this should behave like an identity and return the
    /// model unchanged.
    fn add_constraints<S: SolverModel>(
        &self,
        model: S,
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> S {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => {
                simple.add_constraints(model, self.vars.as_ref(), basket, items)
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
    /// Returns `SolverError` if a selected item index is invalid (missing from the basket),
    /// or if the discount for a selected item cannot be computed.
    pub fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => {
                simple.calculate_item_discounts(solution, self.vars.as_ref(), basket, items)
            }
        }
    }

    /// Post-solve interpretation returning full promotion applications.
    ///
    /// Reads the solved variable values to determine which items this promotion selected and
    /// returns [`PromotionApplication`] instances with bundle IDs and price details.
    pub fn calculate_item_applications(
        &self,
        solution: &dyn Solution,
        basket: &'a Basket<'a>,
        items: &[usize],
        next_bundle_id: &mut usize,
    ) -> SmallVec<[PromotionApplication<'a>; 10]> {
        match &self.promotion {
            Promotion::SimpleDiscount(simple) => simple.calculate_item_applications(
                self.promotion.key(),
                solution,
                self.vars.as_ref(),
                basket,
                items,
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
    /// Add this promotion's usage term for `item_idx` to the running `usage` expression.
    ///
    /// `usage` is the accumulated "how is this item accounted for" expression for a single item.
    /// Implementations should add their own per-item decision variable(s), if any, and return the
    /// updated expression.
    fn add_item_usage(&self, usage: Expression, item_idx: usize) -> Expression;

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
///    variables to each other and/or to the basket.
/// 4. After solving, [`ILPPromotion::calculate_item_discounts`] extracts the final
///    per-item discount amounts from the solution.
///
/// Notes:
/// - Implementations should be deterministic for a given `basket`/`items` input; ILP
///   solutions are already sensitive to tiny numeric differences.
/// - Promotions that are not applicable are modeled as a no-op by [`PromotionInstance::new`].
///   Implementations should therefore treat an "empty" `vars` as "selects nothing" and avoid
///   introducing constraints that would accidentally affect the global model.
pub trait ILPPromotion<'a>: Send + Sync {
    /// Return whether this promotion _might_ apply to the given basket and candidate items.
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
    fn is_applicable(&self, basket: &'a Basket<'a>, items: &[usize]) -> bool;

    /// Create per-item binary variables and add them to the objective expression.
    ///
    /// Each eligible item gets a decision variable indicating whether this promotion applies.
    /// The discounted price contribution should be added to `cost` so the solver can trade off
    /// selecting discounts against other promotions under global constraints.
    ///
    /// Missing items should be surfaced to callers via [`SolverError::Basket`].
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
        basket: &'a Basket<'a>,
        items: &[usize],
        pb: &mut ProblemVariables,
        cost: &mut Expression,
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
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> S;

    /// Calculate per-item discounts chosen for this promotion.
    ///
    /// This is the "interpretation" step: it inspects the solved decision variables
    /// and returns a mapping from basket item index to `(original_minor, discounted_minor)`.
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
    /// Returns [`SolverError`] if a selected item index is invalid (missing from the basket),
    /// or if the discount for a selected item cannot be computed.
    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        basket: &'a Basket<'a>,
        items: &[usize],
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
    fn calculate_item_applications(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        basket: &'a Basket<'a>,
        items: &[usize],
        next_bundle_id: &mut usize,
    ) -> SmallVec<[PromotionApplication<'a>; 10]>;
}

/// No-op promotion variables implementation
#[derive(Debug)]
struct NoopPromotionVars;

impl PromotionVars for NoopPromotionVars {
    fn add_item_usage(&self, usage: Expression, _item_idx: usize) -> Expression {
        usage
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
        basket::Basket,
        discounts::Discount,
        items::Item,
        products::ProductKey,
        promotions::{Promotion, PromotionKey, simple_discount::SimpleDiscount},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::PromotionInstance;

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

    #[test]
    fn promotion_instance_calculates_item_discounts_via_inner_promotion() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promotion = Promotion::SimpleDiscount(SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        ));

        let mut pb = ProblemVariables::new();
        let mut cost = Expression::default();
        let instance = PromotionInstance::new(&promotion, &basket, &[0], &mut pb, &mut cost)?;

        let discounts = instance.calculate_item_discounts(&SelectAllSolution, &basket, &[0])?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

        Ok(())
    }
}
