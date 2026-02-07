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
}

/// A promotion instance that pairs a promotion with its solver variables
#[derive(Debug)]
pub struct PromotionInstance<'a> {
    /// The promotion being solved
    promotion: &'a dyn ILPPromotion,

    /// The solver variables for this promotion instance
    vars: Option<PromotionVars>,
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
        promotion: &'a dyn ILPPromotion,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<Self, SolverError> {
        let vars = if promotion.is_applicable(item_group) {
            let vars = promotion.add_variables(item_group, state, observer)?;

            if vars.owns_runtime_behavior() {
                vars.add_constraints(promotion.key(), item_group, state, observer)?;
            } else {
                promotion.add_constraints(vars.as_ref(), item_group, state, observer)?;
            }

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
    pub fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
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
    pub fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        self.vars.as_ref().map_or_else(
            || Ok(FxHashMap::default()),
            |vars| {
                if vars.owns_runtime_behavior() {
                    vars.calculate_item_discounts(solution, item_group)
                } else {
                    self.promotion
                        .calculate_item_discounts(solution, vars.as_ref(), item_group)
                }
            },
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
    pub fn calculate_item_applications<'b>(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        self.vars.as_ref().map_or_else(
            || Ok(SmallVec::new()),
            |vars| {
                if vars.owns_runtime_behavior() {
                    vars.calculate_item_applications(
                        self.promotion.key(),
                        solution,
                        item_group,
                        next_bundle_id,
                    )
                } else {
                    self.promotion.calculate_item_applications(
                        self.promotion.key(),
                        solution,
                        vars.as_ref(),
                        item_group,
                        next_bundle_id,
                    )
                }
            },
        )
    }
}

/// Interface for promotion-specific solver variable bundles.
pub trait ILPPromotionVars: Debug + Send + Sync {
    /// Contribute the participation variable(s) for `item_idx` into `expr`.
    fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression;

    /// Returns true if `item_idx` participates in this promotion.
    fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool;

    /// Returns true if this promotion determines the final price for `item_idx`.
    fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.is_item_participating(solution, item_idx)
    }

    /// Returns true when this vars implementation owns runtime behavior
    /// (constraints + post-solve interpretation).
    fn owns_runtime_behavior(&self) -> bool {
        false
    }

    /// Stable identifier for vars runtime ownership.
    ///
    /// Built-in promotion types should override this with a unique value so
    /// promotion methods can reject mismatched vars at runtime.
    fn runtime_kind(&self) -> &'static str {
        "unknown"
    }

    /// Emit vars-owned constraints into the ILP state.
    ///
    /// Default: no vars-owned behavior; caller should use promotion fallback.
    fn add_constraints(
        &self,
        _promotion_key: PromotionKey,
        _item_group: &ItemGroup<'_>,
        _state: &mut ILPState,
        _observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        Err(SolverError::InvariantViolation {
            message: "promotion type mismatch with vars",
        })
    }

    /// Vars-owned post-solve discount extraction.
    ///
    /// Default: no vars-owned behavior; caller should use promotion fallback.
    fn calculate_item_discounts(
        &self,
        _solution: &dyn Solution,
        _item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        Err(SolverError::InvariantViolation {
            message: "promotion type mismatch with vars",
        })
    }

    /// Vars-owned post-solve promotion application extraction.
    ///
    /// Default: no vars-owned behavior; caller should use promotion fallback.
    fn calculate_item_applications<'b>(
        &self,
        _promotion_key: PromotionKey,
        _solution: &dyn Solution,
        _item_group: &ItemGroup<'b>,
        _next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        Err(SolverError::InvariantViolation {
            message: "promotion type mismatch with vars",
        })
    }

    /// Runtime downcasting support for custom promotion implementations.
    fn as_any(&self) -> &dyn Any;
}

/// Promotion variable bundle produced by an ILP promotion implementation.
pub type PromotionVars = Box<dyn ILPPromotionVars>;

