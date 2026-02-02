//! Simple Promotions ILP

use std::slice;

use good_lp::{Expression, ProblemVariables, Solution, Variable, variable};
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    basket::Basket,
    discounts::calculate_discount,
    promotions::simple_discount::SimpleDisount,
    solvers::{
        SolverError,
        ilp::{BINARY_THRESHOLD, promotions::ILPPromotion},
    },
    tags::collection::TagCollection,
};

use super::PromotionVars;

#[derive(Debug)]
pub struct SimpleDiscountVars {
    item_vars: SmallVec<[(usize, Variable); 10]>,
}

impl PromotionVars for SimpleDiscountVars {
    fn add_item_usage(&self, usage: Expression, item_idx: usize) -> Expression {
        let mut new_usage = usage;

        for &(idx, var) in &self.item_vars {
            if idx == item_idx {
                new_usage += var;
            }
        }

        new_usage
    }

    fn is_item_selected(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.item_vars
            .iter()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }
}

impl<'a> ILPPromotion<'a> for SimpleDisount<'a> {
    fn is_applicable(&self, basket: &'a Basket<'a>, items: &[usize]) -> bool {
        if items.is_empty() {
            return false;
        }

        let promotion_tags = self.tags();

        if promotion_tags.is_empty() {
            return true;
        }

        items.iter().copied().any(|item_idx| {
            basket
                .get_item(item_idx)
                .map(|item| item.tags().intersects(promotion_tags))
                .unwrap_or(false)
        })
    }

    fn add_variables(
        &self,
        basket: &'a Basket<'a>,
        items: &[usize],
        pb: &mut ProblemVariables,
        cost: &mut Expression,
    ) -> Result<Box<dyn PromotionVars>, SolverError> {
        // An empty tag set means "this promotion can target any item", so we can skip tag checks
        // if that is the case.
        let match_all = self.tags().is_empty();

        // Keep the mapping from basket item index -> solver variable so we can interpret solutions later.
        let mut item_vars: SmallVec<[(usize, Variable); 10]> = SmallVec::new();

        for &item_idx in items {
            let item = basket.get_item(item_idx).map_err(SolverError::from)?;

            // Enforce the promotion's targeting rules up-front so the solver doesn't need extra constraints.
            if !match_all && !item.tags().intersects(self.tags()) {
                continue;
            }

            // Compute the discounted price in minor units; if the discount can't be computed, skip the item.
            let discounted_minor = match calculate_discount(self.discount(), slice::from_ref(item))
            {
                Ok(price) => price.to_minor_units(),
                Err(err) => return Err(err.into()),
            };

            // `good_lp` uses floating point coefficients; only accept values that are exactly representable
            // to avoid tiny rounding changing the solver's preferred choice.
            let Some(coeff) = i64_to_f64_exact(discounted_minor) else {
                return Err(SolverError::MinorUnitsNotRepresentable {
                    minor_units: discounted_minor,
                });
            };

            // A binary decision variable lets the solver choose whether to apply this promotion to the item.
            let var = pb.add(variable().binary());

            // Persist the variable so we can later mark items as "selected" from the solved model.
            item_vars.push((item_idx, var));

            // Add this item's discounted cost contribution to the objective expression.
            *cost += var * coeff;
        }

        Ok(Box::new(SimpleDiscountVars { item_vars }))
    }

    fn add_constraints<S: good_lp::SolverModel>(
        &self,
        model: S,
        _vars: &dyn PromotionVars,
        _basket: &'a Basket<'a>,
        _items: &[usize],
    ) -> S {
        // Return the model without additional constraints
        model
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &dyn PromotionVars,
        basket: &'a Basket<'a>,
        items: &[usize],
    ) -> FxHashMap<usize, (i64, i64)> {
        let mut discounts = FxHashMap::default();

        for &item_idx in items {
            if !vars.is_item_selected(solution, item_idx) {
                continue;
            }

            let Ok(item) = basket.get_item(item_idx) else {
                continue;
            };

            let discounted_minor = match calculate_discount(self.discount(), slice::from_ref(item))
            {
                Ok(price) => price.to_minor_units(),
                Err(_) => continue,
            };

            let original_minor = item.price().to_minor_units();

            discounts.insert(item_idx, (original_minor, discounted_minor));
        }

        discounts
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
    use testresult::TestResult;

    use crate::{
        basket::Basket,
        discounts::Discount,
        items::Item,
        solvers::{SolverError, ilp::promotions::ILPPromotion},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::{PromotionVars, SimpleDisount};

    #[derive(Debug)]
    struct AlwaysSelectedVars;

    impl PromotionVars for AlwaysSelectedVars {
        fn add_item_usage(&self, usage: Expression, _item_idx: usize) -> Expression {
            usage
        }

        fn is_item_selected(&self, _solution: &dyn Solution, _item_idx: usize) -> bool {
            true
        }
    }

    #[test]
    fn is_applicable_returns_false_for_empty_items() -> TestResult {
        let basket = Basket::with_items([], iso::GBP)?;
        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        assert!(!promo.is_applicable(&basket, &[]));

        Ok(())
    }

    #[test]
    fn add_variables_errors_on_missing_item_indices() -> TestResult {
        let items = [Item::new(Money::from_minor(100, iso::GBP))];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let mut pb = ProblemVariables::new();
        let mut cost = Expression::default();
        let result = promo.add_variables(&basket, &[0, 1], &mut pb, &mut cost);

        assert!(matches!(result, Err(SolverError::Basket(_))));

        Ok(())
    }

    #[test]
    fn add_variables_errors_on_discount_error() -> TestResult {
        let items = [Item::new(Money::from_minor(100, iso::GBP))];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetCheapestItemPrice(Money::from_minor(50, iso::USD)),
        );

        let mut pb = ProblemVariables::new();
        let mut cost = Expression::default();
        let result = promo.add_variables(&basket, &[0], &mut pb, &mut cost);

        assert!(matches!(result, Err(SolverError::Discount(_))));

        Ok(())
    }

    #[test]
    fn add_variables_errors_on_nonrepresentable_minor_units() -> TestResult {
        let items = [Item::new(Money::from_minor(100, iso::GBP))];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(9_007_199_254_740_993, iso::GBP)),
        );

        let mut pb = ProblemVariables::new();
        let mut cost = Expression::default();
        let result = promo.add_variables(&basket, &[0], &mut pb, &mut cost);

        assert!(matches!(
            result,
            Err(SolverError::MinorUnitsNotRepresentable { .. })
        ));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_skips_missing_items() -> TestResult {
        let items = [Item::new(Money::from_minor(100, iso::GBP))];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();

        let discounts = promo.calculate_item_discounts(&solution, &vars, &basket, &[1]);

        assert!(discounts.is_empty());

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_skips_on_discount_error() -> TestResult {
        let items = [Item::new(Money::from_minor(100, iso::GBP))];
        let basket = Basket::with_items(items, iso::GBP)?;

        let promo = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetCheapestItemPrice(Money::from_minor(50, iso::USD)),
        );

        let vars = AlwaysSelectedVars;
        let solution: HashMap<Variable, f64> = HashMap::new();

        let discounts = promo.calculate_item_discounts(&solution, &vars, &basket, &[0]);

        assert!(discounts.is_empty());

        Ok(())
    }
}
