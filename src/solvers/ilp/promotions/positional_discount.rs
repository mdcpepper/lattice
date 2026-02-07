//! Positional Discount Promotions ILP

#[cfg(test)]
use std::any::Any;

use good_lp::{Expression, Solution, Variable, variable};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    discounts::{SimpleDiscount, percent_of_minor},
    items::groups::ItemGroup,
    promotions::{
        PromotionKey, applications::PromotionApplication, types::PositionalDiscountPromotion,
    },
    solvers::{
        SolverError,
        ilp::{
            BINARY_THRESHOLD, ILPObserver, i64_to_f64_exact,
            promotions::{ILPPromotion, ILPPromotionVars, PromotionVars},
            state::ILPState,
        },
    },
    tags::collection::TagCollection,
};

#[derive(Debug, Clone, Copy)]
enum PositionalRuntimeDiscount {
    PercentageOff(decimal_percentage::Percentage),
    AmountOverride(i64),
    AmountOff(i64),
}

/// Solver variables for a positional discount promotion.
///
/// Tracks the mapping from item group indices to their corresponding
/// binary decision variables in the ILP model.
#[derive(Debug)]
pub struct PositionalDiscountVars {
    /// Promotion key for observer metadata.
    promotion_key: PromotionKey,

    /// Sorted eligible items: (`item_group_index`, `price_minor`)
    eligible_items: SmallVec<[(usize, i64); 10]>,

    /// Participation variables: `eligible_items[i]` participates in promotion
    item_participation: SmallVec<[(usize, Variable); 10]>,

    /// Discount variables: `eligible_items[i]` receives discount
    item_discounts: SmallVec<[(usize, Variable); 10]>,

    /// DFA constraint data
    dfa_data: Option<PositionalDFAConstraintData>,

    /// Runtime discount mode captured during variable creation.
    runtime_discount: PositionalRuntimeDiscount,

    /// Bundle size copied from promotion config.
    bundle_size: usize,

    /// Budget: optional max applications.
    application_limit: Option<u32>,

    /// Budget: optional max total discount value in minor units.
    monetary_limit_minor: Option<i64>,
}

/// Data needed to construct DFA constraints.
#[derive(Debug)]
struct PositionalDFAConstraintData {
    /// Bundle size
    size: u16,

    /// 0-indexed positions within each bundle that receive discounts
    positions: SmallVec<[u16; 5]>,

    /// DFA state variables: `state_vars[pos][r]` where `r = (item_count mod size)`
    state_vars: SmallVec<[SmallVec<[Variable; 8]>; 12]>,

    /// DFA transition variables: `take_vars[pos][r]` = take item at pos when in state r
    take_vars: SmallVec<[SmallVec<[Variable; 8]>; 12]>,
}

impl PositionalDiscountVars {
    /// Check if an item is discounted based on the solution.
    pub fn is_item_discounted(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.item_discounts
            .iter()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }

