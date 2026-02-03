//! Simple Promotions ILP

use std::slice;

use good_lp::{Expression, Solution, Variable, variable};
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    discounts::calculate_discount,
    items::groups::ItemGroup,
    promotions::{
        PromotionKey, applications::PromotionApplication, simple_discount::SimpleDiscount,
    },
    solvers::{
        SolverError,
        ilp::{
            BINARY_THRESHOLD,
            promotions::{ILPPromotion, PromotionVars},
            state::ILPState,
        },
    },
    tags::collection::TagCollection,
};

#[derive(Debug)]
pub struct SimpleDiscountVars {
    item_vars: SmallVec<[(usize, Variable); 10]>,
}

impl PromotionVars for SimpleDiscountVars {
    fn add_item_presence_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for &(idx, var) in &self.item_vars {
            if idx == item_idx {
                updated_expr += var;
            }
        }

        updated_expr
    }

    fn is_item_selected(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.item_vars
            .iter()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }
}

impl ILPPromotion for SimpleDiscount<'_> {
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

    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
    ) -> Result<Box<dyn PromotionVars>, SolverError> {
        // An empty tag set means this promotion can target any item, so we can skip tag checks
        // if that is the case.
        let match_all = self.tags().is_empty();

        // Keep the mapping from item group index -> solver variable so we can interpret solutions later.
        let mut item_vars: SmallVec<[(usize, Variable); 10]> = SmallVec::new();

        for (item_idx, item) in item_group.iter().enumerate() {
            // Enforce the promotion's tagging rules up-front so the solver doesn't need extra constraints.
            if !match_all && !item.tags().intersects(self.tags()) {
                continue;
            }

            // Compute the discounted price in minor units; if the discount can't be computed, return an error.
            let discounted_minor = match calculate_discount(self.discount(), slice::from_ref(item))
            {
                Ok(price) => price.to_minor_units(),
                Err(err) => return Err(err.into()),
            };

            // `good_lp` uses floating point coefficients; only accept values that are exactly representable
            // to avoid rounding changing the solver's preferred choice.
            let Some(coeff) = i64_to_f64_exact(discounted_minor) else {
                return Err(SolverError::MinorUnitsNotRepresentable {
                    minor_units: discounted_minor,
                });
            };

            // Create a binary decision variable for this item: should this promotion apply to it?
            let var = state.problem_variables_mut().add(variable().binary());

            // Persist the variable so we can later mark items as "selected" from the solved model.
            item_vars.push((item_idx, var));

            // Tell the solver "if you set this variable to 1 (apply this promotion to this item),
            // add the discounted price to the total instead of full price". The solver will weigh
            // this against other options when minimizing cost.
            state.add_to_objective(var, coeff);
        }

        Ok(Box::new(SimpleDiscountVars { item_vars }))
    }

    fn add_constraints<S: good_lp::SolverModel>(
        &self,
        model: S,
        _vars: &dyn PromotionVars,
        _item_group: &ItemGroup<'_>,
    ) -> S {
        // Return the model without any additional constraints
        model
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        let mut discounts = FxHashMap::default();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !vars.is_item_selected(solution, item_idx) {
                continue;
            }

            // This must mirror the discounted minor unit value used during variable creation.
            // If we can't compute it here, something is inconsistent and should be surfaced.
            let discounted_minor = calculate_discount(self.discount(), slice::from_ref(item))
                .map(|price| price.to_minor_units())
                .map_err(SolverError::from)?;

            let original_minor = item.price().to_minor_units();

            discounts.insert(item_idx, (original_minor, discounted_minor));
        }

        Ok(discounts)
    }

    fn calculate_item_applications<'group>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        item_group: &'group ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> SmallVec<[PromotionApplication<'group>; 10]> {
        let mut applications = SmallVec::new();
        let currency = item_group.currency();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !vars.is_item_selected(solution, item_idx) {
                continue;
            }

            let discounted_minor = match calculate_discount(self.discount(), slice::from_ref(item))
            {
                Ok(price) => price.to_minor_units(),
                Err(_) => continue,
            };

            let original_minor = item.price().to_minor_units();

            // For SimpleDiscount, each item gets its own unique bundle_id
            let bundle_id = *next_bundle_id;
            *next_bundle_id += 1;

            applications.push(PromotionApplication {
                promotion_key,
                item_idx,
                bundle_id,
                original_price: Money::from_minor(original_minor, currency),
                final_price: Money::from_minor(discounted_minor, currency),
            });
        }

        applications
    }
}

