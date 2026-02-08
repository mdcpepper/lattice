//! ILP Promotions

use std::{any::Any, fmt::Debug, sync::Arc};

use good_lp::{Expression, Solution};
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    items::groups::ItemGroup,
    promotions::{PromotionKey, applications::PromotionApplication},
    solvers::{
        SolverError,
        ilp::{ILPObserver, state::ILPState},
    },
};

mod direct_discount;
mod mix_and_match;
mod positional_discount;
mod tiered_threshold;

#[cfg(test)]
pub(crate) mod test_support;

/// Collection of promotion instances for a solve operation
#[derive(Debug)]
pub(crate) struct PromotionInstances<'a> {
    instances: SmallVec<[PromotionInstance<'a>; 5]>,
}

impl<'a> PromotionInstances<'a> {
    /// Create Promotion Instances from a slice of promotions
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if any applicable promotion fails to add variables.
    pub(crate) fn from_promotions(
        promotions: &[&'a dyn ILPPromotion],
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<Self, SolverError> {
        let mut instances = SmallVec::new();

        for &promotion in promotions {
            let instance = PromotionInstance::new(promotion, item_group, state, observer)?;

            instances.push(instance);
        }

        Ok(Self { instances })
    }

    /// Iterate over instances
    pub(crate) fn iter(&self) -> impl Iterator<Item = &PromotionInstance<'a>> {
        self.instances.iter()
    }

    /// Add presence terms for all promotion instances to the item constraint
    ///
    /// Contributes each promotion's decision variables for the given item to the
    /// presence/exclusivity constraint expression.
    #[must_use]
    pub(crate) fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for instance in &self.instances {
            updated_expr = instance.add_item_presence_term(updated_expr, item_idx);
        }

        updated_expr
    }
}

/// A promotion instance that pairs a promotion with its solver variables
#[derive(Debug)]
pub(crate) struct PromotionInstance<'a> {
    /// The promotion being solved
    promotion: &'a dyn ILPPromotion,

    /// The solver variables for this promotion instance
    vars: Option<PromotionVars>,
}

impl<'a> PromotionInstance<'a> {
    /// Create a new promotion instance (promotion & its solver variables).
    ///
    /// If the promotion cannot apply to the provided item group, we can avoid adding
    /// any decision variables and skip creating vars for it. This keeps the global
    /// model smaller and prevents inapplicable promotions from contributing to the objective
    /// expression (`cost`), or any per-item usage sums used for the exclusivity constraints.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if the promotion fails to add variables (for example, due to
    /// invalid indices, discount errors, or non-representable coefficients).
    pub(crate) fn new(
        promotion: &'a dyn ILPPromotion,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<Self, SolverError> {
        let vars = if promotion.is_applicable(item_group) {
            let vars = promotion.add_variables(item_group, state, observer)?;
            vars.add_constraints(promotion.key(), item_group, state, observer)?;

            Some(vars)
        } else {
            None
        };

        Ok(Self { promotion, vars })
    }

    /// Contribute this promotion's presence term for `item_idx`.
    ///
    /// This is called while building the per-item presence/exclusivity constraint that
    /// enforces each item is either at full price or used by exactly one promotion.
    #[must_use]
    pub(crate) fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        match &self.vars {
            Some(vars) => vars.add_item_participation_term(expr, item_idx),
            None => expr,
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
    #[cfg(test)]
    pub(crate) fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        self.vars.as_ref().map_or_else(
            || Ok(FxHashMap::default()),
            |vars| vars.calculate_item_discounts(solution, item_group),
        )
    }

    /// Post-solve interpretation returning full promotion applications.
    ///
    /// Reads the solved variable values to determine which items this promotion selected and
    /// returns [`PromotionApplication`] instances with bundle IDs and price details.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if a selected item index is invalid (missing from the item group),
    /// or if the discount for a selected item cannot be computed.
    pub(crate) fn calculate_item_applications<'b>(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        self.vars.as_ref().map_or_else(
            || Ok(SmallVec::new()),
            |vars| {
                vars.calculate_item_applications(
                    self.promotion.key(),
                    solution,
                    item_group,
                    next_bundle_id,
                )
            },
        )
    }
}

