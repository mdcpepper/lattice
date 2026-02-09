//! Integration test proving ILP extensibility via custom promotion traits.

use good_lp::{Expression, Solution, Variable, variable};
use rustc_hash::FxHashMap;
use rusty_money::{Money, iso::GBP};
use smallvec::SmallVec;
use testresult::TestResult;

use lattice::{
    discounts::SimpleDiscount,
    items::{Item, groups::ItemGroup},
    prelude::promotion,
    products::ProductKey,
    promotions::{
        PromotionKey,
        applications::PromotionApplication,
        budget::PromotionBudget,
        prelude::{ILPPromotion, ILPPromotionVars, ILPState, PromotionVars, i64_to_f64_exact},
        types::DirectDiscountPromotion,
    },
    solvers::{
        Solver, SolverError,
        ilp::{BINARY_THRESHOLD, ILPSolver, observer::ILPObserver},
    },
    tags::string::StringTagCollection,
};

#[derive(Debug)]
struct ExternalCustomPromotion {
    key: PromotionKey,
    final_minor: i64,
}

#[derive(Debug)]
struct ExternalCustomPromotionVars {
    key: PromotionKey,
    final_minor: i64,
    item_participation: SmallVec<[(usize, Variable); 10]>,
}

impl ILPPromotionVars for ExternalCustomPromotionVars {
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
        // Allow at most one discounted item from this promotion.
        let expr: Expression = self.item_participation.iter().map(|(_, var)| *var).sum();

        observer.on_promotion_constraint(self.key, "external custom limit", &expr, "<=", 1.0);

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

impl ILPPromotion for ExternalCustomPromotion {
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
        let coeff = i64_to_f64_exact(self.final_minor)
            .ok_or(SolverError::MinorUnitsNotRepresentable(self.final_minor))?;

        let mut item_participation = SmallVec::new();

        for (item_idx, _item) in item_group.iter().enumerate() {
            let var = state.problem_variables_mut().add(variable().binary());

            state.add_to_objective(var, coeff);
            observer.on_promotion_variable(self.key, item_idx, var, self.final_minor, Some("ext"));
            observer.on_objective_term(var, coeff);

            item_participation.push((item_idx, var));
        }

        Ok(Box::new(ExternalCustomPromotionVars {
            key: self.key,
            final_minor: self.final_minor,
            item_participation,
        }))
    }
}

#[test]
fn solve_supports_external_custom_promotion_types() -> TestResult {
    let items = [
        Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
        Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
    ];
    let item_group = ItemGroup::new(items.into_iter().collect(), GBP);

    let promotion = promotion(ExternalCustomPromotion {
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
fn solve_handles_builtin_and_external_promotions() -> TestResult {
    let items = [
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fruit"]),
        ),
        Item::with_tags(
            ProductKey::default(),
            Money::from_minor(200, GBP),
            StringTagCollection::from_strs(&["veg"]),
        ),
    ];

    let item_group = ItemGroup::new(items.into_iter().collect(), GBP);

    let external = promotion(ExternalCustomPromotion {
        key: PromotionKey::default(),
        final_minor: 1,
    });

    let promotion = promotion(DirectDiscountPromotion::new(
        PromotionKey::default(),
        lattice::promotions::qualification::Qualification::match_any(
            StringTagCollection::from_strs(&["fruit"]),
        ),
        SimpleDiscount::AmountOverride(Money::from_minor(10, GBP)),
        PromotionBudget::unlimited(),
    ));

    let result = ILPSolver::solve(&[promotion, external], &item_group)?;

    // External promotion should win on the highest-priced item (200 -> 1),
    // while the built-in direct discount should still apply to "fruit" (100 -> 10).
    assert_eq!(result.total.to_minor_units(), 11);
    assert_eq!(result.affected_items.len(), 2);
    assert!(result.unaffected_items.is_empty());
    assert_eq!(result.promotion_applications.len(), 2);

    Ok(())
}
