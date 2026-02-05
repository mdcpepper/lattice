//! Direct Discount Promotions ILP

use good_lp::{Expression, Solution, Variable, variable};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    items::groups::ItemGroup,
    promotions::{
        PromotionKey, applications::PromotionApplication, direct_discount::DirectDiscountPromotion,
    },
    solvers::{
        SolverError,
        ilp::{
            BINARY_THRESHOLD, ILPObserver, i64_to_f64_exact,
            promotions::{ILPPromotion, PromotionVars},
            state::ILPState,
        },
    },
    tags::collection::TagCollection,
};

/// Solver variables for a direct discount promotion.
///
/// Tracks the mapping from item group indices to their corresponding
/// binary decision variables in the ILP model.
#[derive(Debug)]
pub struct DirectDiscountPromotionVars {
    /// Variables for tracking item participation
    item_participation: SmallVec<[(usize, Variable); 10]>,
}

impl DirectDiscountPromotionVars {
    pub fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for &(idx, var) in &self.item_participation {
            if idx == item_idx {
                updated_expr += var;
            }
        }

        updated_expr
    }

    pub fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.item_participation
            .iter()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }
}

impl ILPPromotion for DirectDiscountPromotion<'_> {
    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        if item_group.is_empty() {
            return false;
        }

        let promotion_tags = self.tags();

        if promotion_tags.is_empty() {
            return true;
        }

        item_group
            .iter()
            .any(|item| item.tags().intersects(promotion_tags))
    }

    fn add_variables<O: ILPObserver + ?Sized>(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut O,
    ) -> Result<PromotionVars, SolverError> {
        // An empty tag set means this promotion can target any item, so we can skip tag checks
        // if that is the case.
        let match_all = self.tags().is_empty();

        // Keep the mapping from item group index to solver variable so we can interpret solutions later.
        let mut item_participation = SmallVec::new();

        for (item_idx, item) in item_group.iter().enumerate() {
            // Enforce the promotion's tagging rules up-front so the solver doesn't need extra constraints.
            if !match_all && !item.tags().intersects(self.tags()) {
                continue;
            }

            // Compute the discounted price in minor units; if the discount can't be computed, return an error.
            let discounted_minor = self
                .calculate_discounted_price(item)
                .map_err(SolverError::from)?
                .to_minor_units();

            // `good_lp` uses floating point coefficients; only accept values that are exactly representable
            // to avoid rounding changing the solver's preferred choice.
            let Some(coeff) = i64_to_f64_exact(discounted_minor) else {
                return Err(SolverError::MinorUnitsNotRepresentable(discounted_minor));
            };

            // Create a binary decision variable for this item: should this promotion apply to it?
            let participation_var = state.problem_variables_mut().add(variable().binary());

            // Persist the variable so we can later mark items as participating from the solved model.
            item_participation.push((item_idx, participation_var));

            // Tell the solver "if you set this variable to 1 (apply this promotion to this item),
            // add the discounted price to the total instead of full price". The solver will weigh
            // this against other options when minimizing cost.
            state.add_to_objective(participation_var, coeff);

            // Notify observer
            observer.on_promotion_variable(
                promotion_key,
                item_idx,
                participation_var,
                discounted_minor,
                None,
            );

            observer.on_objective_term(participation_var, coeff);
        }

        Ok(PromotionVars::DirectDiscount(DirectDiscountPromotionVars {
            item_participation,
        }))
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &PromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        let mut discounts = FxHashMap::default();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !vars.is_item_participating(solution, item_idx) {
                continue;
            }

            // This must mirror the discounted minor unit value used during variable creation.
            // If we can't compute it here, something is inconsistent and should be surfaced.
            let discounted_minor = self
                .calculate_discounted_price(item)
                .map_err(SolverError::from)?
                .to_minor_units();

            discounts.insert(item_idx, (item.price().to_minor_units(), discounted_minor));
        }

        Ok(discounts)
    }

    fn calculate_item_applications<'group>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &PromotionVars,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'group>; 10]>, SolverError> {
        let mut applications = SmallVec::new();
        let currency = item_group.currency();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !vars.is_item_participating(solution, item_idx) {
                continue;
            }

            let discounted_minor = self
                .calculate_discounted_price(item)
                .map_err(SolverError::from)?
                .to_minor_units();

            // For DirectDiscountPromotion, each item gets its own unique bundle_id
            let bundle_id = *next_bundle_id;
            *next_bundle_id += 1;

            applications.push(PromotionApplication {
                promotion_key,
                item_idx,
                bundle_id,
                original_price: *item.price(),
                final_price: Money::from_minor(discounted_minor, currency),
            });
        }

        Ok(applications)
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, ProblemVariables, Solution, SolutionStatus, Variable};
    use rusty_money::{
        Money,
        iso::{self, GBP},
    };
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::PromotionKey,
        solvers::{
            SolverError,
            ilp::{NoopObserver, promotions::ILPPromotion},
        },
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
        let currency = items.first().map_or(GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

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

    #[derive(Debug)]
    struct SelectNoneSolution;

    impl Solution for SelectNoneSolution {
        fn status(&self) -> SolutionStatus {
            SolutionStatus::Optimal
        }

        fn value(&self, _variable: Variable) -> f64 {
            0.0
        }
    }

    #[test]
    fn is_applicable_returns_false_for_empty_items() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn add_variables_errors_on_discount_error() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        // Create a discount with currency mismatch to trigger an error
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOff(Money::from_minor(50, iso::USD)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let result = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer);

        assert!(matches!(result, Err(SolverError::Discount(_))));
    }

    #[test]
    fn add_variables_errors_on_nonrepresentable_minor_units() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(9_007_199_254_740_993, GBP)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let result = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer);

        assert!(matches!(
            result,
            Err(SolverError::MinorUnitsNotRepresentable { .. })
        ));
    }

    #[test]
    fn calculate_item_discounts_skips_on_discount_error() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        // Create a discount with currency mismatch to trigger an error
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOff(Money::from_minor(50, iso::USD)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);

        let promo_with_vars = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        );

        let mut observer = NoopObserver;

        let vars = promo_with_vars.add_variables(
            promo_with_vars.key(),
            &item_group,
            &mut state,
            &mut observer,
        )?;

        let result = promo.calculate_item_discounts(&SelectAllSolution, &vars, &item_group);

        assert!(matches!(result, Err(SolverError::Discount(_))));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_skips_unselected_items() -> TestResult {
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

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let discounts = promo.calculate_item_discounts(&SelectNoneSolution, &vars, &item_group)?;

        assert!(discounts.is_empty());

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_returns_discounted_values() -> TestResult {
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

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let discounts = promo.calculate_item_discounts(&SelectAllSolution, &vars, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

        Ok(())
    }

    #[test]
    fn calculate_item_applications_returns_applications_with_unique_bundle_ids() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        let item_group = item_group_from_items(items);

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;
        let mut next_bundle_id = 0_usize;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        )?;

        // Should have 2 applications
        assert_eq!(apps.len(), 2);

        // Each item should have a unique bundle_id
        assert_eq!(apps.first().map(|a| a.bundle_id), Some(0));
        assert_eq!(apps.get(1).map(|a| a.bundle_id), Some(1));

        // Verify next_bundle_id was incremented
        assert_eq!(next_bundle_id, 2);

        // Verify prices
        assert_eq!(
            apps.first().map(|a| a.original_price),
            Some(Money::from_minor(100, GBP))
        );
        assert_eq!(
            apps.first().map(|a| a.final_price),
            Some(Money::from_minor(50, GBP))
        );
        assert_eq!(
            apps.get(1).map(|a| a.original_price),
            Some(Money::from_minor(200, GBP))
        );
        assert_eq!(
            apps.get(1).map(|a| a.final_price),
            Some(Money::from_minor(50, GBP))
        );

        Ok(())
    }

    #[test]
    fn calculate_item_applications_returns_error_on_discount_error() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        // Create a discount with currency mismatch to trigger an error
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOff(Money::from_minor(50, iso::USD)),
        );

        let mut next_bundle_id = 0_usize;
        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);

        let promo_with_vars = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        );

        let mut observer = NoopObserver;

        let vars = promo_with_vars.add_variables(
            promo_with_vars.key(),
            &item_group,
            &mut state,
            &mut observer,
        )?;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        );

        assert!(matches!(apps, Err(SolverError::Discount(_))));
        assert_eq!(next_bundle_id, 0);

        Ok(())
    }

    #[test]
    fn calculate_item_applications_continues_bundle_id_counter() -> TestResult {
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

        // Start with a non-zero bundle_id (e.g., from previous promotions)
        let mut next_bundle_id = 5_usize;
        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(apps.len(), 1);
        assert_eq!(apps.first().map(|a| a.bundle_id), Some(5));
        assert_eq!(next_bundle_id, 6);

        Ok(())
    }
}