/// Interface for promotion-specific runtime variable bundles.
///
/// Implementations represent a fully-compiled promotion runtime:
/// they own all decision-variable references needed to emit constraints and
/// to interpret a solved model into discounts/applications.
pub trait ILPPromotionVars: Debug + Send + Sync + Any {
    /// Contribute the participation variable(s) for `item_idx` into `expr`.
    fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression;

    /// Returns true if `item_idx` participates in this promotion.
    fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool;

    /// Returns true if this promotion determines the final price for `item_idx`.
    fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.is_item_participating(solution, item_idx)
    }

    /// Emit vars-owned constraints into the ILP state.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if constraint construction fails, such as
    /// invalid item indices, discount computation failures, or non-representable
    /// minor-unit coefficients.
    fn add_constraints(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError>;

    /// Vars-owned post-solve discount extraction.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if solution interpretation fails, such as
    /// missing item indices in the group or invalid promotion runtime state.
    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError>;

    /// Vars-owned post-solve promotion application extraction.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if solution interpretation fails, such as
    /// missing item indices in the group or invalid promotion runtime state.
    fn calculate_item_applications<'b>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError>;
}

/// Promotion variable bundle produced by an ILP promotion implementation.
pub type PromotionVars = Box<dyn ILPPromotionVars>;

/// Makes a [`crate::promotions::Promotion`] usable by the ILP solver.
///
/// Implementations are responsible for compiling a promotion into:
///
/// - A set of per-item decision variables (including any encoded constraints), and
/// - A runtime vars object that owns constraints + post-solve interpretation.
///
/// Expected lifecycle for a promotion instance within a solve:
///
/// 1. [`ILPPromotion::is_applicable`] is used as a cheap pre-filter.
/// 2. [`ILPPromotion::add_variables`] creates the decision variables and contributes
///    to the objective (`cost`), returning a vars runtime bundle.
/// 3. The solver calls vars-owned constraint and post-solve methods.
///
/// Notes:
/// - Implementations should be deterministic for a given item group input; ILP
///   solutions are already sensitive to tiny numeric differences.
/// - Inapplicable promotions are skipped during solver instance creation, so no vars
///   bundle is created and they contribute nothing to the solve model.
pub trait ILPPromotion: Debug + Send + Sync {
    /// Return the promotion key.
    fn key(&self) -> PromotionKey;

    /// Return whether this promotion _might_ apply to the given item group.
    ///
    /// This is used as a fast pre-check to avoid allocating variables/constraints for
    /// promotions that cannot possibly contribute to the solution.
    ///
    /// Prefer no false negatives, returning `true` when unsure is safe (it just creates extra
    /// variables), while returning `false` can prevent a valid discount from ever being
    /// considered by the solver.
    ///
    /// Avoid expensive computations that can be deferred until [`ILPPromotion::add_variables`].
    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool;

    /// Create per-item binary variables and add them to the objective expression.
    ///
    /// Each eligible item gets a decision variable indicating whether this promotion applies.
    /// The discounted price contribution should be added to `cost` so the solver can trade off
    /// selecting discounts against other promotions under global constraints.
    ///
    /// # Errors
    ///
    /// Missing items should be surfaced to callers via [`SolverError::ItemGroup`].
    /// Discount calculation errors should be surfaced to callers via [`SolverError::Discount`].
    /// If a discounted minor unit amount cannot be represented exactly as a solver coefficient,
    /// return [`SolverError::MinorUnitsNotRepresentable`].
    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError>;
}

impl ILPPromotion for Arc<dyn ILPPromotion + '_> {
    fn key(&self) -> PromotionKey {
        self.as_ref().key()
    }

    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        self.as_ref().is_applicable(item_group)
    }

    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError> {
        self.as_ref().add_variables(item_group, state, observer)
    }
}