    /// Add DFA constraints to the model.
    ///
    /// This models a small state machine that walks over the eligible items
    /// in order and tracks progress through a bundle.
    ///
    /// At each item position, the machine is in exactly one state, which
    /// represents how many items of the current bundle have already been
    /// collected. The machine starts in the "empty bundle" state, and must
    /// also end in that state, which ensures that only complete bundles of
    /// the configured size required are formed, and partial bundles are not
    /// allowed.
    ///
    /// When an item is taken as part of a bundle, the state machine advances
    /// to the next state. When an item is skipped, the state stays the same.
    /// These state transitions are enforced with linear constraints so that
    /// only valid sequences of takes and skips are possible.
    ///
    /// The model then links these transitions back to the rest of the pricing
    /// logic: whether an item participates in the promotion, and whether it
    /// receives a discount, is determined by the position where it falls within
    /// the bundle.
    ///
    /// - [Example for a "Buy one get one free" promotion](https://mermaid.live/view#pako:eNpdUsFu2zAM_RWCQAF3SAIrTWJHh13aS4Gd1p0296DajCM0lgxJDrYF_fdRip0s1cnSe3zvkfQJa9sQSvRBBXrSqnWqmx-XlQE-jXZUB20NfPtemfPbry-vMJ9_hZccJHCVCxOSJKBCRrIcdKDO31cIyjP3E0NAJhJjIohJ5O4Ofqh3guCU8Tp6e8hUc1SmJngbTHOg-zOTbVIOwTlCLIl6kPVKNxNDXJP-x9g5otmoBbXt-gOFKHoJ8PKu-9sAxkLvbOvI-0_uaQqRH7VvbcUtdJF_0r62gwmgjLE8EbY4Y3wjcLrdB7C7NBMYz3MMHlswoA2EvfbnWV4ZvCjSR_IMEu9tdEi9jonJNMlhSjJ2EPcpE5h1gw_wRoxwCc6wdbpBGdxAM-zIdSpe8RTLK2SfjiqU_NnQTg2HUGFlPrisV-antd1U6ezQ7lHu1MHzbeib6492obA9uccYGeU6KaA84W-Um4fFpigeRLkultttmTP4B6VYl4t8s1yVq2IlliLfrj5m-Dd55ouyWOd8RFEsy4KRf72J3lQ)
    /// - [Example for a "3-for-2" promotion](https://mermaid.live/view#pako:eNptkkFzmzAQhf_KjmYyQzo2RTIYrEMuybGnpqeWHhRYgyYgMZLItPX4v1cIYxo3OiF97-2-FTqRStdIOLFOOHySojGi376xUoFftTRYOakVfPlaqvnsx6efsN0-wHMCHLzLuIWEElAST6IEpMPe3pcEhPXaGwWFiAbFIqA3AgYRe1-CLW3u7uCbeEVwRigrp3QWIlG_CVUhvIyq7vB-VvogISn1Sd1kmQpeEJ0R-wCxdb4rgmiuDJXuhw7d1OIa5_lVDu_jKA2D0Y1Ba2-yhFub9P9noR8gtsb8B107P0lb6VE5EEppf3e--8z8DsHIpnWgj-Hy4LJWyzB0Ei20aHDFEcZNDEeDCJ-halEMaN3lVwURqjpUX1JcBpueBQ8w6kfveEFPvIVsSGNkTbgzI25Ij6YX05acJntJXIs9loT7zxqPYuxcSUp19rZBqO9a94vT6LFpCT-KzvrdONTre71KfHs0j9N0hGehAuEn8ovw_S7e5_mOFlnODoci8fA34TQr4mTP0iLNU8pockjPG_In9EziIs8Svw55ke3SdLc7_wVqEvAV)
    ///
    /// See:
    /// - <https://en.wikipedia.org/wiki/Deterministic_finite_automaton>
    #[expect(clippy::too_many_lines, reason = "Complex DFA constraint logic")]
    pub fn add_dfa_constraints(
        &self,
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        let Some(dfa_data) = &self.dfa_data else {
            return;
        };

        // Number of items in the group that could _potentially_ participate in this promotion
        let num_eligible = self.eligible_items.len();

        // The size of a single bundle (e.g. 3 for a "3-for-2" style promotion)
        let bundle_size = dfa_data.size as usize;

        // Constraint 1: Exactly one state active at each position (including final)
        for states_at_pos in &dfa_data.state_vars {
            let state_sum: Expression = states_at_pos.iter().copied().sum();

            observer.on_promotion_constraint(
                promotion_key,
                "DFA state uniqueness",
                &state_sum,
                "=",
                1.0,
            );

            state.add_eq_constraint(state_sum, 1.0);
        }

        // Constraint 2: Start and end in state 0 (complete bundles only)
        // state_vars[0] is the initial state, state_vars[num_eligible] is the final state
        if let Some(&first_var) = dfa_data.state_vars.first().and_then(|s| s.first()) {
            let expr = Expression::from(first_var);

            observer.on_promotion_constraint(promotion_key, "DFA initial state", &expr, "=", 1.0);

            state.add_eq_constraint(expr, 1.0);
        }

        if let Some(&last_var) = dfa_data.state_vars.last().and_then(|s| s.first()) {
            let expr = Expression::from(last_var);

            observer.on_promotion_constraint(promotion_key, "DFA final state", &expr, "=", 1.0);

            state.add_eq_constraint(expr, 1.0);
        }

        // Constraint 3: DFA state transitions
        //
        // As we move from one item to the next:
        // - If the current item is taken as part of a bundle, we advance the
        //   bundle progress to the next state.
        // - If it is not taken, the state stays the same.
        for pos in 0..num_eligible {
            for r in 0..bundle_size {
                // Previous state in the bundle cycle (wrapping around).
                let r_prev = if r == 0 { bundle_size - 1 } else { r - 1 };

                // State at the next item position.
                let Some(next_state) = dfa_data
                    .state_vars
                    .get(pos + 1)
                    .and_then(|s| s.get(r).copied())
                else {
                    continue;
                };

                // State at the current item position.
                let Some(curr_state) = dfa_data.state_vars.get(pos).and_then(|s| s.get(r).copied())
                else {
                    continue;
                };

                // Decision: take this item and advance _from_ state r.
                let Some(take_curr) = dfa_data.take_vars.get(pos).and_then(|t| t.get(r).copied())
                else {
                    continue;
                };

                // Decision: take this item and advance _into_ state r
                // (ie coming from the previous state).
                let Some(take_prev) = dfa_data
                    .take_vars
                    .get(pos)
                    .and_then(|t| t.get(r_prev).copied())
                else {
                    continue;
                };

                // Maintain consistent bundle progress:
                // next_state = current_state - leaving this state + entering from the previous state
                let transition_expr = curr_state - take_curr + take_prev;

                let expr = Expression::from(next_state) - transition_expr.clone();

                observer.on_promotion_constraint(
                    promotion_key,
                    "DFA state transition",
                    &expr,
                    "=",
                    0.0,
                );

                state.add_eq_constraint(Expression::from(next_state) - transition_expr, 0.0);
            }
        }

        // Constraint 4: Link participation to transitions
        for eligible_idx in 0..num_eligible {
            let take_sum: Expression = dfa_data
                .take_vars
                .get(eligible_idx)
                .map(|takes| takes.iter().copied().sum())
                .unwrap_or_default();

            // An item participates in the promotion if and only if it is "taken"
            // by the DFA in one of the bundle states.
            if let Some(&(_idx, participation_var)) = self.item_participation.get(eligible_idx) {
                let expr = Expression::from(participation_var);

                let observed_expr = Expression::from(participation_var) - take_sum.clone();

                observer.on_promotion_constraint(
                    promotion_key,
                    "DFA link participation",
                    &observed_expr,
                    "=",
                    0.0,
                );

                state.add_eq_constraint(expr - take_sum, 0.0);
            }
        }

        // Constraint 5: Link discount to transitions
        for eligible_idx in 0..num_eligible {
            let mut discount_sum = Expression::default();

            if let Some(takes) = dfa_data.take_vars.get(eligible_idx) {
                for r in 0..bundle_size {
                    // Only items that land in certain positions within the bundle
                    // (e.g. "the third item") receive a discount.
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "bundle_size is u16, and r < bundle_size"
                    )]
                    if dfa_data.positions.contains(&(r as u16))
                        && let Some(&take_var) = takes.get(r)
                    {
                        discount_sum += take_var;
                    }
                }
            }

