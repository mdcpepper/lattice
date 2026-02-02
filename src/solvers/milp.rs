//! MILP Solver

use good_lp::{
    Expression, ProblemVariables, Solution, SolverModel, Variable, constraint, variable,
};
use num_traits::ToPrimitive;
use rusty_money::{Money, iso};
use smallvec::{SmallVec, smallvec};

#[cfg(feature = "solver-highs")]
use good_lp::solvers::highs::highs as default_solver;
#[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
use good_lp::solvers::microlp::microlp as default_solver;

use crate::{
    basket::Basket,
    solvers::{Solver, SolverError, SolverResult},
};

/// Solver using Mixed Integer Linear Programming (MILP)
#[derive(Debug)]
pub struct MILPSolver;

impl Solver for MILPSolver {
    fn solve<'a>(basket: &'a Basket<'a>, items: &[usize]) -> Result<SolverResult<'a>, SolverError> {
        if items.is_empty() {
            return Ok(SolverResult {
                affected_items: SmallVec::with_capacity(0),
                unaffected_items: SmallVec::with_capacity(0),
                total: Money::from_minor(0, basket.currency()),
            });
        }

        // Create problem variables
        let mut pb = ProblemVariables::new();

        let (z, exp_cost) = build_presence_variables_and_objective(basket, items, &mut pb)?;

        // Create the solver model
        let mut model = pb.minimise(exp_cost).using(default_solver);

        // Base Constraint: every item must be present. Without this (or promotions adding their own
        // constraints), the minimiser will choose z_i = 0 for every i and the cost will always be 0.
        for var in &z {
            model = model.with(constraint::eq(*var, 1));
        }

        let solution = model.solve()?;

        // Mark all items as unused initially
        let mut used_items: SmallVec<[bool; 10]> = smallvec![false; items.len()];

        // The total cost of this set of items
        let mut total = Money::from_minor(0, basket.currency());

        // The indexes of items that are being affected by promotions
        let affected_items = SmallVec::new();

        // Convert the MILP solution back into our domain result

        let mut unaffected_items = SmallVec::new();

        collect_unaffected_items(
            basket,
            &solution,
            &z,
            items,
            &mut used_items,
            &mut unaffected_items,
            &mut total,
        )?;

        Ok(SolverResult {
            affected_items,
            unaffected_items,
            total,
        })
    }
}

fn build_presence_variables_and_objective<'a>(
    basket: &'a Basket<'a>,
    items: &[usize],
    pb: &mut ProblemVariables,
) -> Result<(SmallVec<[Variable; 10]>, Expression), SolverError> {
    // Add binary variables for item selection. Each item must be present in the
    // solution, whether a promotion is applied to it or not. This prevents
    // items from disappearing from the result if they did not match anything.
    let z: SmallVec<[Variable; 10]> = (0..items.len())
        .map(|_| pb.add(variable().binary()))
        .collect();

    // Create expression for total cost. This is what we are trying to minimise.
    let mut exp_cost = Expression::default();

    // Add base cost for all items at their undiscounted price.
    z.iter().copied().zip(items.iter().copied()).try_for_each(
        |(var, item_idx)| -> Result<(), SolverError> {
            let minor_units = basket.get_item(item_idx)?.price().to_minor_units();

            // `good_lp` stores coefficients as `f64`. Only integers with absolute value <= 2^53
            // can be represented exactly in an IEEE-754 `f64` mantissa; enforce that via a
            // round-trip check so we never silently change the objective.
            exp_cost += var
                * i64_to_f64_exact(minor_units)
                    .ok_or(SolverError::MinorUnitsNotRepresentable { minor_units })?;

            Ok(())
        },
    )?;

    Ok((z, exp_cost))
}