/// Check if an i64 value is exactly representable as f64.
#[must_use]
pub fn i64_to_f64_exact(v: i64) -> Option<f64> {
    let f = v.to_f64()?;

    (f.to_i64() == Some(v)).then_some(f)
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, IntoAffineExpression, ProblemVariables};
    use rusty_money::{Money, iso::GBP};
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::Item,
        products::ProductKey,
        promotions::{
            PromotionKey, PromotionSlotKey,
            budget::PromotionBudget,
            types::{
                DirectDiscountPromotion, MixAndMatchDiscount, MixAndMatchPromotion,
                PositionalDiscountPromotion,
            },
        },
        solvers::ilp::{
            NoopObserver,
            promotions::test_support::{
                CountingObserver, SelectAllSolution, item_group_from_items,
            },
        },
        tags::{collection::TagCollection, string::StringTagCollection},
        utils::slot,
    };

    use super::*;

    #[test]
    fn promotion_instance_calculates_item_discounts_for_direct_discount() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["any"]),
        )];
        let item_group = item_group_from_items(items);

        let promotion = crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ));

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let instance = PromotionInstance::new(&promotion, &item_group, &mut state, &mut observer)?;

        let discounts = instance.calculate_item_discounts(&SelectAllSolution, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

        Ok(())
    }

    #[test]
    fn promotion_instance_handles_positional_promotions() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(400, GBP)),
        ];
        let item_group = item_group_from_items(items);

        let promo = crate::promotions::promotion(PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::PercentageOff(decimal_percentage::Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        ));

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let instance = PromotionInstance::new(&promo, &item_group, &mut state, &mut observer)?;

        let discounts = instance.calculate_item_discounts(&SelectAllSolution, &item_group)?;
        assert_eq!(discounts.len(), 4);

        let mut next_bundle_id = 0;
        let applications = instance.calculate_item_applications(
            &SelectAllSolution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(applications.len(), 4);
        assert!(next_bundle_id > 0);

        Ok(())
    }

    #[test]
    fn promotion_vars_routes_positional_logic() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
        ];

        let item_group = item_group_from_items(items);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::PercentageOff(decimal_percentage::Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let expr = Expression::default();
        let updated = vars.add_item_participation_term(expr, 0);
        assert!(updated.linear_coefficients().next().is_some());

        assert!(vars.is_item_participating(&SelectAllSolution, 0));
        assert!(vars.is_item_priced_by_promotion(&SelectAllSolution, 0));

        // Smoke test vars-owned constraint emission.
        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        Ok(())
    }

    #[test]
    fn promotion_vars_reports_direct_discounted() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];
        let item_group = item_group_from_items(items);

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        assert!(vars.is_item_priced_by_promotion(&SelectAllSolution, 0));

        Ok(())
    }

    #[test]
    fn i64_to_f64_exact_detects_inexact_values() {
        assert_eq!(i64_to_f64_exact(1_000), Some(1_000.0));
        // 2^53 + 1 is not exactly representable in f64.
        assert_eq!(i64_to_f64_exact(9_007_199_254_740_993), None);
    }

    #[test]
    fn promotion_instance_handles_mix_and_match_promotions() -> TestResult {
        let items = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ];

        let item_group = item_group_from_items(items);

        let mut slot_keys = slotmap::SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promotion = crate::promotions::promotion(MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(decimal_percentage::Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        ));

        let mut state = ILPState::with_presence_variables(&item_group)?;

        let mut observer = NoopObserver;

        let instance = PromotionInstance::new(&promotion, &item_group, &mut state, &mut observer)?;

        let discounts = instance.calculate_item_discounts(&SelectAllSolution, &item_group)?;

        let mut next_bundle_id = 0;

        let applications = instance.calculate_item_applications(
            &SelectAllSolution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert!(!discounts.is_empty());
        assert!(!applications.is_empty());

        Ok(())
    }

    #[test]
    fn promotion_vars_runtime_methods_are_callable() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let mut observer = NoopObserver;

        let mut slot_keys = slotmap::SlotMap::<PromotionSlotKey, ()>::with_key();

        let mm = MixAndMatchPromotion::new(
            PromotionKey::default(),
            vec![slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["any"]),
                1,
                Some(1),
            )],
            MixAndMatchDiscount::PercentAllItems(decimal_percentage::Percentage::from(0.1)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;

        let mm_vars = mm.add_variables(&item_group, &mut state, &mut observer)?;
        let _ = mm_vars.is_item_participating(&SelectAllSolution, 0);
        let _ = mm_vars.is_item_priced_by_promotion(&SelectAllSolution, 0);

        let positional = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            1,
            SmallVec::from_vec(vec![0u16]),
            SimpleDiscount::PercentageOff(decimal_percentage::Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        mm_vars.add_constraints(mm.key(), &item_group, &mut state, &mut observer)?;

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let positional_vars = positional.add_variables(&item_group, &mut state, &mut observer)?;
        positional_vars.add_constraints(
            positional.key(),
            &item_group,
            &mut state,
            &mut observer,
        )?;

        Ok(())
    }

    #[test]
    fn inapplicable_promotion_produces_no_runtime_effects() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["a"]),
        )];
        let item_group = item_group_from_items(items);

        let promo = crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["no-match"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ));

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = CountingObserver::default();
        let instance = PromotionInstance::new(&promo, &item_group, &mut state, &mut observer)?;

        let expr = instance.add_item_presence_term(Expression::default(), 0);
        assert!(expr.linear_coefficients().next().is_none());
        assert_eq!(
            instance.calculate_item_discounts(&SelectAllSolution, &item_group)?,
            FxHashMap::default()
        );

        let mut next_bundle_id = 0;
        let applications = instance.calculate_item_applications(
            &SelectAllSolution,
            &item_group,
            &mut next_bundle_id,
        )?;
        assert!(applications.is_empty());
        assert_eq!(next_bundle_id, 0);

        assert_eq!(observer.promotion_variables, 0);
        assert_eq!(observer.objective_terms, 0);
        assert_eq!(observer.promotion_constraints, 0);

        Ok(())
    }

    #[test]
    fn applications_keep_bundle_ids_contiguous_across_instances() -> TestResult {
        let items = [
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
        ];
        let item_group = item_group_from_items(items);

        let promo_a = crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ));
        let promo_b = crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["b"]),
            SimpleDiscount::AmountOverride(Money::from_minor(75, GBP)),
            PromotionBudget::unlimited(),
        ));
        let promotions = vec![&promo_a as &dyn ILPPromotion, &promo_b as &dyn ILPPromotion];

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let instances = PromotionInstances::from_promotions(
            &promotions,
            &item_group,
            &mut state,
            &mut observer,
        )?;

        let mut next_bundle_id = 0;
        let mut bundle_ids = Vec::new();

        for instance in instances.iter() {
            let applications = instance.calculate_item_applications(
                &SelectAllSolution,
                &item_group,
                &mut next_bundle_id,
            )?;

            for app in applications {
                bundle_ids.push(app.bundle_id);
            }
        }

        bundle_ids.sort_unstable();
        assert_eq!(bundle_ids, vec![0, 1]);
        assert_eq!(next_bundle_id, 2);

        Ok(())
    }

    #[test]
    fn direct_discount_budget_callbacks_stay_observer_consistent() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];
        let item_group = item_group_from_items(items);

        let promo = crate::promotions::promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::with_both_limits(1, Money::from_minor(100, GBP)),
        ));

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = CountingObserver::default();
        let _ = PromotionInstance::new(&promo, &item_group, &mut state, &mut observer)?;

        assert_eq!(observer.promotion_variables, 2);
        assert_eq!(observer.objective_terms, 2);
        assert_eq!(observer.promotion_constraints, 2);

        Ok(())
    }
}