fn i64_to_f64_exact(v: i64) -> Option<f64> {
    let f = v.to_f64()?;

    (f.to_i64() == Some(v)).then_some(f)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use good_lp::{Expression, ProblemVariables, Solution, Variable};
    use rusty_money::{Money, iso};
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::Discount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::PromotionKey,
        solvers::{SolverError, ilp::promotions::ILPPromotion},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::{ILPState, PromotionVars, SimpleDiscount};

    fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
        let currency = items
            .first()
            .map_or(iso::GBP, |item| item.price().currency());

        ItemGroup::new(items.into_iter().collect(), currency)
    }

    #[derive(Debug)]
    struct AlwaysSelectedVars;

    impl PromotionVars for AlwaysSelectedVars {
        fn add_item_presence_term(&self, expr: Expression, _item_idx: usize) -> Expression {
            expr
        }

        fn is_item_selected(&self, _solution: &dyn Solution, _item_idx: usize) -> bool {
            true
        }
    }

    #[derive(Debug)]
    struct NeverSelectedVars;

    impl PromotionVars for NeverSelectedVars {
        fn add_item_presence_term(&self, expr: Expression, _item_idx: usize) -> Expression {
            expr
        }

        fn is_item_selected(&self, _solution: &dyn Solution, _item_idx: usize) -> bool {
            false
        }
    }

    #[test]
    fn is_applicable_returns_false_for_empty_items() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), iso::GBP);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn add_variables_errors_on_discount_error() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetCheapestItemPrice(Money::from_minor(50, iso::USD)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let result = promo.add_variables(&item_group, &mut state);

        assert!(matches!(result, Err(SolverError::Discount(_))));
    }

    #[test]
    fn add_variables_errors_on_nonrepresentable_minor_units() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(9_007_199_254_740_993, iso::GBP)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let result = promo.add_variables(&item_group, &mut state);

        assert!(matches!(
            result,
            Err(SolverError::MinorUnitsNotRepresentable { .. })
        ));
    }

    #[test]
    fn calculate_item_discounts_skips_on_discount_error() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetCheapestItemPrice(Money::from_minor(50, iso::USD)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();

        let result = promo.calculate_item_discounts(&solution, &vars, &item_group);

        assert!(matches!(result, Err(SolverError::Discount(_))));
    }

    #[test]
    fn calculate_item_discounts_skips_unselected_items() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let vars = NeverSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();

        let discounts = promo.calculate_item_discounts(&solution, &vars, &item_group)?;

        assert!(discounts.is_empty());

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_returns_discounted_values() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();

        let discounts = promo.calculate_item_discounts(&solution, &vars, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 50)));

        Ok(())
    }

    #[test]
    fn calculate_item_applications_returns_applications_with_unique_bundle_ids() {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, iso::GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, iso::GBP)),
        ];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();
        let mut next_bundle_id = 0_usize;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &solution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        );

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
            Some(Money::from_minor(100, iso::GBP))
        );
        assert_eq!(
            apps.first().map(|a| a.final_price),
            Some(Money::from_minor(50, iso::GBP))
        );
        assert_eq!(
            apps.get(1).map(|a| a.original_price),
            Some(Money::from_minor(200, iso::GBP))
        );
        assert_eq!(
            apps.get(1).map(|a| a.final_price),
            Some(Money::from_minor(50, iso::GBP))
        );
    }

    #[test]
    fn calculate_item_applications_skips_on_discount_error() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetCheapestItemPrice(Money::from_minor(50, iso::USD)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();
        let mut next_bundle_id = 0_usize;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &solution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        );

        assert!(apps.is_empty());

        // next_bundle_id should not have been incremented
        assert_eq!(next_bundle_id, 0);
    }

    #[test]
    fn calculate_item_applications_continues_bundle_id_counter() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, iso::GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();
        // Start with a non-zero bundle_id (e.g., from previous promotions)
        let mut next_bundle_id = 5_usize;

        let apps = promo.calculate_item_applications(
            PromotionKey::default(),
            &solution,
            &vars,
            &item_group,
            &mut next_bundle_id,
        );

        assert_eq!(apps.len(), 1);
        assert_eq!(apps.first().map(|a| a.bundle_id), Some(5));
        assert_eq!(next_bundle_id, 6);
    }
}