/// Collect unaffected items and their total price.
fn collect_unaffected_items<'a>(
    basket: &'a Basket<'a>,
    solution: &impl Solution,
    z: &[Variable],
    items: &[usize],
    used_items: &mut [bool],
    unaffected_items: &mut SmallVec<[usize; 10]>,
    total: &mut Money<'a, iso::Currency>,
) -> Result<(), SolverError> {
    // Any item that wasn't claimed by a promotion is treated as an unaffected
    // full-price item and contributes its full price to the total.
    z.iter()
        .copied()
        .zip(items.iter().copied())
        .zip(used_items.iter_mut())
        .try_for_each(|((var, item_idx), used)| -> Result<(), SolverError> {
            // `var` is a binary decision variable; the solver return floats, so treat values
            // greater than 0.5 as "selected" (i.e. 1) to tolerate tiny numerical noise.
            if solution.value(var) > 0.5 && !*used {
                // Add the item to the list of unaffected items.
                unaffected_items.push(item_idx);

                // Add the item's full price to the result total.
                *total = total.add(*basket.get_item(item_idx)?.price())?;

                // Mark the item as used.
                *used = true;
            }

            Ok(())
        })?;

    Ok(())
}

/// Convert an `i64` to an `f64` if it can be represented exactly.
fn i64_to_f64_exact(v: i64) -> Option<f64> {
    let f = v.to_f64()?;

    (f.to_i64() == Some(v)).then_some(f)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rusty_money::iso;
    use testresult::TestResult;

    use crate::items::Item;

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(Money::from_minor(100, iso::GBP)),
            Item::new(Money::from_minor(200, iso::GBP)),
            Item::new(Money::from_minor(300, iso::GBP)),
        ]
    }

    #[test]
    fn solver_returns_all_items_full_price_with_no_promotions() -> TestResult {
        let items = test_items();

        let subtotal = items
            .iter()
            .map(|item| item.price().to_minor_units())
            .sum::<i64>();

        let basket = Basket::with_items(items, iso::GBP)?;

        let result = MILPSolver::solve(&basket, &[0, 1, 2])?;

        assert_eq!(subtotal, result.total.to_minor_units());
        assert_eq!(0, result.affected_items.len());
        assert_eq!(3, result.unaffected_items.len());

        Ok(())
    }

    #[test]
    fn solver_with_no_items_returns_empty_result() -> TestResult {
        let basket = Basket::with_items([], iso::GBP)?;

        let result = MILPSolver::solve(&basket, &[])?;

        assert_eq!(result.total.to_minor_units(), 0);
        assert!(result.affected_items.is_empty());
        assert!(result.unaffected_items.is_empty());

        Ok(())
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "This is a test case for exact conversion"
    )]
    fn i64_to_f64_exact_accepts_exactly_representable_integers() {
        let cases: [i64; 5] = [0, 1, -1, 123, 9_007_199_254_740_992]; // 2^53

        for v in cases {
            assert_eq!(i64_to_f64_exact(v), Some(v as f64));
        }
    }

    #[test]
    fn i64_to_f64_exact_rejects_nonrepresentable_integers() {
        let cases: [i64; 2] = [9_007_199_254_740_993, -9_007_199_254_740_993]; // 2^53 + 1

        for v in cases {
            assert_eq!(i64_to_f64_exact(v), None);
        }
    }

    #[test]
    fn base_objective_matches_sum_of_item_minor_units() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let mut pb = ProblemVariables::new();
        let (z, objective) = build_presence_variables_and_objective(&basket, &[0, 1, 2], &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let expected = 600.0_f64;
        let actual = solution.eval(&objective);

        assert!((actual - expected).abs() <= f64::EPSILON);

        Ok(())
    }

    #[test]
    fn unaffected_items_collection_skips_pre_used_items() -> TestResult {
        let items = test_items();
        let basket = Basket::with_items(items, iso::GBP)?;

        let mut pb = ProblemVariables::new();
        let (z, _objective) = build_presence_variables_and_objective(&basket, &[0, 1, 2], &mut pb)?;

        let solution: HashMap<Variable, f64> = z.iter().copied().map(|v| (v, 1.0)).collect();

        let mut used_items = vec![false; 3];
        used_items[1] = true; // pretend item 1 was claimed by a promotion

        let mut unaffected_items = SmallVec::new();
        let mut total = Money::from_minor(0, basket.currency());

        collect_unaffected_items(
            &basket,
            &solution,
            &z,
            &[0, 1, 2],
            &mut used_items,
            &mut unaffected_items,
            &mut total,
        )?;

        assert_eq!(unaffected_items.as_slice(), &[0, 2]);
        assert_eq!(total.to_minor_units(), 400);

        Ok(())
    }
}
