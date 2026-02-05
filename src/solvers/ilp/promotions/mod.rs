//! ILP Promotions

use good_lp::{Expression, Solution, SolverModel};
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    items::groups::ItemGroup,
    promotions::{Promotion, PromotionKey, applications::PromotionApplication},
    solvers::{
        SolverError,
        ilp::{ILPObserver, state::ILPState},
    },
};

mod direct_discount;
mod mix_and_match;
mod positional_discount;

use direct_discount::DirectDiscountPromotionVars;
use mix_and_match::MixAndMatchVars;
use positional_discount::PositionalDiscountVars;

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
    pub fn from_promotions<O: ILPObserver + ?Sized>(
        promotions: &'a [Promotion<'_>],
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut O,
    ) -> Result<Self, SolverError> {
        let mut instances = SmallVec::new();

        for promotion in promotions {
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

    /// Add constraints for all promotion instances
    pub fn add_constraints<S: SolverModel, O: ILPObserver + ?Sized>(
        &self,
        mut model: S,
        item_group: &ItemGroup<'_>,
        observer: &mut O,
    ) -> S {
        for instance in &self.instances {
            model = instance.add_constraints(model, item_group, observer);
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
    vars: PromotionVars,
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
    pub fn new<O: ILPObserver + ?Sized>(
        promotion: &'a Promotion<'a>,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut O,
    ) -> Result<Self, SolverError> {
        let vars = match (promotion, promotion.is_applicable(item_group)) {
            (Promotion::DirectDiscount(direct_discount), true) => {
                direct_discount.add_variables(promotion.key(), item_group, state, observer)?
            }
            (Promotion::MixAndMatch(mix_and_match), true) => {
                mix_and_match.add_variables(promotion.key(), item_group, state, observer)?
            }
            (Promotion::PositionalDiscount(positional_discount), true) => {
                positional_discount.add_variables(promotion.key(), item_group, state, observer)?
            }
            (_promotion, false) => PromotionVars::Noop,
        };

        Ok(Self { promotion, vars })
    }

    /// Contribute this promotion's presence term for `item_idx`.
    ///
    /// This is called while building the per-item presence/exclusivity constraint that
    /// enforces each item is either at full price or used by exactly one promotion.
    pub fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        self.vars.add_item_participation_term(expr, item_idx)
    }

    /// Add promotion-specific constraints for this instance.
    ///
    /// This uses the enum to dispatch to the correct constraint logic based on the
    /// concrete vars type.
    fn add_constraints<S: SolverModel, O: ILPObserver + ?Sized>(
        &self,
        model: S,
        _item_group: &ItemGroup<'_>,
        observer: &mut O,
    ) -> S {
        self.vars
            .add_constraints(model, self.promotion.key(), observer)
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
            Promotion::DirectDiscount(direct_discount) => {
                direct_discount.calculate_item_discounts(solution, &self.vars, item_group)
            }
            Promotion::MixAndMatch(mix_and_match) => {
                mix_and_match.calculate_item_discounts(solution, &self.vars, item_group)
            }
            Promotion::PositionalDiscount(positional_discount) => {
                positional_discount.calculate_item_discounts(solution, &self.vars, item_group)
            }
        }
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
    pub fn calculate_item_applications<'group>(
        &self,
        solution: &dyn Solution,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'group>; 10]>, SolverError> {
        match &self.promotion {
            Promotion::DirectDiscount(direct_discount) => direct_discount
                .calculate_item_applications(
                    self.promotion.key(),
                    solution,
                    &self.vars,
                    item_group,
                    next_bundle_id,
                ),
            Promotion::MixAndMatch(mix_and_match) => mix_and_match.calculate_item_applications(
                self.promotion.key(),
                solution,
                &self.vars,
                item_group,
                next_bundle_id,
            ),
            Promotion::PositionalDiscount(positional_discount) => positional_discount
                .calculate_item_applications(
                    self.promotion.key(),
                    solution,
                    &self.vars,
                    item_group,
                    next_bundle_id,
                ),
        }
    }
}

/// Enum wrapping concrete promotion vars types.
///
/// This allows us to store promotion-specific vars while still being able to
/// add promotion-specific constraints without downcasting.
#[derive(Debug)]
pub enum PromotionVars {
    /// No-op vars for inapplicable promotions
    Noop,

    /// Direct discount promotion vars
    DirectDiscount(Box<DirectDiscountPromotionVars>),

    /// Mix-and-Match promotion vars
    MixAndMatch(Box<MixAndMatchVars>),

    /// Positional discount promotion vars
    PositionalDiscount(Box<PositionalDiscountVars>),
}

impl PromotionVars {
    /// Add item participation term to the expression.
    pub fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
        match self {
            Self::Noop => expr,
            Self::DirectDiscount(vars) => vars.add_item_participation_term(expr, item_idx),
            Self::MixAndMatch(vars) => vars.add_item_participation_term(expr, item_idx),
            Self::PositionalDiscount(vars) => vars.add_item_participation_term(expr, item_idx),
        }
    }

    /// Returns true if the item is participating in the promotion.
    pub fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        match self {
            Self::Noop => false,
            Self::DirectDiscount(vars) => vars.is_item_participating(solution, item_idx),
            Self::MixAndMatch(vars) => vars.is_item_participating(solution, item_idx),
            Self::PositionalDiscount(vars) => vars.is_item_participating(solution, item_idx),
        }
    }

    /// Returns true if the promotion determines the item's final price
    /// (which may be a discount or a price that stays the same).
    pub fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        match self {
            Self::Noop => false,
            Self::DirectDiscount(vars) => vars.is_item_participating(solution, item_idx),
            Self::MixAndMatch(vars) => vars.is_item_priced_by_promotion(solution, item_idx),
            Self::PositionalDiscount(vars) => vars.is_item_discounted(solution, item_idx),
        }
    }

    /// Add promotion-specific constraints to the model.
    pub fn add_constraints<S: SolverModel, O: ILPObserver + ?Sized>(
        &self,
        model: S,
        promotion_key: PromotionKey,
        observer: &mut O,
    ) -> S {
        match self {
            Self::Noop | Self::DirectDiscount(_) => model,
            Self::MixAndMatch(vars) => vars.add_constraints(model, promotion_key, observer),
            Self::PositionalDiscount(vars) => {
                vars.add_dfa_constraints(model, promotion_key, observer)
            }
        }
    }
}

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
    /// # Errors
    ///
    /// Missing items should be surfaced to callers via [`SolverError::ItemGroup`].
    /// Discount calculation errors should be surfaced to callers via [`SolverError::Discount`].
    /// If a discounted minor unit amount cannot be represented exactly as a solver coefficient,
    /// return [`SolverError::MinorUnitsNotRepresentable`].
    fn add_variables<O: ILPObserver + ?Sized>(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut O,
    ) -> Result<PromotionVars, SolverError>;

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
        vars: &PromotionVars,
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
    fn calculate_item_applications<'group>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &PromotionVars,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'group>; 10]>, SolverError>;
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

    #[cfg(feature = "solver-highs")]
    use good_lp::solvers::highs::highs as default_solver;
    #[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
    use good_lp::solvers::microlp::microlp as default_solver;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            Promotion, PromotionKey, direct_discount::DirectDiscountPromotion,
            positional_discount::PositionalDiscountPromotion,
        },
        solvers::ilp::NoopObserver,
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
        let currency = items.first().map_or(GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[test]
    fn promotion_instance_calculates_item_discounts_via_inner_promotion() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];
        let item_group = item_group_from_items(items);

        let promotion = Promotion::DirectDiscount(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
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

        let promo = Promotion::PositionalDiscount(PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::PercentageOff(decimal_percentage::Percentage::from(0.5)),
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
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let expr = Expression::default();
        let updated = vars.add_item_participation_term(expr, 0);
        assert!(updated.linear_coefficients().next().is_some());

        assert!(vars.is_item_participating(&SelectAllSolution, 0));
        assert!(vars.is_item_priced_by_promotion(&SelectAllSolution, 0));

        // Smoke test the revised model
        let (pb, cost, _presence) = state.into_parts();
        let model = pb.minimise(cost).using(default_solver);
        let _model = vars.add_constraints(model, promo.key(), &mut observer);

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
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        assert!(vars.is_item_priced_by_promotion(&SelectAllSolution, 0));

        Ok(())
    }

    #[test]
    fn i64_to_f64_exact_detects_inexact_values() {
        assert_eq!(i64_to_f64_exact(1_000), Some(1_000.0));
        // 2^53 + 1 is not exactly representable in f64.
        assert_eq!(i64_to_f64_exact(9_007_199_254_740_993), None);
    }
}