/// Makes a [`Promotion`] usable by the ILP solver.
///
/// Implementations are responsible for translating a promotion into:
///
/// - A set of per-item decision variables (including any encoded constraints), and
/// - A post-solve interpretation step that turns the chosen variables into concrete
///   per-item discounts.
///
/// Expected lifecycle for a promotion instance within a solve:
///
/// 1. [`ILPPromotion::is_applicable`] is used as a cheap pre-filter.
/// 2. [`ILPPromotion::add_variables`] creates the decision variables and contributes
///    to the objective (`cost`).
/// 3. After solving, [`ILPPromotion::calculate_item_discounts`] extracts the final
///    per-item discount amounts from the solution and creates the [`PromotionApplication`]s.
///
/// Notes:
/// - Implementations should be deterministic for a given item group input; ILP
///   solutions are already sensitive to tiny numeric differences.
/// - Promotions that are not applicable are modeled as a no-op by [`PromotionInstance::new`].
///   Implementations should therefore treat an "empty" `vars` as "selects nothing" and avoid
///   introducing constraints that would accidentally affect the global model.
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
    /// Avoid expensive computations that can be deferred until
    /// [`ILPPromotion::add_variables`] / [`ILPPromotion::calculate_item_discounts`].
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

    /// Emit promotion-specific constraints into the ILP state.
    ///
    /// This is called immediately after [`ILPPromotion::add_variables`] for
    /// applicable promotions.
    fn add_constraints(
        &self,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError>;

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
    /// Returns [`SolverError`] if the discount for a selected item cannot be computed.
    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError>;

    /// Calculate promotion applications for selected items.
    ///
    /// Similar to [`ILPPromotion::calculate_item_discounts`], but returns full
    /// [`PromotionApplication`] instances with bundle IDs and `Money` values.
    ///
    /// Each promotion type determines its own bundling semantics:
    /// - `DirectDiscountPromotion`: Each item gets its own unique `bundle_id` (no bundling).
    /// - Future bundle promotions: Items in the same deal share one `bundle_id`.
    ///
    /// The `next_bundle_id` counter is passed mutably and should be incremented
    /// for each new bundle created. This ensures unique IDs across all promotions.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if the discount for a selected item cannot be computed.
    fn calculate_item_applications<'b>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError>;
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

    fn add_constraints(
        &self,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.as_ref()
            .add_constraints(vars, item_group, state, observer)
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        self.as_ref()
            .calculate_item_discounts(solution, vars, item_group)
    }

    fn calculate_item_applications<'b>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &dyn ILPPromotionVars,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        self.as_ref().calculate_item_applications(
            promotion_key,
            solution,
            vars,
            item_group,
            next_bundle_id,
        )
    }
}

/// Check if an i64 value is exactly representable as f64.
pub fn i64_to_f64_exact(v: i64) -> Option<f64> {
    let f = v.to_f64()?;

    (f.to_i64() == Some(v)).then_some(f)
}

#[cfg(test)]
mod tests {
    use good_lp::{
        Expression, IntoAffineExpression, ProblemVariables, Solution, SolutionStatus, Variable,
    };
    use rusty_money::{Money, iso::GBP};
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            PromotionKey, PromotionSlotKey,
            budget::PromotionBudget,
            types::{
                DirectDiscountPromotion, MixAndMatchDiscount, MixAndMatchPromotion,
                PositionalDiscountPromotion,
            },
        },
        solvers::ilp::NoopObserver,
        tags::{collection::TagCollection, string::StringTagCollection},
        utils::slot,
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
        let currency = items.first().map_or(GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[test]
    fn promotion_instance_calculates_item_discounts_via_inner_promotion() -> TestResult {
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

        // Smoke test constraint emission.
        promo.add_constraints(vars.as_ref(), &item_group, &mut state, &mut observer)?;

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
    fn promotion_vars_noop_and_type_mismatch_paths() -> TestResult {
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

        let direct = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;

        let _ = direct.add_variables(&item_group, &mut state, &mut observer)?;

        let positional = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            1,
            SmallVec::from_vec(vec![0u16]),
            SimpleDiscount::PercentageOff(decimal_percentage::Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let positional_vars = positional.add_variables(&item_group, &mut state, &mut observer)?;

        let Err(err) = direct.add_constraints(
            positional_vars.as_ref(),
            &item_group,
            &mut state,
            &mut observer,
        ) else {
            panic!("expected promotion/vars mismatch")
        };

        assert!(matches!(
            err,
            SolverError::InvariantViolation {
                message: "promotion type mismatch with vars"
            }
        ));

        Ok(())
    }
}
