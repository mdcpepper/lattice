use good_lp::{Expression, Solution, SolutionStatus, Variable};
use rustc_hash::FxHashMap;
use rusty_money::{Money, iso::GBP};
use smallvec::SmallVec;

use crate::{
    items::{Item, groups::ItemGroup},
    products::ProductKey,
    promotions::PromotionKey,
    solvers::ilp::{
        observer::ILPObserver,
        state::{ConstraintRelation, ILPConstraint},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct RecordedPromotionConstraint {
    pub(crate) constraint_type: String,
    pub(crate) expr: Expression,
    pub(crate) relation: String,
    pub(crate) rhs: f64,
}

#[derive(Debug, Default)]
pub(crate) struct RecordingObserver {
    pub(crate) objective_terms: Vec<(Variable, f64)>,
    pub(crate) promotion_constraints: Vec<RecordedPromotionConstraint>,
}

#[derive(Debug, Default)]
pub(crate) struct CountingObserver {
    pub(crate) promotion_variables: usize,
    pub(crate) objective_terms: usize,
    pub(crate) promotion_constraints: usize,
}

#[derive(Debug, Default)]
pub(crate) struct MapSolution {
    values: FxHashMap<Variable, f64>,
}

impl MapSolution {
    pub(crate) fn with(values: &[(Variable, f64)]) -> Self {
        let mut map = FxHashMap::default();

        for (var, value) in values {
            map.insert(*var, *value);
        }

        Self { values: map }
    }
}

impl Solution for MapSolution {
    fn status(&self) -> SolutionStatus {
        SolutionStatus::Optimal
    }

    fn value(&self, variable: Variable) -> f64 {
        *self.values.get(&variable).unwrap_or(&0.0)
    }
}

#[derive(Debug)]
pub(crate) struct SelectAllSolution;

impl Solution for SelectAllSolution {
    fn status(&self) -> SolutionStatus {
        SolutionStatus::Optimal
    }

    fn value(&self, _variable: Variable) -> f64 {
        1.0
    }
}

#[derive(Debug)]
pub(crate) struct SelectNoneSolution;

impl Solution for SelectNoneSolution {
    fn status(&self) -> SolutionStatus {
        SolutionStatus::Optimal
    }

    fn value(&self, _variable: Variable) -> f64 {
        0.0
    }
}

pub(crate) fn item_group_from_prices(prices: &[i64]) -> ItemGroup<'_> {
    let items: SmallVec<[Item<'_>; 10]> = prices
        .iter()
        .map(|&price| Item::new(ProductKey::default(), Money::from_minor(price, GBP)))
        .collect();

    ItemGroup::new(items, GBP)
}

pub(crate) fn item_group_from_items<const N: usize>(items: [Item<'_>; N]) -> ItemGroup<'_> {
    let currency = items.first().map_or(GBP, |item| item.price().currency());

    ItemGroup::new(items.into_iter().collect(), currency)
}

impl ILPObserver for RecordingObserver {
    fn on_presence_variable(&mut self, _item_idx: usize, _var: Variable, _price_minor: i64) {}

    fn on_promotion_variable(
        &mut self,
        _promotion_key: PromotionKey,
        _item_idx: usize,
        _var: Variable,
        _discounted_price_minor: i64,
        _metadata: Option<&str>,
    ) {
    }

    fn on_objective_term(&mut self, var: Variable, coefficient: f64) {
        self.objective_terms.push((var, coefficient));
    }

    fn on_exclusivity_constraint(&mut self, _item_idx: usize, _constraint_expr: &Expression) {}

    fn on_promotion_constraint(
        &mut self,
        _promotion_key: PromotionKey,
        constraint_type: &str,
        constraint_expr: &Expression,
        relation: &str,
        rhs: f64,
    ) {
        self.promotion_constraints
            .push(RecordedPromotionConstraint {
                constraint_type: constraint_type.to_string(),
                expr: constraint_expr.clone(),
                relation: relation.to_string(),
                rhs,
            });
    }
}

impl ILPObserver for CountingObserver {
    fn on_presence_variable(&mut self, _item_idx: usize, _var: Variable, _price_minor: i64) {}

    fn on_promotion_variable(
        &mut self,
        _promotion_key: PromotionKey,
        _item_idx: usize,
        _var: Variable,
        _discounted_price_minor: i64,
        _metadata: Option<&str>,
    ) {
        self.promotion_variables += 1;
    }

    fn on_exclusivity_constraint(&mut self, _item_idx: usize, _constraint_expr: &Expression) {}

    fn on_promotion_constraint(
        &mut self,
        _promotion_key: PromotionKey,
        _constraint_type: &str,
        _constraint_expr: &Expression,
        _relation: &str,
        _rhs: f64,
    ) {
        self.promotion_constraints += 1;
    }

    fn on_objective_term(&mut self, _var: Variable, _coefficient: f64) {
        self.objective_terms += 1;
    }
}

pub(crate) fn assert_relation_holds(lhs: f64, relation: &str, rhs: f64) {
    const EPS: f64 = 1e-9;

    match relation {
        "=" => assert!((lhs - rhs).abs() <= EPS, "Expected {lhs} = {rhs}"),
        "<=" => assert!(lhs <= rhs + EPS, "Expected {lhs} <= {rhs}"),
        ">=" => assert!(lhs + EPS >= rhs, "Expected {lhs} >= {rhs}"),
        other => panic!("Unsupported relation: {other}"),
    }
}

pub(crate) fn assert_state_constraints_hold<S: Solution>(
    constraints: &[ILPConstraint],
    solution: &S,
) {
    const EPS: f64 = 1e-9;

    for constraint in constraints {
        let lhs = solution.eval(&constraint.lhs);

        match constraint.relation {
            ConstraintRelation::Eq => {
                assert!(
                    (lhs - constraint.rhs).abs() <= EPS,
                    "Expected {lhs} = {}",
                    constraint.rhs
                );
            }
            ConstraintRelation::Leq => {
                assert!(
                    lhs <= constraint.rhs + EPS,
                    "Expected {lhs} <= {}",
                    constraint.rhs
                );
            }
            ConstraintRelation::Geq => {
                assert!(
                    lhs + EPS >= constraint.rhs,
                    "Expected {lhs} >= {}",
                    constraint.rhs
                );
            }
        }
    }
}

pub(crate) fn observed_lhs_values_for_type<S: Solution>(
    observer: &RecordingObserver,
    constraint_type: &str,
    solution: &S,
) -> Vec<f64> {
    observer
        .promotion_constraints
        .iter()
        .filter(|record| record.constraint_type == constraint_type)
        .map(|record| solution.eval(&record.expr))
        .collect()
}

pub(crate) fn state_lhs_values_for_relation<S: Solution>(
    constraints: &[ILPConstraint],
    relation: ConstraintRelation,
    solution: &S,
) -> Vec<f64> {
    let mut values: Vec<f64> = constraints
        .iter()
        .filter(|constraint| constraint.relation == relation)
        .map(|constraint| solution.eval(&constraint.lhs))
        .collect();

    values.sort_by(f64::total_cmp);

    values
}