            if let Some(&(_idx, discount_var)) = self.item_discounts.get(eligible_idx) {
                let expr = Expression::from(discount_var);

                let observed_expr = Expression::from(discount_var) - discount_sum.clone();

                observer.on_promotion_constraint(
                    promotion_key,
                    "DFA link discount",
                    &observed_expr,
                    "=",
                    0.0,
                );

                state.add_eq_constraint(expr - discount_sum, 0.0);
            }
        }

        // Constraint 6: Restrict transitions to valid states
        for pos in 0..num_eligible {
            for r in 0..bundle_size {
                // We can only take an item using a given bundle state if the DFA
                // is actually in that state at this position.
                if let (Some(take_var), Some(state_var)) = (
                    // Look at all "take" decisions for this item.
                    dfa_data.take_vars.get(pos).and_then(|t| t.get(r).copied()),
                    // Pick the one that corresponds to state `r`
                    dfa_data.state_vars.get(pos).and_then(|s| s.get(r).copied()),
                ) {
                    // We are only allowed to take an item in state `r` if the DFA is actually in state `r`:
                    //
                    // | take_var | state_var | Valid? | Why                                 |
                    // | -------- | --------- | ------ | ----------------------------------- |
                    // |        0 |         0 |    Y   | Not taking the item                 |
                    // |        0 |         1 |    Y   | State active, item not taken        |
                    // |        1 |         1 |    Y   | Taking item in active state         |
                    // |        1 |         0 |    N   | Taking item in a state we're not in |
                    let expr = Expression::from(take_var) - state_var;

                    observer.on_promotion_constraint(
                        promotion_key,
                        "DFA restrict transitions",
                        &expr,
                        "<=",
                        0.0,
                    );

                    state.add_leq_constraint(Expression::from(take_var) - state_var, 0.0);
                }
            }
        }
    }

    /// Add budget constraints for positional promotions
    pub fn add_budget_constraints(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        let promotion_key = self.promotion_key;
        let bundle_size = self.bundle_size;

        // Application limit: For positional, this limits bundles
        // Constraint: sum(participation_vars) <= application_limit * bundle_size
        if let Some(application_limit) = self.application_limit {
            let participation_sum: Expression =
                self.item_participation.iter().map(|(_, var)| *var).sum();

            let bundle_size_u32 =
                u32::try_from(bundle_size).map_err(|_e| SolverError::InvariantViolation {
                    message: "bundle size too large",
                })?;

            let max_items = i64::from(application_limit)
                .checked_mul(i64::from(bundle_size_u32))
                .ok_or(SolverError::InvariantViolation {
                    message: "application limit overflow",
                })?;

            let limit_f64 = i64_to_f64_exact(max_items)
                .ok_or(SolverError::MinorUnitsNotRepresentable(max_items))?;

            observer.on_promotion_constraint(
                promotion_key,
                "application count budget (bundle limit)",
                &participation_sum,
                "<=",
                limit_f64,
            );

            state.add_leq_constraint(participation_sum, limit_f64);
        }

        // Monetary limit: sum(discount_amount * discount_var) <= limit
        if let Some(limit_minor) = self.monetary_limit_minor {
            let mut discount_expr = Expression::default();

            for &(item_idx, discount_var) in &self.item_discounts {
                let item = item_group.get_item(item_idx).map_err(SolverError::from)?;

                let full_minor = item.price().to_minor_units();
                let discounted_minor =
                    calculate_discounted_minor_for_runtime(full_minor, self.runtime_discount)?;

                let discount_amount = full_minor.saturating_sub(discounted_minor);
                let coeff = i64_to_f64_exact(discount_amount)
                    .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                discount_expr += discount_var * coeff;
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

impl ILPPromotionVars for PositionalDiscountVars {
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

    fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.is_item_discounted(solution, item_idx)
    }

    fn add_constraints(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_dfa_constraints(promotion_key, state, observer);
        self.add_budget_constraints(item_group, state, observer)
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

            let original_minor = item.price().to_minor_units();

            let final_minor = if self.is_item_priced_by_promotion(solution, item_idx) {
                calculate_discounted_minor_for_runtime(original_minor, self.runtime_discount)?
            } else {
                original_minor
            };

            discounts.insert(item_idx, (original_minor, final_minor));
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
        let bundle_size = self.bundle_size;

        let mut participating_items: SmallVec<[(usize, i64); 10]> = SmallVec::new();

        for (item_idx, item) in item_group.iter().enumerate() {
            if self.is_item_participating(solution, item_idx) {
                participating_items.push((item_idx, item.price().to_minor_units()));
            }
        }

        participating_items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        for chunk in participating_items.chunks(bundle_size) {
            let bundle_id = *next_bundle_id;
            *next_bundle_id += 1;

            for &(item_idx, price_minor) in chunk {
                let item = item_group.get_item(item_idx)?;

                let final_price = if self.is_item_priced_by_promotion(solution, item_idx) {
                    Money::from_minor(
                        calculate_discounted_minor_for_runtime(price_minor, self.runtime_discount)?,
                        currency,
                    )
                } else {
                    Money::from_minor(price_minor, currency)
                };

                applications.push(PromotionApplication {
                    promotion_key,
                    item_idx,
                    bundle_id,
                    original_price: *item.price(),
                    final_price,
                });
            }
        }

        Ok(applications)
    }
}

fn positional_runtime_discount_from_config(
    discount: &SimpleDiscount<'_>,
) -> PositionalRuntimeDiscount {
    match discount {
        SimpleDiscount::PercentageOff(pct) => PositionalRuntimeDiscount::PercentageOff(*pct),
        SimpleDiscount::AmountOverride(amount) => {
            PositionalRuntimeDiscount::AmountOverride(amount.to_minor_units())
        }
        SimpleDiscount::AmountOff(amount) => {
            PositionalRuntimeDiscount::AmountOff(amount.to_minor_units())
        }
    }
}

fn calculate_discounted_minor_for_runtime(
    original_minor: i64,
    discount: PositionalRuntimeDiscount,
) -> Result<i64, SolverError> {
    let discount_minor = match discount {
        PositionalRuntimeDiscount::PercentageOff(pct) => {
            let discount_minor =
                percent_of_minor(&pct, original_minor).map_err(SolverError::Discount)?;

            original_minor.saturating_sub(discount_minor)
        }
        PositionalRuntimeDiscount::AmountOverride(amount_minor) => amount_minor,
        PositionalRuntimeDiscount::AmountOff(amount_minor) => {
            original_minor.saturating_sub(amount_minor)
        }
    };

    Ok(0.max(discount_minor))
}

impl ILPPromotion for PositionalDiscountPromotion<'_> {
    fn key(&self) -> PromotionKey {
        PositionalDiscountPromotion::key(self)
    }

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

    #[expect(
        clippy::too_many_lines,
        reason = "This function is long due to the DFA constraints."
    )]
    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError> {
        let promotion_key = self.key();
        let runtime_discount = positional_runtime_discount_from_config(self.discount());
        let bundle_size = self.size() as usize;
        let application_limit = self.budget().application_limit;
        let monetary_limit_minor = self
            .budget()
            .monetary_limit
            .map(|value| value.to_minor_units());

        // An empty tag set means this promotion can target any item, so we can skip tag checks
        // if that is the case.
        let match_all = self.tags().is_empty();

        // Filter and sort eligible items by price descending, index ascending
        let mut eligible: SmallVec<[(usize, i64); 10]> = SmallVec::new();

        for (item_idx, item) in item_group.iter().enumerate() {
            if !match_all && !item.tags().intersects(self.tags()) {
                continue;
            }

            eligible.push((item_idx, item.price().to_minor_units()));
        }

        eligible.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)) // price desc, then index asc
        });

        let num_eligible = eligible.len();
        // Early return if there are insufficient items that are eligible for even
        // a single bundle
        if num_eligible < bundle_size {
            return Ok(Box::new(PositionalDiscountVars {
                promotion_key,
                eligible_items: SmallVec::new(),
                item_participation: SmallVec::new(),
                item_discounts: SmallVec::new(),
                dfa_data: None,
                runtime_discount,
                bundle_size,
                application_limit,
                monetary_limit_minor,
            }));
        }

        // Create participation and discount variables
        let mut item_participation = SmallVec::new();
        let mut item_discounts = SmallVec::new();

        for &(item_idx, _price_minor) in &eligible {
            let item = item_group.get_item(item_idx)?;

            let original_minor = item.price().to_minor_units();

            // Create participation variable
            let participation_var = state.problem_variables_mut().add(variable().binary());
            item_participation.push((item_idx, participation_var));

            // Add objective contribution for participation (full price)
            let Some(full_price_coeff) = i64_to_f64_exact(original_minor) else {
                return Err(SolverError::MinorUnitsNotRepresentable(original_minor));
            };

            state.add_to_objective(participation_var, full_price_coeff);

            observer.on_promotion_variable(
                promotion_key,
                item_idx,
                participation_var,
                original_minor,
                Some("participation"),
            );

            observer.on_objective_term(participation_var, full_price_coeff);

            // Calculate discounted price
            let discounted_minor =
                calculate_discounted_minor_for_runtime(original_minor, runtime_discount)?;

            // Create discount variable
            let discount_var = state.problem_variables_mut().add(variable().binary());
            item_discounts.push((item_idx, discount_var));

            // Subtract discount contribution from the objective
            let discount_amount = original_minor.saturating_sub(discounted_minor);

            let Some(discount_coeff) = i64_to_f64_exact(discount_amount) else {
                return Err(SolverError::MinorUnitsNotRepresentable(discount_amount));
            };

            state.add_to_objective(discount_var, -discount_coeff);

            observer.on_promotion_variable(
                promotion_key,
                item_idx,
                discount_var,
                discounted_minor,
                Some("discount"),
            );

            observer.on_objective_term(discount_var, -discount_coeff);
        }

        // Create DFA state and transition variables (see PositionalDiscountVars::add_dfa_constraints())
        //
        // The DFA needs num_eligible + 1 state positions:
        //   state[i] is the state before processing item i
        //   state[num_eligible] is the final state after all items
        // Transitions at position i connect state[i] to state[i+1].
        let mut state_vars =
            SmallVec::<[SmallVec<[Variable; 8]>; 12]>::with_capacity(num_eligible + 1);

        let mut take_vars = SmallVec::<[SmallVec<[Variable; 8]>; 12]>::with_capacity(num_eligible);

        for pos in 0..num_eligible {
            let mut states_at_pos = SmallVec::<[Variable; 8]>::with_capacity(bundle_size);
            let mut takes_at_pos = SmallVec::<[Variable; 8]>::with_capacity(bundle_size);

            for r in 0..bundle_size {
                let state_var = state.problem_variables_mut().add(variable().binary());
                let take_var = state.problem_variables_mut().add(variable().binary());

                observer.on_auxiliary_variable(
                    promotion_key,
                    state_var,
                    "DFA state",
                    Some(pos),
                    Some(r),
                );
                observer.on_auxiliary_variable(
                    promotion_key,
                    take_var,
                    "DFA take",
                    Some(pos),
                    Some(r),
                );

                states_at_pos.push(state_var);
                takes_at_pos.push(take_var);
            }

            state_vars.push(states_at_pos);
            take_vars.push(takes_at_pos);
        }

        // Final state position (after all items processed)
        let final_states: SmallVec<[Variable; 8]> = (0..bundle_size)
            .map(|r| {
                let state_var = state.problem_variables_mut().add(variable().binary());

                observer.on_auxiliary_variable(
                    promotion_key,
                    state_var,
                    "DFA state",
                    Some(num_eligible),
                    Some(r),
                );

                state_var
            })
            .collect();

        Ok(Box::new(PositionalDiscountVars {
            promotion_key,
            eligible_items: eligible,
            item_participation,
            item_discounts,
            dfa_data: Some(PositionalDFAConstraintData {
                state_vars: {
                    state_vars.push(final_states);
                    state_vars
                },
                take_vars,
                size: self.size(),
                positions: self.positions().iter().copied().collect(),
            }),
            runtime_discount,
            bundle_size,
            application_limit,
            monetary_limit_minor,
        }))
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use good_lp::{Expression, IntoAffineExpression, Solution};
    use rustc_hash::FxHashMap;
    use rusty_money::{Money, iso::GBP};
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{PromotionKey, budget::PromotionBudget},
        solvers::ilp::{
            NoopObserver,
            promotions::test_support::{
                MapSolution, RecordingObserver, assert_relation_holds, item_group_from_prices,
            },
            state::ILPState,
        },
        tags::string::StringTagCollection,
    };

    use super::*;

    #[test]
    fn calculate_discounted_price_handles_discount_types() -> TestResult {
        let original_minor = 100_i64;

        let pct = calculate_discounted_minor_for_runtime(
            original_minor,
            positional_runtime_discount_from_config(&SimpleDiscount::PercentageOff(
                Percentage::from(0.25),
            )),
        )?;

        assert_eq!(pct, 75);

        let override_price = calculate_discounted_minor_for_runtime(
            original_minor,
            positional_runtime_discount_from_config(&SimpleDiscount::AmountOverride(
                Money::from_minor(60, GBP),
            )),
        )?;

        assert_eq!(override_price, 60);

        let amount_off = calculate_discounted_minor_for_runtime(
            original_minor,
            positional_runtime_discount_from_config(&SimpleDiscount::AmountOff(Money::from_minor(
                30, GBP,
            ))),
        )?;

        assert_eq!(amount_off, 70);

        let clamped = calculate_discounted_minor_for_runtime(
            original_minor,
            positional_runtime_discount_from_config(&SimpleDiscount::AmountOff(Money::from_minor(
                200, GBP,
            ))),
        )?;

        assert_eq!(clamped, 0);

        Ok(())
    }

    #[test]
    fn is_applicable_checks_item_group_and_tags() {
        let empty_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["fresh"]),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&empty_group));

        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fresh"]),
        )]);

        let item_group = ItemGroup::new(items, GBP);

        assert!(promo.is_applicable(&item_group));

        let match_all = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        assert!(match_all.is_applicable(&item_group));
    }

    #[test]
    fn add_variables_returns_no_dfa_when_insufficient_items() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            3,
            SmallVec::from_vec(vec![2]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let pb = good_lp::ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        assert!(vars.eligible_items.is_empty());
        assert!(vars.dfa_data.is_none());

        Ok(())
    }

    #[test]
    fn add_item_participation_term_includes_matching_item() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200, 300]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let pb = good_lp::ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        let expr = vars.add_item_participation_term(Expression::default(), 1);
        assert!(expr.linear_coefficients().next().is_some());

        Ok(())
    }

    #[test]
    fn add_dfa_constraints_noop_when_no_dfa_data() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            3,
            SmallVec::from_vec(vec![2]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let pb = good_lp::ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        assert!(vars.eligible_items.is_empty());
        assert!(vars.dfa_data.is_none());

        vars.add_dfa_constraints(promo.key(), &mut state, &mut observer);

        Ok(())
    }

    #[test]
    fn add_dfa_constraints_skips_missing_next_state() {
        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());
        let state_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let take_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let participation_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let discount_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let vars = PositionalDiscountVars {
            promotion_key: PromotionKey::default(),
            eligible_items: SmallVec::from_vec(vec![(0, 100)]),
            item_participation: SmallVec::from_vec(vec![(0, participation_var)]),
            item_discounts: SmallVec::from_vec(vec![(0, discount_var)]),
            dfa_data: Some(PositionalDFAConstraintData {
                size: 1,
                positions: SmallVec::from_vec(vec![0]),
                state_vars: SmallVec::from_vec(vec![SmallVec::from_vec(vec![state_var])]),
                take_vars: SmallVec::from_vec(vec![SmallVec::from_vec(vec![take_var])]),
            }),
            runtime_discount: PositionalRuntimeDiscount::PercentageOff(Percentage::from(0.5)),
            bundle_size: 1,
            application_limit: None,
            monetary_limit_minor: None,
        };

        assert_eq!(vars.eligible_items.len(), 1);
        assert_eq!(vars.item_participation.len(), 1);
        assert_eq!(vars.item_discounts.len(), 1);
        assert_eq!(
            vars.dfa_data.as_ref().map(|data| data.state_vars.len()),
            Some(1)
        );

        let mut observer = NoopObserver;
        vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);
    }

    #[test]
    fn add_dfa_constraints_skips_missing_current_state() {
        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());
        let next_state = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let take_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let participation_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let discount_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let vars = PositionalDiscountVars {
            promotion_key: PromotionKey::default(),
            eligible_items: SmallVec::from_vec(vec![(0, 100)]),
            item_participation: SmallVec::from_vec(vec![(0, participation_var)]),
            item_discounts: SmallVec::from_vec(vec![(0, discount_var)]),
            dfa_data: Some(PositionalDFAConstraintData {
                size: 1,
                positions: SmallVec::from_vec(vec![0]),
                state_vars: SmallVec::from_vec(vec![
                    SmallVec::new(),
                    SmallVec::from_vec(vec![next_state]),
                ]),
                take_vars: SmallVec::from_vec(vec![SmallVec::from_vec(vec![take_var])]),
            }),
            runtime_discount: PositionalRuntimeDiscount::PercentageOff(Percentage::from(0.5)),
            bundle_size: 1,
            application_limit: None,
            monetary_limit_minor: None,
        };

        assert_eq!(
            vars.dfa_data.as_ref().map(|data| data.state_vars.len()),
            Some(2)
        );
        assert!(
            vars.dfa_data
                .as_ref()
                .and_then(|data| data.state_vars.first())
                .is_none_or(smallvec::SmallVec::is_empty)
        );

        let mut observer = NoopObserver;
        vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);
    }

    #[test]
    fn add_dfa_constraints_skips_missing_take_current() {
        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());
        let state_now = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let state_next = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let participation_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let discount_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let vars = PositionalDiscountVars {
            promotion_key: PromotionKey::default(),
            eligible_items: SmallVec::from_vec(vec![(0, 100)]),
            item_participation: SmallVec::from_vec(vec![(0, participation_var)]),
            item_discounts: SmallVec::from_vec(vec![(0, discount_var)]),
            dfa_data: Some(PositionalDFAConstraintData {
                size: 1,
                positions: SmallVec::from_vec(vec![0]),
                state_vars: SmallVec::from_vec(vec![
                    SmallVec::from_vec(vec![state_now]),
                    SmallVec::from_vec(vec![state_next]),
                ]),
                take_vars: SmallVec::from_vec(vec![SmallVec::new()]),
            }),
            runtime_discount: PositionalRuntimeDiscount::PercentageOff(Percentage::from(0.5)),
            bundle_size: 1,
            application_limit: None,
            monetary_limit_minor: None,
        };

        assert_eq!(
            vars.dfa_data.as_ref().map(|data| data.take_vars.len()),
            Some(1)
        );
        assert!(
            vars.dfa_data
                .as_ref()
                .and_then(|data| data.take_vars.first())
                .is_none_or(SmallVec::is_empty)
        );

        let mut observer = NoopObserver;
        vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);
    }

    #[test]
    fn add_dfa_constraints_skips_missing_take_prev() {
        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());
        let state_now = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let state_next = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let take_curr = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let participation_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let discount_var = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let vars = PositionalDiscountVars {
            promotion_key: PromotionKey::default(),
            eligible_items: SmallVec::from_vec(vec![(0, 100)]),
            item_participation: SmallVec::from_vec(vec![(0, participation_var)]),
            item_discounts: SmallVec::from_vec(vec![(0, discount_var)]),
            dfa_data: Some(PositionalDFAConstraintData {
                size: 2,
                positions: SmallVec::from_vec(vec![1]),
                state_vars: SmallVec::from_vec(vec![
                    SmallVec::from_vec(vec![state_now]),
                    SmallVec::from_vec(vec![state_next]),
                ]),
                take_vars: SmallVec::from_vec(vec![SmallVec::from_vec(vec![take_curr])]),
            }),
            runtime_discount: PositionalRuntimeDiscount::PercentageOff(Percentage::from(0.5)),
            bundle_size: 2,
            application_limit: None,
            monetary_limit_minor: None,
        };

        assert_eq!(vars.dfa_data.as_ref().map(|data| data.size), Some(2));
        assert_eq!(
            vars.dfa_data
                .as_ref()
                .and_then(|data| data.take_vars.first())
                .map(SmallVec::len),
            Some(1)
        );

        let mut observer = NoopObserver;
        vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);
    }

    #[test]
    fn add_variables_errors_on_nonrepresentable_price() {
        let huge = 9_007_199_254_740_993_i64;
        let item_group = ItemGroup::new(
            SmallVec::from_vec(vec![Item::new(
                ProductKey::default(),
                Money::from_minor(huge, GBP),
            )]),
            GBP,
        );

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            1,
            SmallVec::from_vec(vec![0]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let pb = good_lp::ProblemVariables::new();
        let cost = Expression::default();
        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let err = promo
            .add_variables(&item_group, &mut state, &mut observer)
            .expect_err("expected non-representable error");

        assert!(matches!(
            err,
            SolverError::MinorUnitsNotRepresentable(v) if v == huge
        ));
    }

    #[test]
    fn add_variables_filters_by_tags() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["fresh"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["frozen"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["fresh"]),
            1,
            SmallVec::from_vec(vec![0]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        assert_eq!(vars.eligible_items.len(), 1);

        Ok(())
    }

    #[test]
    fn add_variables_sorts_eligible_items_and_builds_dfa() -> TestResult {
        let item_group = item_group_from_prices(&[100, 300, 200]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        assert_eq!(
            vars.eligible_items,
            SmallVec::<[(usize, i64); 10]>::from_vec(vec![(1, 300), (2, 200), (0, 100)])
        );
        assert_eq!(vars.item_participation.len(), 3);
        assert_eq!(vars.item_discounts.len(), 3);

        let dfa_data = vars.dfa_data.as_ref().expect("expected DFA data");

        assert_eq!(dfa_data.take_vars.len(), 3);
        assert_eq!(dfa_data.state_vars.len(), 4);

        Ok(())
    }

    #[test]
    fn add_dfa_constraints_smoke_test() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200, 300, 400]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        if let Some(vars) = (vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>() {
            let dfa_data = vars.dfa_data.as_ref().expect("expected DFA data");

            assert_eq!(dfa_data.take_vars.len(), 4);
            assert_eq!(dfa_data.state_vars.len(), 5);

            vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);
        } else {
            panic!("Expected positional discount vars");
        }

        Ok(())
    }

    #[test]
    fn add_dfa_constraints_observer_expressions_match_complete_cycle() {
        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());

        let s00 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let s01 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let s10 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let s11 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let s20 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let s21 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let t00 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let t01 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let t10 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let t11 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let p0 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let p1 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let d0 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());
        let d1 = state
            .problem_variables_mut()
            .add(good_lp::variable().binary());

        let vars = PositionalDiscountVars {
            promotion_key: PromotionKey::default(),
            eligible_items: SmallVec::from_vec(vec![(0, 400), (1, 300)]),
            item_participation: SmallVec::from_vec(vec![(0, p0), (1, p1)]),
            item_discounts: SmallVec::from_vec(vec![(0, d0), (1, d1)]),
            dfa_data: Some(PositionalDFAConstraintData {
                size: 2,
                positions: SmallVec::from_vec(vec![1]),
                state_vars: SmallVec::from_vec(vec![
                    SmallVec::from_vec(vec![s00, s01]),
                    SmallVec::from_vec(vec![s10, s11]),
                    SmallVec::from_vec(vec![s20, s21]),
                ]),
                take_vars: SmallVec::from_vec(vec![
                    SmallVec::from_vec(vec![t00, t01]),
                    SmallVec::from_vec(vec![t10, t11]),
                ]),
            }),
            runtime_discount: PositionalRuntimeDiscount::PercentageOff(Percentage::from(0.5)),
            bundle_size: 2,
            application_limit: None,
            monetary_limit_minor: None,
        };

        let mut observer = RecordingObserver::default();

        vars.add_dfa_constraints(PromotionKey::default(), &mut state, &mut observer);

        let transitions = observer
            .promotion_constraints
            .iter()
            .filter(|c| c.constraint_type == "DFA state transition")
            .count();

        assert_eq!(transitions, 4);

        let solution = MapSolution::with(&[
            (s00, 1.0),
            (s01, 0.0),
            (s10, 0.0),
            (s11, 1.0),
            (s20, 1.0),
            (s21, 0.0),
            (t00, 1.0),
            (t01, 0.0),
            (t10, 0.0),
            (t11, 1.0),
            (p0, 1.0),
            (p1, 1.0),
            (d0, 0.0),
            (d1, 1.0),
        ]);

        for record in &observer.promotion_constraints {
            let lhs = solution.eval(&record.expr);

            assert_relation_holds(lhs, &record.relation, record.rhs);
        }
    }

    #[test]
    fn vars_report_participation_and_discount_flags() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200, 300]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        let mut values = Vec::new();

        for &(idx, var) in &vars.item_participation {
            if idx == 1 {
                values.push((var, 1.0));
            }
        }

        for &(idx, var) in &vars.item_discounts {
            if idx == 1 {
                values.push((var, 1.0));
            }
        }

        let solution = MapSolution::with(&values);

        assert!(vars.is_item_participating(&solution, 1));
        assert!(vars.is_item_discounted(&solution, 1));
        assert!(!vars.is_item_participating(&solution, 0));

        Ok(())
    }

    #[test]
    fn add_variables_reports_negative_discount_objective_term() -> TestResult {
        let item_group = item_group_from_prices(&[100]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            1,
            SmallVec::from_vec(vec![0]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(good_lp::ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        let discount_var = vars
            .item_discounts
            .first()
            .map(|(_idx, var)| *var)
            .ok_or("Expected discount var")?;

        let discount_coeff = observer
            .objective_terms
            .iter()
            .find(|(var, _coeff)| *var == discount_var)
            .map(|(_var, coeff)| *coeff);

        assert_eq!(discount_coeff, Some(-50.0));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_respects_participation_and_discount_flags() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200, 300]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        let mut values = Vec::new();

        for &(idx, var) in &vars.item_participation {
            if idx == 0 || idx == 1 {
                values.push((var, 1.0));
            }
        }

        for &(idx, var) in &vars.item_discounts {
            if idx == 1 {
                values.push((var, 1.0));
            }
        }

        let solution = MapSolution::with(&values);
        let discounts = vars.calculate_item_discounts(&solution, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 100)));
        assert_eq!(discounts.get(&1), Some(&(200, 100)));
        assert!(!discounts.contains_key(&2));

        Ok(())
    }

    #[test]
    fn calculate_item_applications_groups_by_bundle_and_applies_discounts() -> TestResult {
        let item_group = item_group_from_prices(&[400, 300, 200, 100]);

        let promo = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1]),
            SimpleDiscount::PercentageOff(Percentage::from(0.5)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<PositionalDiscountVars>())
            .expect("Expected positional discount vars");

        let mut values = Vec::new();

        for &(_idx, var) in &vars.item_participation {
            values.push((var, 1.0));
        }

        for &(idx, var) in &vars.item_discounts {
            if idx == 1 || idx == 3 {
                values.push((var, 1.0));
            }
        }

        let solution = MapSolution::with(&values);

        let mut next_bundle_id = 0;

        let applications = vars.calculate_item_applications(
            PromotionKey::default(),
            &solution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(applications.len(), 4);

        let mut by_item = FxHashMap::default();

        for app in applications {
            by_item.insert(app.item_idx, (app.bundle_id, app.final_price));
        }

        assert_eq!(by_item.get(&0).map(|(id, _)| *id), Some(0));
        assert_eq!(by_item.get(&1).map(|(id, _)| *id), Some(0));
        assert_eq!(by_item.get(&2).map(|(id, _)| *id), Some(1));
        assert_eq!(by_item.get(&3).map(|(id, _)| *id), Some(1));

        assert_eq!(
            by_item.get(&1).map(|(_, price)| *price),
            Some(Money::from_minor(150, GBP))
        );
        assert_eq!(
            by_item.get(&3).map(|(_, price)| *price),
            Some(Money::from_minor(50, GBP))
        );

        Ok(())
    }
}
