//! Direct Discount Promotions ILP

use good_lp::{Expression, Solution, Variable, variable};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    items::groups::ItemGroup,
    promotions::{
        PromotionKey, applications::PromotionApplication, types::DirectDiscountPromotion,
    },
    solvers::{
        SolverError,
        ilp::{
            BINARY_THRESHOLD, ILPObserver, i64_to_f64_exact,
            promotions::{ILPPromotion, ILPPromotionVars, PromotionVars},
            state::ILPState,
        },
    },
};

/// Solver variables for a direct discount promotion.
///
/// Tracks the mapping from item group indices to their corresponding
/// binary decision variables in the ILP model.
#[derive(Debug)]
pub struct DirectDiscountPromotionVars {
    /// Promotion key for observer/application output.
    promotion_key: PromotionKey,

    /// Variables for tracking item participation
    item_participation: SmallVec<[(usize, Variable); 10]>,

    /// Discounted minor unit value captured during variable creation.
    discounted_minor_by_item: FxHashMap<usize, i64>,

    /// Budget: optional max applications.
    application_limit: Option<u32>,

    /// Budget: optional max total discount value in minor units.
    monetary_limit_minor: Option<i64>,
}

impl DirectDiscountPromotionVars {
    fn add_model_constraints(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_budget_constraints(self.promotion_key, item_group, state, observer)
    }

    fn discounted_minor_for_item(&self, item_idx: usize) -> Result<i64, SolverError> {
        self.discounted_minor_by_item.get(&item_idx).copied().ok_or(
            SolverError::InvariantViolation {
                message: "missing discounted value for participating item",
            },
        )
    }

    /// Add budget constraints to the ILP state.
    pub fn add_budget_constraints(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        // Application count limit: sum(participation_vars) <= limit
        if let Some(application_limit) = self.application_limit {
            let participation_sum: Expression =
                self.item_participation.iter().map(|(_, var)| *var).sum();

            let limit_f64 = i64_to_f64_exact(i64::from(application_limit)).ok_or(
                SolverError::MinorUnitsNotRepresentable(i64::from(application_limit)),
            )?;

            observer.on_promotion_constraint(
                promotion_key,
                "application count budget",
                &participation_sum,
                "<=",
                limit_f64,
            );

            state.add_leq_constraint(participation_sum, limit_f64);
        }

        // Monetary limit: sum((full_price - discounted_price) * var) <= limit
        if let Some(limit_minor) = self.monetary_limit_minor {
            let mut discount_expr = Expression::default();

            for &(item_idx, var) in &self.item_participation {
                let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
                let full_minor = item.price().to_minor_units();
                let discounted_minor = self.discounted_minor_for_item(item_idx)?;

                let discount_amount = full_minor.saturating_sub(discounted_minor);
                let coeff = i64_to_f64_exact(discount_amount)
                    .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                discount_expr += var * coeff;
            }

            let limit_f64 = i64_to_f64_exact(limit_minor)
                .ok_or(SolverError::MinorUnitsNotRepresentable(limit_minor))?;

            observer.on_promotion_constraint(
                promotion_key,
                "monetary value budget",
                &discount_expr,
                "<=",
                limit_f64,
            );

            state.add_leq_constraint(discount_expr, limit_f64);
        }

        Ok(())
    }
}

impl ILPPromotionVars for DirectDiscountPromotionVars {
    fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for &(idx, var) in &self.item_participation {
            if idx == item_idx {
                updated_expr += var;
            }
        }

        updated_expr
    }

    fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.item_participation
            .iter()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }

    fn add_constraints(
        &self,
        _promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_model_constraints(item_group, state, observer)
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        let mut discounts = FxHashMap::default();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !self.is_item_participating(solution, item_idx) {
                continue;
            }

            let discounted_minor = self.discounted_minor_for_item(item_idx)?;

            discounts.insert(item_idx, (item.price().to_minor_units(), discounted_minor));
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

            let discounted_minor = self.discounted_minor_for_item(item_idx)?;

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

impl ILPPromotion for DirectDiscountPromotion<'_> {
    fn key(&self) -> PromotionKey {
        DirectDiscountPromotion::key(self)
    }

    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        if item_group.is_empty() {
            return false;
        }

        let qualification = self.qualification();

        item_group
            .iter()
            .any(|item| qualification.matches(item.tags()))
    }

    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError> {
        let promotion_key = self.key();

        // Keep the mapping from item group index to solver variable so we can interpret solutions later.
        let mut item_participation = SmallVec::new();
        let mut discounted_minor_by_item = FxHashMap::default();

        for (item_idx, item) in item_group.iter().enumerate() {
            // Enforce the promotion's qualification rules up-front so the solver doesn't need
            // extra constraints.
            if !self.qualification().matches(item.tags()) {
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
            discounted_minor_by_item.insert(item_idx, discounted_minor);

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

        Ok(Box::new(DirectDiscountPromotionVars {
            promotion_key,
            item_participation,
            discounted_minor_by_item,
            application_limit: self.budget().application_limit,
            monetary_limit_minor: self.budget().monetary_limit.map(|v| v.to_minor_units()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, ProblemVariables};
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
        promotions::{PromotionKey, budget::PromotionBudget, qualification::Qualification},
        solvers::{
            SolverError,
            ilp::{
                NoopObserver,
                promotions::{
                    ILPPromotion,
                    test_support::{SelectAllSolution, SelectNoneSolution, item_group_from_items},
                },
            },
        },
    };

    use super::*;

    #[test]
    fn is_applicable_returns_false_for_empty_items() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
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
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::match_all(),
            SimpleDiscount::AmountOff(Money::from_minor(50, iso::USD)),
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let result = promo.add_variables(&item_group, &mut state, &mut observer);

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
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(9_007_199_254_740_993, GBP)),
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let result = promo.add_variables(&item_group, &mut state, &mut observer);

        assert!(matches!(
            result,
            Err(SolverError::MinorUnitsNotRepresentable { .. })
        ));
    }

    #[test]
    fn calculate_item_discounts_uses_vars_runtime_data() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);

        let promo_with_vars = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut observer = NoopObserver;

        let vars = promo_with_vars.add_variables(&item_group, &mut state, &mut observer)?;

        let discounts = vars
            .as_ref()
            .calculate_item_discounts(&SelectAllSolution, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

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
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let discounts = vars
            .as_ref()
            .calculate_item_discounts(&SelectNoneSolution, &item_group)?;

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
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let discounts = vars
            .as_ref()
            .calculate_item_discounts(&SelectAllSolution, &item_group)?;

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
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let mut next_bundle_id = 0_usize;

        let apps = vars.as_ref().calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
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
    fn calculate_item_applications_uses_vars_runtime_data() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let mut next_bundle_id = 0_usize;

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);

        let promo_with_vars = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut observer = NoopObserver;

        let vars = promo_with_vars.add_variables(&item_group, &mut state, &mut observer)?;

        let apps = vars.as_ref().calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(apps.len(), 1);
        assert_eq!(
            apps.first().map(|a| a.final_price),
            Some(Money::from_minor(50, GBP))
        );
        assert_eq!(next_bundle_id, 1);

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
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        // Start with a non-zero bundle_id (e.g., from previous promotions)
        let mut next_bundle_id = 5_usize;

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let apps = vars.as_ref().calculate_item_applications(
            PromotionKey::default(),
            &SelectAllSolution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(apps.len(), 1);
        assert_eq!(apps.first().map(|a| a.bundle_id), Some(5));
        assert_eq!(next_bundle_id, 6);

        Ok(())
    }
}
