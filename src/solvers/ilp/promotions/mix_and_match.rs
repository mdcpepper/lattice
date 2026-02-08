//! Mix-and-Match Bundle Promotions ILP

#[cfg(test)]
use std::any::Any;

use decimal_percentage::Percentage;
use good_lp::{Expression, Solution, Variable, variable};
use num_traits::ToPrimitive;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    discounts::percent_of_minor,
    items::groups::ItemGroup,
    promotions::{
        PromotionKey,
        applications::PromotionApplication,
        types::{MixAndMatchDiscount, MixAndMatchPromotion},
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
enum MixAndMatchRuntimeDiscount {
    PercentAllItems(Percentage),
    AmountOffEachItem(i64),
    FixedPriceEachItem(i64),
    AmountOffTotal(i64),
    PercentCheapest(Percentage),
    FixedCheapest(i64),
    FixedTotal(i64),
}

/// Solver variables for a mix-and-match promotion.
#[derive(Debug)]
pub struct MixAndMatchVars {
    /// Promotion key for observer metadata.
    promotion_key: PromotionKey,

    /// Per-slot item selection variables.
    slot_vars: Vec<SmallVec<[(usize, Variable); 10]>>,

    /// Optional bundle counter (fixed-arity bundles).
    y_bundle: Option<Variable>,

    /// Optional bundle-formed indicator (variable-arity bundles).
    bundle_formed: Option<Variable>,

    /// Target variables for cheapest-item discounts.
    target_vars: Vec<Option<Variable>>,

    /// Slot bounds (min, max) copied from the promotion.
    slot_bounds: Vec<(usize, Option<usize>)>,

    /// Total bundle size (sum of slot mins).
    bundle_size: usize,

    /// Eligible items sorted by price asc, then index asc (for cheapest targeting).
    sorted_items: SmallVec<[(usize, i64); 10]>,

    /// Runtime discount mode captured during variable creation.
    runtime_discount: MixAndMatchRuntimeDiscount,

    /// Budget: optional max applications.
    application_limit: Option<u32>,

    /// Budget: optional max total discount value in minor units.
    monetary_limit_minor: Option<i64>,
}

impl MixAndMatchVars {
    fn selected_exprs(&self) -> SmallVec<[Expression; 10]> {
        let mut exprs: SmallVec<[Expression; 10]> = SmallVec::with_capacity(self.target_vars.len());

        exprs.resize_with(self.target_vars.len(), Expression::default);

        for slot in &self.slot_vars {
            for &(item_idx, var) in slot {
                if let Some(expr) = exprs.get_mut(item_idx) {
                    *expr += var;
                }
            }
        }

        exprs
    }

    fn add_model_constraints(
        &self,
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        if self.slot_vars.is_empty() || !self.has_bundle_control_vars() {
            return;
        }

        self.add_slot_constraints(promotion_key, state, observer);

        if self.needs_target_constraints() {
            self.add_target_constraints(promotion_key, state, observer);
        }
    }

    fn has_bundle_control_vars(&self) -> bool {
        self.y_bundle.is_some() || self.bundle_formed.is_some()
    }

    fn needs_target_constraints(&self) -> bool {
        self.target_vars.iter().any(Option::is_some)
    }

    fn add_slot_constraints(
        &self,
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        for (slot_idx, slot_vars) in self.slot_vars.iter().enumerate() {
            let slot_sum: Expression = slot_vars.iter().map(|(_, var)| *var).sum();
            let (min, max) = self.slot_bounds.get(slot_idx).copied().unwrap_or((0, None));

            if let Some(y_bundle) = self.y_bundle {
                Self::add_fixed_arity_slot_constraints(
                    promotion_key,
                    state,
                    observer,
                    slot_sum,
                    min,
                    max,
                    y_bundle,
                );
            } else if let Some(bundle_formed) = self.bundle_formed {
                Self::add_variable_arity_slot_constraints(
                    promotion_key,
                    state,
                    observer,
                    slot_sum,
                    min,
                    max,
                    bundle_formed,
                );
            }
        }
    }

    fn add_fixed_arity_slot_constraints(
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
        slot_sum: Expression,
        min: usize,
        max: Option<usize>,
        y_bundle: Variable,
    ) {
        let min_i32 = i32_from_usize(min);
        let min_expr = slot_sum.clone() - min_i32 * y_bundle;

        observer.on_promotion_constraint(promotion_key, "slot min", &min_expr, ">=", 0.0);
        state.add_geq_constraint(min_expr, 0.0);

        if let Some(max) = max {
            let max_i32 = i32_from_usize(max);
            let max_expr = slot_sum - max_i32 * y_bundle;

            observer.on_promotion_constraint(promotion_key, "slot max", &max_expr, "<=", 0.0);
            state.add_leq_constraint(max_expr, 0.0);
        }
    }

    fn add_variable_arity_slot_constraints(
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
        slot_sum: Expression,
        min: usize,
        max: Option<usize>,
        bundle_formed: Variable,
    ) {
        let min_i32 = i32_from_usize(min);
        let min_expr = slot_sum.clone() - min_i32 * bundle_formed;

        observer.on_promotion_constraint(promotion_key, "slot min (formed)", &min_expr, ">=", 0.0);
        state.add_geq_constraint(min_expr, 0.0);

        if let Some(max) = max {
            let max_i32 = i32_from_usize(max);
            let max_expr = slot_sum.clone() - max_i32 * bundle_formed;

            observer.on_promotion_constraint(
                promotion_key,
                "slot max (formed)",
                &max_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(max_expr, 0.0);
        }

        // If slot has enough items, bundle can be formed.
        let formed_expr = Expression::from(bundle_formed) - slot_sum / min_i32;

        observer.on_promotion_constraint(promotion_key, "bundle formed", &formed_expr, "<=", 0.0);

        state.add_leq_constraint(formed_expr, 0.0);
    }

    fn add_target_constraints(
        &self,
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        let selected_exprs = self.selected_exprs();
        let mut target_sum = Expression::default();

        for &(item_idx, _price) in &self.sorted_items {
            let Some(target_var) = self.target_vars.get(item_idx).and_then(|v| *v) else {
                continue;
            };

            let selected_expr = selected_exprs.get(item_idx).cloned().unwrap_or_default();

            let expr = Expression::from(target_var) - selected_expr.clone();

            observer.on_promotion_constraint(
                promotion_key,
                "target implies selected",
                &expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(Expression::from(target_var) - selected_expr, 0.0);

            target_sum += target_var;
        }

        if let Some(y_bundle) = self.y_bundle {
            self.add_fixed_arity_target_constraints(
                promotion_key,
                state,
                observer,
                &selected_exprs,
                target_sum,
                y_bundle,
            );
        } else if let Some(bundle_formed) = self.bundle_formed {
            Self::add_variable_arity_target_constraints(
                promotion_key,
                state,
                observer,
                target_sum,
                bundle_formed,
            );
        }
    }

    fn add_fixed_arity_target_constraints(
        &self,
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
        selected_exprs: &[Expression],
        target_sum: Expression,
        y_bundle: Variable,
    ) {
        let mut prefix_selected = Expression::default();
        let mut prefix_targets = Expression::default();

        let k_total = i32_from_usize(self.bundle_size);

        if k_total > 0 {
            for &(item_idx, _price) in &self.sorted_items {
                let selected_expr = selected_exprs.get(item_idx).cloned().unwrap_or_default();

                prefix_selected += selected_expr;

                if let Some(target_var) = self.target_vars.get(item_idx).and_then(|v| *v) {
                    prefix_targets += target_var;
                }

                let expr =
                    prefix_targets.clone() - (prefix_selected.clone() - (k_total - 1) * y_bundle);

                observer.on_promotion_constraint(
                    promotion_key,
                    "cheapest prefix",
                    &expr,
                    ">=",
                    0.0,
                );

                state.add_geq_constraint(
                    prefix_targets.clone() - (prefix_selected.clone() - (k_total - 1) * y_bundle),
                    0.0,
                );
            }
        }

        let target_count_expr = target_sum - y_bundle;

        observer.on_promotion_constraint(
            promotion_key,
            "target count",
            &target_count_expr,
            "=",
            0.0,
        );

        state.add_eq_constraint(target_count_expr, 0.0);
    }

    fn add_variable_arity_target_constraints(
        promotion_key: PromotionKey,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
        target_sum: Expression,
        bundle_formed: Variable,
    ) {
        let expr = target_sum - bundle_formed;

        observer.on_promotion_constraint(promotion_key, "target count (formed)", &expr, "=", 0.0);

        state.add_eq_constraint(expr, 0.0);
    }

    /// Add budget constraints for mix-and-match promotions
    pub fn add_budget_constraints(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        // Application limit: For mix-and-match, this limits bundles
        if let Some(application_limit) = self.application_limit {
            // Use bundle counter variable if available
            if let Some(y_bundle) = self.y_bundle {
                let limit_f64 = i64_to_f64_exact(i64::from(application_limit)).ok_or(
                    SolverError::MinorUnitsNotRepresentable(i64::from(application_limit)),
                )?;

                let expr = Expression::from(y_bundle);

                observer.on_promotion_constraint(
                    self.promotion_key,
                    "application count budget (bundle limit)",
                    &expr,
                    "<=",
                    limit_f64,
                );

                state.add_leq_constraint(expr, limit_f64);
            } else if let Some(bundle_formed) = self.bundle_formed {
                // Variable-arity bundles: bundle_formed is binary (0 or 1)
                // application_limit only makes sense if >= 1
                if application_limit == 0 {
                    let expr = Expression::from(bundle_formed);

                    observer.on_promotion_constraint(
                        self.promotion_key,
                        "application count budget (no bundles)",
                        &expr,
                        "=",
                        0.0,
                    );

                    state.add_eq_constraint(expr, 0.0);
                }
                // If application_limit >= 1, no constraint needed (bundle_formed is already <= 1)
            }
        }

        // Monetary limit: sum(discount_amount * participation_var) <= limit
        if let Some(limit_minor) = self.monetary_limit_minor {
            let mut discount_expr = Expression::default();

            match self.runtime_discount {
                MixAndMatchRuntimeDiscount::PercentCheapest(_)
                | MixAndMatchRuntimeDiscount::FixedCheapest(_) => {
                    // Cheapest-item modes are exact with target vars: only targets consume budget.
                    for (item_idx, target_var) in self.target_vars.iter().enumerate() {
                        let Some(target_var) = target_var else {
                            continue;
                        };

                        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
                        let full_minor = item.price().to_minor_units();
                        let discounted_minor = calculate_discounted_minor_for_budget(
                            full_minor,
                            self.runtime_discount,
                        )?;

                        let discount_amount = full_minor.saturating_sub(discounted_minor);
                        let coeff = i64_to_f64_exact(discount_amount)
                            .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                        discount_expr += *target_var * coeff;
                    }
                }
                _ => {
                    // Iterate over all slot variables to compute total discount.
                    // For bundle-total discounts this remains a conservative estimate.
                    for slot in &self.slot_vars {
                        for &(item_idx, var) in slot {
                            let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
                            let full_minor = item.price().to_minor_units();
                            let discounted_minor = calculate_discounted_minor_for_budget(
                                full_minor,
                                self.runtime_discount,
                            )?;

                            let discount_amount = full_minor.saturating_sub(discounted_minor);
                            let coeff = i64_to_f64_exact(discount_amount)
                                .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                            discount_expr += var * coeff;
                        }
                    }
                }
            }

            let limit_f64 = i64_to_f64_exact(limit_minor)
                .ok_or(SolverError::MinorUnitsNotRepresentable(limit_minor))?;

            observer.on_promotion_constraint(
                self.promotion_key,
                "monetary value budget",
                &discount_expr,
                "<=",
                limit_f64,
            );

            state.add_leq_constraint(discount_expr, limit_f64);
        }

        Ok(())
    }

    fn bundle_count(&self, solution: &dyn Solution) -> usize {
        if let Some(y_bundle) = self.y_bundle {
            let count = solution.value(y_bundle).round();
            let count = count.to_i64().unwrap_or(0).max(0);

            usize::try_from(count).unwrap_or(0)
        } else if let Some(bundle_formed) = self.bundle_formed {
            usize::from(solution.value(bundle_formed) > BINARY_THRESHOLD)
        } else {
            0
        }
    }
}

impl ILPPromotionVars for MixAndMatchVars {
    fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for slot in &self.slot_vars {
            for &(idx, var) in slot {
                if idx == item_idx {
                    updated_expr += var;
                }
            }
        }

        updated_expr
    }

    fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.slot_vars.iter().any(|slot| {
            slot.iter()
                .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
        })
    }

    fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        if let Some(var) = self.target_vars.get(item_idx).and_then(|v| *v) {
            return solution.value(var) > BINARY_THRESHOLD;
        }

        self.is_item_participating(solution, item_idx)
    }

    fn add_constraints(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_model_constraints(promotion_key, state, observer);
        self.add_budget_constraints(item_group, state, observer)
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        calculate_discounts_for_vars(solution, self, item_group)
    }

    fn calculate_item_applications<'b>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        let bundles = build_bundles(solution, self);

        if bundles.is_empty() {
            return Ok(SmallVec::new());
        }

        let discounts = calculate_discounts_for_vars(solution, self, item_group)?;
        let currency = item_group.currency();

        let mut applications = SmallVec::new();

        for bundle in bundles {
            let bundle_id = *next_bundle_id;
            *next_bundle_id += 1;

            for item_idx in bundle {
                let item = item_group.get_item(item_idx)?;
                let original_minor = item.price().to_minor_units();

                let final_minor = discounts
                    .get(&item_idx)
                    .map_or(original_minor, |(_, final_minor)| *final_minor);

                applications.push(PromotionApplication {
                    promotion_key,
                    item_idx,
                    bundle_id,
                    original_price: *item.price(),
                    final_price: Money::from_minor(final_minor, currency),
                });
            }
        }

        Ok(applications)
    }
}

fn discounted_minor_percent(pct: &Percentage, original_minor: i64) -> Result<i64, SolverError> {
    let discount_minor = percent_of_minor(pct, original_minor).map_err(SolverError::Discount)?;

    Ok(original_minor.saturating_sub(discount_minor))
}

fn i32_from_usize(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn runtime_discount_from_config(discount: &MixAndMatchDiscount<'_>) -> MixAndMatchRuntimeDiscount {
    match discount {
        MixAndMatchDiscount::PercentAllItems(pct) => {
            MixAndMatchRuntimeDiscount::PercentAllItems(*pct)
        }
        MixAndMatchDiscount::AmountOffEachItem(amount) => {
            MixAndMatchRuntimeDiscount::AmountOffEachItem(amount.to_minor_units())
        }
        MixAndMatchDiscount::FixedPriceEachItem(amount) => {
            MixAndMatchRuntimeDiscount::FixedPriceEachItem(amount.to_minor_units())
        }
        MixAndMatchDiscount::AmountOffTotal(amount) => {
            MixAndMatchRuntimeDiscount::AmountOffTotal(amount.to_minor_units())
        }
        MixAndMatchDiscount::PercentCheapest(pct) => {
            MixAndMatchRuntimeDiscount::PercentCheapest(*pct)
        }
        MixAndMatchDiscount::FixedCheapest(amount) => {
            MixAndMatchRuntimeDiscount::FixedCheapest(amount.to_minor_units())
        }
        MixAndMatchDiscount::FixedTotal(amount) => {
            MixAndMatchRuntimeDiscount::FixedTotal(amount.to_minor_units())
        }
    }
}

fn calculate_discounted_minor_for_budget(
    full_minor: i64,
    discount: MixAndMatchRuntimeDiscount,
) -> Result<i64, SolverError> {
    let discounted_minor = match discount {
        MixAndMatchRuntimeDiscount::PercentAllItems(pct)
        | MixAndMatchRuntimeDiscount::PercentCheapest(pct) => {
            let discount_amount =
                percent_of_minor(&pct, full_minor).map_err(SolverError::Discount)?;

            full_minor.saturating_sub(discount_amount)
        }
        MixAndMatchRuntimeDiscount::AmountOffEachItem(amount_off) => {
            full_minor.saturating_sub(amount_off)
        }
        // Conservative approximation for budgeting bundle-total discounts:
        // assume each selected item could be fully discounted.
        MixAndMatchRuntimeDiscount::AmountOffTotal(_)
        | MixAndMatchRuntimeDiscount::FixedTotal(_) => 0,
        MixAndMatchRuntimeDiscount::FixedPriceEachItem(fixed_minor)
        | MixAndMatchRuntimeDiscount::FixedCheapest(fixed_minor) => fixed_minor,
    };

    Ok(discounted_minor.max(0))
}

fn proportional_alloc(total: i64, part: i64, denom: i64) -> i64 {
    if denom == 0 {
        return 0;
    }

    let total = i128::from(total);
    let part = i128::from(part);
    let denom = i128::from(denom);

    let numerator = total * part + denom / 2;
    let value = numerator / denom;

    i64::try_from(value).unwrap_or(0)
}

fn build_bundles(solution: &dyn Solution, vars: &MixAndMatchVars) -> Vec<Vec<usize>> {
    let bundles_applied = vars.bundle_count(solution);

    if bundles_applied == 0 {
        return Vec::new();
    }

    // Collect selected items per slot
    let mut slot_items: Vec<Vec<usize>> = Vec::with_capacity(vars.slot_vars.len());

    for slot_vars in &vars.slot_vars {
        let mut items = Vec::new();

        for &(item_idx, var) in slot_vars {
            if solution.value(var) > BINARY_THRESHOLD {
                items.push(item_idx);
            }
        }

        slot_items.push(items);
    }

    let mut bundles = Vec::new();

    if vars.y_bundle.is_some() {
        for bundle_idx in 0..bundles_applied {
            let mut bundle_items = Vec::new();

            for (slot_idx, (min, _max)) in vars.slot_bounds.iter().copied().enumerate() {
                let items: &[usize] = slot_items.get(slot_idx).map_or(&[], |v| v.as_slice());
                let start = bundle_idx * min;

                if start >= items.len() {
                    continue;
                }

                let end = (bundle_idx + 1) * min;

                if let Some(slice) = items.get(start..items.len().min(end)) {
                    bundle_items.extend_from_slice(slice);
                }
            }

            if !bundle_items.is_empty() {
                bundles.push(bundle_items);
            }
        }
    } else {
        let mut bundle_items = Vec::new();

        for items in slot_items {
            bundle_items.extend(items);
        }

        if !bundle_items.is_empty() {
            bundles.push(bundle_items);
        }
    }

    bundles
}

#[expect(clippy::too_many_lines, reason = "Complex discount calculation logic")]
fn calculate_discounts_for_vars(
    solution: &dyn Solution,
    vars: &MixAndMatchVars,
    item_group: &ItemGroup<'_>,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = FxHashMap::default();

    match vars.runtime_discount {
        MixAndMatchRuntimeDiscount::PercentAllItems(pct) => {
            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();
                let discounted_minor = discounted_minor_percent(&pct, original_minor)?;

                discounts.insert(item_idx, (original_minor, discounted_minor));
            }
        }
        MixAndMatchRuntimeDiscount::AmountOffEachItem(amount_off) => {
            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();
                let discounted_minor = original_minor.saturating_sub(amount_off).max(0);

                discounts.insert(item_idx, (original_minor, discounted_minor));
            }
        }
        MixAndMatchRuntimeDiscount::FixedPriceEachItem(fixed_minor) => {
            let fixed_minor = fixed_minor.max(0);

            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                discounts.insert(item_idx, (item.price().to_minor_units(), fixed_minor));
            }
        }
        MixAndMatchRuntimeDiscount::PercentCheapest(pct) => {
            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();

                discounts.insert(item_idx, (original_minor, original_minor));

                if vars.is_item_priced_by_promotion(solution, item_idx) {
                    let discounted_minor = discounted_minor_percent(&pct, original_minor)?;

                    discounts.insert(item_idx, (original_minor, discounted_minor));
                }
            }
        }
        MixAndMatchRuntimeDiscount::FixedCheapest(fixed_minor) => {
            let fixed_minor = fixed_minor.max(0);

            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();

                let final_minor = if vars.is_item_priced_by_promotion(solution, item_idx) {
                    fixed_minor
                } else {
                    original_minor
                };

                discounts.insert(item_idx, (original_minor, final_minor));
            }
        }
        MixAndMatchRuntimeDiscount::AmountOffTotal(amount_off) => {
            let bundles = build_bundles(solution, vars);

            for bundle_items in bundles {
                if bundle_items.is_empty() {
                    continue;
                }

                let mut original_total = 0_i64;

                for &item_idx in &bundle_items {
                    let item = item_group.get_item(item_idx)?;

                    original_total += item.price().to_minor_units();
                }

                let bundle_total = original_total.saturating_sub(amount_off).max(0);
                let mut remaining = bundle_total;

                for (i, &item_idx) in bundle_items.iter().enumerate() {
                    let item = item_group.get_item(item_idx)?;
                    let original_minor = item.price().to_minor_units();

                    let final_minor = if i == bundle_items.len() - 1 {
                        remaining
                    } else if original_total == 0 {
                        0
                    } else {
                        proportional_alloc(bundle_total, original_minor, original_total)
                    };

                    remaining -= final_minor;

                    discounts.insert(item_idx, (original_minor, final_minor));
                }
            }
        }
        MixAndMatchRuntimeDiscount::FixedTotal(bundle_price) => {
            let bundles = build_bundles(solution, vars);

            for bundle_items in bundles {
                if bundle_items.is_empty() {
                    continue;
                }

                let mut original_total = 0_i64;

                for &item_idx in &bundle_items {
                    let item = item_group.get_item(item_idx)?;

                    original_total += item.price().to_minor_units();
                }

                let mut remaining = bundle_price;

                for (i, &item_idx) in bundle_items.iter().enumerate() {
                    let item = item_group.get_item(item_idx)?;
                    let original_minor = item.price().to_minor_units();

                    let final_minor = if i == bundle_items.len() - 1 {
                        remaining
                    } else if original_total == 0 {
                        0
                    } else {
                        proportional_alloc(bundle_price, original_minor, original_total)
                    };

                    remaining -= final_minor;

                    discounts.insert(item_idx, (original_minor, final_minor));
                }
            }
        }
    }

    Ok(discounts)
}

impl ILPPromotion for MixAndMatchPromotion<'_> {
    fn key(&self) -> PromotionKey {
        MixAndMatchPromotion::key(self)
    }

    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        if item_group.is_empty() {
            return false;
        }

        for slot in self.slots() {
            let matching_items = item_group
                .iter()
                .filter(|item| item.tags().intersects(slot.tags()))
                .count();

            if matching_items < slot.min() {
                return false;
            }
        }

        true
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Complexity due to multiple discount types"
    )]
    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError> {
        let promotion_key = self.key();
        let runtime_discount = runtime_discount_from_config(self.discount());
        let application_limit = self.budget().application_limit;

        let monetary_limit_minor = self
            .budget()
            .monetary_limit
            .map(|value| value.to_minor_units());

        if item_group.is_empty() {
            return Ok(Box::new(MixAndMatchVars {
                promotion_key,
                slot_vars: Vec::new(),
                y_bundle: None,
                bundle_formed: None,
                target_vars: Vec::new(),
                slot_bounds: Vec::new(),
                bundle_size: 0,
                sorted_items: SmallVec::new(),
                runtime_discount,
                application_limit,
                monetary_limit_minor,
            }));
        }

        // Collect eligible items per slot.
        let mut eligible_per_slot: Vec<SmallVec<[(usize, i64); 10]>> =
            Vec::with_capacity(self.slots().len());
        let mut slot_bounds = Vec::with_capacity(self.slots().len());
        let mut feasible = true;

        for slot in self.slots() {
            let mut eligible = SmallVec::new();

            for (item_idx, item) in item_group.iter().enumerate() {
                if item.tags().intersects(slot.tags()) {
                    eligible.push((item_idx, item.price().to_minor_units()));
                }
            }

            if eligible.len() < slot.min() {
                feasible = false;
            }

            slot_bounds.push((slot.min(), slot.max()));
            eligible_per_slot.push(eligible);
        }

        if !feasible {
            return Ok(Box::new(MixAndMatchVars {
                promotion_key,
                slot_vars: Vec::new(),
                y_bundle: None,
                bundle_formed: None,
                target_vars: Vec::new(),
                slot_bounds: Vec::new(),
                bundle_size: 0,
                sorted_items: SmallVec::new(),
                runtime_discount,
                application_limit,
                monetary_limit_minor,
            }));
        }

        // Determine whether we can use a bundle counter.
        let can_use_bundle_counter = self.has_fixed_arity();

        let max_bundles = eligible_per_slot
            .iter()
            .zip(self.slots())
            .map(|(slot_items, slot)| slot_items.len() / slot.min())
            .min()
            .unwrap_or(0);

        let (y_bundle, bundle_formed) = if can_use_bundle_counter {
            let max_bundles_i32 = i32_from_usize(max_bundles);

            let var = state
                .problem_variables_mut()
                .add(variable().integer().min(0).max(max_bundles_i32));

            observer.on_auxiliary_variable(promotion_key, var, "bundle count", None, None);

            (Some(var), None)
        } else {
            let var = state.problem_variables_mut().add(variable().binary());

            observer.on_auxiliary_variable(promotion_key, var, "bundle formed", None, None);

            (None, Some(var))
        };

        let bundle_size = self.bundle_size();

        // Build per-slot variables and collect all eligible items for target vars.
        let mut slot_vars: Vec<SmallVec<[(usize, Variable); 10]>> =
            Vec::with_capacity(eligible_per_slot.len());
        let mut all_bundle_items: Vec<(usize, i64)> = Vec::new();
        let mut seen_items: FxHashSet<usize> = FxHashSet::default();

        for slot_items in &eligible_per_slot {
            let mut vars = SmallVec::new();

            for &(item_idx, price_minor) in slot_items {
                let var = state.problem_variables_mut().add(variable().binary());

                vars.push((item_idx, var));

                if seen_items.insert(item_idx) {
                    all_bundle_items.push((item_idx, price_minor));
                }

                let coeff_minor = match self.discount() {
                    MixAndMatchDiscount::PercentAllItems(pct) => {
                        discounted_minor_percent(pct, price_minor)?
                    }
                    MixAndMatchDiscount::AmountOffEachItem(amount) => {
                        price_minor.saturating_sub(amount.to_minor_units()).max(0)
                    }
                    MixAndMatchDiscount::FixedPriceEachItem(amount) => {
                        amount.to_minor_units().max(0)
                    }
                    MixAndMatchDiscount::FixedTotal(_) => 0,
                    MixAndMatchDiscount::AmountOffTotal(_)
                    | MixAndMatchDiscount::PercentCheapest(_)
                    | MixAndMatchDiscount::FixedCheapest(_) => price_minor,
                };

                if coeff_minor != 0 {
                    let coeff = i64_to_f64_exact(coeff_minor)
                        .ok_or(SolverError::MinorUnitsNotRepresentable(coeff_minor))?;

                    state.add_to_objective(var, coeff);
                    observer.on_objective_term(var, coeff);
                }

                observer.on_promotion_variable(
                    promotion_key,
                    item_idx,
                    var,
                    coeff_minor,
                    Some("slot"),
                );
            }

            slot_vars.push(vars);
        }

        // Target variables (cheapest item)
        let needs_target = matches!(
            self.discount(),
            MixAndMatchDiscount::PercentCheapest(_) | MixAndMatchDiscount::FixedCheapest(_)
        );

        let mut target_vars = vec![None; item_group.len()];
        let mut sorted_items: SmallVec<[(usize, i64); 10]> = SmallVec::new();

        if needs_target {
            // Sort items by price ascending (cheapest first).
            all_bundle_items.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            sorted_items.extend(all_bundle_items.iter().copied());

            for &(item_idx, price_minor) in &all_bundle_items {
                let var = state.problem_variables_mut().add(variable().binary());

                if let Some(slot) = target_vars.get_mut(item_idx) {
                    *slot = Some(var);
                }

                let (discount_amount, discounted_minor) = match self.discount() {
                    MixAndMatchDiscount::PercentCheapest(pct) => {
                        let discounted = discounted_minor_percent(pct, price_minor)?;
                        let discount_amount = price_minor.saturating_sub(discounted);

                        (discount_amount, discounted)
                    }
                    MixAndMatchDiscount::FixedCheapest(amount) => {
                        let fixed_minor = amount.to_minor_units().max(0);
                        let discount_amount = price_minor - fixed_minor;

                        (discount_amount, fixed_minor)
                    }
                    _ => (0, price_minor),
                };

                if discount_amount != 0 {
                    let coeff = i64_to_f64_exact(discount_amount)
                        .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                    state.add_to_objective(var, -coeff);
                    observer.on_objective_term(var, -coeff);
                }

                observer.on_promotion_variable(
                    promotion_key,
                    item_idx,
                    var,
                    discounted_minor,
                    Some("target"),
                );
            }
        }

        // Fixed total price objective term
        if let MixAndMatchDiscount::FixedTotal(amount) = self.discount() {
            let bundle_price = amount.to_minor_units();
            let coeff = i64_to_f64_exact(bundle_price)
                .ok_or(SolverError::MinorUnitsNotRepresentable(bundle_price))?;

            if let Some(y_bundle) = y_bundle {
                state.add_to_objective(y_bundle, coeff);
                observer.on_objective_term(y_bundle, coeff);
            } else if let Some(bundle_formed) = bundle_formed {
                state.add_to_objective(bundle_formed, coeff);
                observer.on_objective_term(bundle_formed, coeff);
            }
        }

        // Amount off total objective term (negative per bundle formed)
        if let MixAndMatchDiscount::AmountOffTotal(amount) = self.discount() {
            let amount_off = amount.to_minor_units();
            let coeff = i64_to_f64_exact(amount_off)
                .ok_or(SolverError::MinorUnitsNotRepresentable(amount_off))?;

            if let Some(y_bundle) = y_bundle {
                state.add_to_objective(y_bundle, -coeff);
                observer.on_objective_term(y_bundle, -coeff);
            } else if let Some(bundle_formed) = bundle_formed {
                state.add_to_objective(bundle_formed, -coeff);
                observer.on_objective_term(bundle_formed, -coeff);
            }
        }

        Ok(Box::new(MixAndMatchVars {
            promotion_key,
            slot_vars,
            y_bundle,
            bundle_formed,
            target_vars,
            slot_bounds,
            bundle_size,
            sorted_items,
            runtime_discount,
            application_limit,
            monetary_limit_minor,
        }))
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use good_lp::{ProblemVariables, Solution, SolverModel, Variable, variable};
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;
    use smallvec::{SmallVec, smallvec};
    use testresult::TestResult;

    #[cfg(feature = "solver-highs")]
    use good_lp::solvers::highs::highs as test_solver;
    #[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
    use good_lp::solvers::microlp::microlp as test_solver;

    use crate::{
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{PromotionKey, PromotionSlotKey, budget::PromotionBudget},
        solvers::ilp::{
            NoopObserver,
            promotions::test_support::{
                MapSolution, RecordingObserver, assert_relation_holds,
                assert_state_constraints_hold, item_group_from_prices,
                observed_lhs_values_for_type, state_lhs_values_for_relation,
            },
            state::{ConstraintRelation, ILPState},
        },
        tags::string::StringTagCollection,
        utils::slot,
    };

    use super::*;

    #[test]
    fn is_applicable_checks_slots() {
        let item_group = item_group_from_prices(&[100]);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.1)),
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn add_constraints_smoke_test() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(120, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_fixed_total_allocates() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
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
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(150, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Select both items into the slots and form one bundle.
        let mut values = Vec::new();

        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 1.0));
        }

        let solution = MapSolution::with(&values);
        let discounts = vars.calculate_item_discounts(&solution, &item_group)?;

        assert_eq!(discounts.len(), 2);
        assert_eq!(discounts.get(&0), Some(&(200, 100)));
        assert_eq!(discounts.get(&1), Some(&(100, 50)));

        let total_final: i64 = discounts
            .values()
            .map(|(_, final_minor)| *final_minor)
            .sum();

        assert_eq!(total_final, 150);

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_percent_all_items() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(400, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 1.0));
        }

        let solution = MapSolution::with(&values);
        let discounts = vars.calculate_item_discounts(&solution, &item_group)?;

        assert_eq!(discounts.len(), 2);

        // Main: 400 * 0.75 = 300
        assert_eq!(discounts.get(&0).map(|(_, d)| *d), Some(300));

        // Drink: 200 * 0.75 = 150
        assert_eq!(discounts.get(&1).map(|(_, d)| *d), Some(150));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_percent_cheapest() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(400, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;
        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 1.0));
        }

        // Mark the cheapest item's target variable as selected
        if let Some(target_var) = vars.target_vars.get(1).and_then(|v| *v) {
            values.push((target_var, 1.0));
        }

        let solution = MapSolution::with(&values);
        let discounts = vars.calculate_item_discounts(&solution, &item_group)?;

        assert_eq!(discounts.len(), 2);

        // Main: 400 (no discount)
        assert_eq!(discounts.get(&0).map(|(_, d)| *d), Some(400));

        // Drink: 200 * 0.50 = 100 (cheapest gets discount)
        assert_eq!(discounts.get(&1).map(|(_, d)| *d), Some(100));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_fixed_cheapest() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(400, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedCheapest(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 1.0));
        }

        // Mark the cheapest item's target variable as selected
        if let Some(target_var) = vars.target_vars.get(1).and_then(|v| *v) {
            values.push((target_var, 1.0));
        }

        let solution = MapSolution::with(&values);
        let discounts = vars.calculate_item_discounts(&solution, &item_group)?;

        assert_eq!(discounts.len(), 2);

        // Main: 400 (no discount)
        assert_eq!(discounts.get(&0).map(|(_, d)| *d), Some(400));

        // Drink: 50 (fixed price)
        assert_eq!(discounts.get(&1).map(|(_, d)| *d), Some(50));

        Ok(())
    }

    #[test]
    fn variable_arity_bundle_formed() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        // Variable arity: min=2, max=None
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            2,
            None, // No max - variable arity
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        assert!(!promo.has_fixed_arity());

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Should use bundle_formed, not y_bundle
        assert!(vars.bundle_formed.is_some());
        assert!(vars.y_bundle.is_none());

        Ok(())
    }

    #[test]
    fn calculate_item_applications_returns_bundle_ids() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
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
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(150, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 1.0));
        }

        let solution = MapSolution::with(&values);
        let mut next_bundle_id = 0;

        let applications = vars.calculate_item_applications(
            promo.key(),
            &solution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(applications.len(), 2);
        assert_eq!(next_bundle_id, 1); // One bundle created

        // All items should have the same bundle ID
        assert_eq!(applications[0].bundle_id, applications[1].bundle_id);

        Ok(())
    }

    #[test]
    fn multiple_bundles_increment_bundle_ids() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(120, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        // Select all items
        for slot in &vars.slot_vars {
            for &(_idx, var) in slot {
                values.push((var, 1.0));
            }
        }

        // Set bundle count to 2
        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 2.0));
        }

        let solution = MapSolution::with(&values);
        let mut next_bundle_id = 0;

        let applications = vars.calculate_item_applications(
            promo.key(),
            &solution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert_eq!(applications.len(), 4);
        assert_eq!(next_bundle_id, 2); // Two bundles created

        Ok(())
    }

    #[test]
    fn empty_item_group_returns_empty_vars() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::new();
        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        assert!(vars.slot_vars.is_empty());
        assert!(vars.y_bundle.is_none());

        Ok(())
    }

    #[test]
    fn insufficient_items_returns_empty_vars() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["main"]),
        )]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Should return empty vars when not feasible
        assert!(vars.slot_vars.is_empty());

        Ok(())
    }

    #[test]
    fn add_constraints_with_variable_arity_and_max() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        // Variable arity with max: min=1, max=2
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(2),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Should use bundle_formed for variable arity
        assert!(vars.bundle_formed.is_some());

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        Ok(())
    }

    #[test]
    fn calculate_item_applications_with_no_bundles() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["main"]),
        )]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Don't select any items
        let solution = MapSolution::default();
        let mut next_bundle_id = 0;

        let applications = vars.calculate_item_applications(
            promo.key(),
            &solution,
            &item_group,
            &mut next_bundle_id,
        )?;

        assert!(applications.is_empty());
        assert_eq!(next_bundle_id, 0);

        Ok(())
    }

    #[test]
    fn is_item_priced_by_promotion_without_target() {
        let vars = MixAndMatchVars {
            promotion_key: PromotionKey::default(),
            slot_vars: vec![SmallVec::new()],
            y_bundle: None,
            bundle_formed: None,
            target_vars: Vec::new(),
            slot_bounds: Vec::new(),
            bundle_size: 0,
            sorted_items: SmallVec::new(),
            runtime_discount: MixAndMatchRuntimeDiscount::PercentAllItems(Percentage::from(0.0)),
            application_limit: None,
            monetary_limit_minor: None,
        };

        let solution = MapSolution::default();
        assert!(!vars.is_item_priced_by_promotion(&solution, 0));
    }

    #[test]
    fn add_constraints_returns_early_without_bundle_control_vars() {
        let mut pb = ProblemVariables::new();
        let slot_var = pb.add(variable().binary());
        let target_var = pb.add(variable().binary());

        let vars = MixAndMatchVars {
            promotion_key: PromotionKey::default(),
            slot_vars: vec![smallvec![(0, slot_var)]],
            y_bundle: None,
            bundle_formed: None,
            target_vars: vec![Some(target_var)],
            slot_bounds: vec![(1, Some(1))],
            bundle_size: 1,
            sorted_items: smallvec![(0, 100)],
            runtime_discount: MixAndMatchRuntimeDiscount::PercentCheapest(Percentage::from(0.5)),
            application_limit: None,
            monetary_limit_minor: None,
        };

        let mut state = ILPState::new(pb, Expression::default());
        let mut observer = RecordingObserver::default();

        vars.add_model_constraints(PromotionKey::default(), &mut state, &mut observer);

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert!(constraints.is_empty());
        assert!(observer.promotion_constraints.is_empty());
    }

    #[test]
    fn add_constraints_fixed_arity_has_expected_constraint_lhs_values() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(150, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            Some(2),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        let slot0_var = vars.slot_vars[0][0].1;
        let slot1_var = vars.slot_vars[0][1].1;
        let slot2_var = vars.slot_vars[0][2].1;
        let y_bundle = vars.y_bundle.ok_or("Expected y_bundle")?;
        let target0 = vars.target_vars[0].ok_or("Expected target var for item 0")?;
        let target1 = vars.target_vars[1].ok_or("Expected target var for item 1")?;
        let target2 = vars.target_vars[2].ok_or("Expected target var for item 2")?;

        let solution = MapSolution::with(&[
            (slot0_var, 1.0),
            (slot1_var, 1.0),
            (slot2_var, 0.0),
            (y_bundle, 1.0),
            (target0, 1.0),
            (target1, 0.0),
            (target2, 0.0),
        ]);

        assert_eq!(
            observed_lhs_values_for_type(&observer, "slot min", &solution),
            vec![0.0]
        );
        assert_eq!(
            observed_lhs_values_for_type(&observer, "slot max", &solution),
            vec![0.0]
        );
        assert_eq!(
            observed_lhs_values_for_type(&observer, "cheapest prefix", &solution),
            vec![1.0, 0.0, 0.0]
        );
        assert_eq!(
            observed_lhs_values_for_type(&observer, "target count", &solution),
            vec![0.0]
        );

        let target_implies =
            observed_lhs_values_for_type(&observer, "target implies selected", &solution);

        assert_eq!(target_implies, vec![0.0, -1.0, 0.0]);

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert_eq!(
            state_lhs_values_for_relation(&constraints, ConstraintRelation::Geq, &solution),
            vec![0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            state_lhs_values_for_relation(&constraints, ConstraintRelation::Leq, &solution),
            vec![-1.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            state_lhs_values_for_relation(&constraints, ConstraintRelation::Eq, &solution),
            vec![0.0]
        );

        Ok(())
    }

    #[test]
    fn add_constraints_variable_arity_has_expected_constraint_lhs_values() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(150, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            Some(3),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        let slot0_var = vars.slot_vars[0][0].1;
        let slot1_var = vars.slot_vars[0][1].1;
        let slot2_var = vars.slot_vars[0][2].1;
        let bundle_formed = vars.bundle_formed.ok_or("Expected bundle_formed")?;

        let solution = MapSolution::with(&[
            (slot0_var, 1.0),
            (slot1_var, 1.0),
            (slot2_var, 0.0),
            (bundle_formed, 1.0),
        ]);

        assert_eq!(
            observed_lhs_values_for_type(&observer, "slot min (formed)", &solution),
            vec![0.0]
        );
        assert_eq!(
            observed_lhs_values_for_type(&observer, "slot max (formed)", &solution),
            vec![-1.0]
        );
        assert_eq!(
            observed_lhs_values_for_type(&observer, "bundle formed", &solution),
            vec![0.0]
        );

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert_eq!(
            state_lhs_values_for_relation(&constraints, ConstraintRelation::Geq, &solution),
            vec![0.0]
        );
        assert_eq!(
            state_lhs_values_for_relation(&constraints, ConstraintRelation::Leq, &solution),
            vec![-1.0, 0.0]
        );

        Ok(())
    }

    #[test]
    fn add_budget_constraints_variable_arity_only_blocks_when_limit_is_zero() -> TestResult {
        let empty_group = item_group_from_prices(&[]);

        let mut pb_zero = ProblemVariables::new();

        let bundle_formed_zero = pb_zero.add(variable().binary());

        let vars_zero = MixAndMatchVars {
            promotion_key: PromotionKey::default(),
            slot_vars: Vec::new(),
            y_bundle: None,
            bundle_formed: Some(bundle_formed_zero),
            target_vars: Vec::new(),
            slot_bounds: Vec::new(),
            bundle_size: 0,
            sorted_items: SmallVec::new(),
            runtime_discount: MixAndMatchRuntimeDiscount::PercentAllItems(Percentage::from(0.25)),
            application_limit: Some(0),
            monetary_limit_minor: None,
        };

        let mut state_zero = ILPState::new(pb_zero, Expression::default());
        let mut observer_zero = RecordingObserver::default();

        vars_zero.add_budget_constraints(&empty_group, &mut state_zero, &mut observer_zero)?;

        let (_pb0, _cost0, _presence0, constraints_zero) = state_zero.into_parts_with_constraints();

        assert_eq!(constraints_zero.len(), 1);
        assert_eq!(
            observer_zero
                .promotion_constraints
                .iter()
                .map(|c| c.constraint_type.as_str())
                .collect::<Vec<_>>(),
            vec!["application count budget (no bundles)"]
        );

        let mut pb_one = ProblemVariables::new();

        let bundle_formed_one = pb_one.add(variable().binary());

        let vars_one = MixAndMatchVars {
            promotion_key: PromotionKey::default(),
            slot_vars: Vec::new(),
            y_bundle: None,
            bundle_formed: Some(bundle_formed_one),
            target_vars: Vec::new(),
            slot_bounds: Vec::new(),
            bundle_size: 0,
            sorted_items: SmallVec::new(),
            runtime_discount: MixAndMatchRuntimeDiscount::PercentAllItems(Percentage::from(0.25)),
            application_limit: Some(1),
            monetary_limit_minor: None,
        };

        let mut state_one = ILPState::new(pb_one, Expression::default());
        let mut observer_one = RecordingObserver::default();

        vars_one.add_budget_constraints(&empty_group, &mut state_one, &mut observer_one)?;

        let (_pb1, _cost1, _presence1, constraints_one) = state_one.into_parts_with_constraints();

        assert!(constraints_one.is_empty());
        assert!(observer_one.promotion_constraints.is_empty());

        Ok(())
    }

    #[test]
    fn add_variables_sets_bundle_counter_upper_bound_from_floor_division() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![
            slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["main"]),
                2,
                Some(2),
            ),
            slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["drink"]),
                3,
                Some(3),
            ),
        ];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(0, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let y_bundle = vars
            .y_bundle
            .ok_or("Expected fixed-arity bundle counter variable")?;

        // Incentivize y_bundle to reach its upper bound without adding promotion constraints.
        state.add_to_objective(y_bundle, -1.0);

        let (pb, cost, _presence, constraints) = state.into_parts_with_constraints();

        assert!(constraints.is_empty());

        let model = pb.minimise(cost).using(test_solver);
        let solution = model.solve()?;

        // max_bundles = min(4/2, 3/3) = 1
        assert!((solution.value(y_bundle).round() - 1.0).abs() < f64::EPSILON);

        Ok(())
    }

    #[test]
    fn add_variables_percent_all_items_adds_discounted_slot_objective_terms() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(400, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            1,
            Some(1),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let objective_terms: FxHashMap<Variable, f64> =
            observer.objective_terms.into_iter().collect();

        for slot in &vars.slot_vars {
            for &(item_idx, var) in slot {
                let expected = match item_idx {
                    0 => 300.0, // 400 with 25% off
                    1 => 150.0, // 200 with 25% off
                    _ => panic!("Unexpected item index"),
                };

                assert_eq!(objective_terms.get(&var), Some(&expected));
            }
        }

        Ok(())
    }

    #[test]
    fn add_variables_fixed_cheapest_target_terms_use_discount_delta() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(400, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            1,
            Some(1),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedCheapest(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let objective_terms: FxHashMap<Variable, f64> =
            observer.objective_terms.into_iter().collect();

        let item_0_target = vars.target_vars[0].ok_or("Missing item 0 target")?;
        let item_1_target = vars.target_vars[1].ok_or("Missing item 1 target")?;

        assert_eq!(objective_terms.get(&item_0_target), Some(&-350.0));
        assert_eq!(objective_terms.get(&item_1_target), Some(&-150.0));

        Ok(())
    }

    #[test]
    fn calculate_discounted_minor_for_budget_handles_all_discount_types() -> TestResult {
        let percent = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::PercentAllItems(Percentage::from(0.25)),
        )?;

        assert_eq!(percent, 150);

        let amount_off_each = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::AmountOffEachItem(50),
        )?;

        assert_eq!(amount_off_each, 150);

        let fixed_price_each = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::FixedPriceEachItem(90),
        )?;

        assert_eq!(fixed_price_each, 90);

        let amount_off_total = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::AmountOffTotal(120),
        )?;

        // Conservative for bundle-total discounts in budget constraints.
        assert_eq!(amount_off_total, 0);

        let fixed_total = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::FixedTotal(120),
        )?;

        assert_eq!(fixed_total, 0);

        let fixed_cheapest = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::FixedCheapest(50),
        )?;

        assert_eq!(fixed_cheapest, 50);

        let fixed_cheapest_negative = calculate_discounted_minor_for_budget(
            200,
            MixAndMatchRuntimeDiscount::FixedCheapest(-50),
        )?;

        assert_eq!(fixed_cheapest_negative, 0);

        Ok(())
    }

    #[test]
    fn proportional_alloc_rounds_half_up_with_odd_denominator() {
        // (1*3 + 5/2) / 5 = (3+2)/5 = 1
        assert_eq!(proportional_alloc(1, 3, 5), 1);
    }

    #[test]
    fn build_bundles_strides_by_slot_min_for_each_bundle() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(110, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(120, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(130, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            Some(2),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::FixedTotal(Money::from_minor(200, GBP)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let mut values = Vec::new();

        for &(_idx, var) in &vars.slot_vars[0] {
            values.push((var, 1.0));
        }

        if let Some(y_bundle) = vars.y_bundle {
            values.push((y_bundle, 2.0));
        }

        let solution = MapSolution::with(&values);
        let bundles = build_bundles(&solution, vars);

        assert_eq!(bundles, vec![vec![0, 1], vec![2, 3]]);

        Ok(())
    }

    #[test]
    fn build_bundles_variable_arity_skips_empty_bundle() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            None,
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let bundle_formed = vars
            .bundle_formed
            .ok_or("Expected bundle_formed for variable arity")?;

        let solution = MapSolution::with(&[(bundle_formed, 1.0)]);

        let bundles = build_bundles(&solution, vars);

        assert!(bundles.is_empty());

        Ok(())
    }

    #[test]
    fn bundle_count_uses_binary_threshold_for_bundle_formed() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            None,
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        let bundle_formed = vars
            .bundle_formed
            .ok_or("Expected bundle_formed for variable arity")?;

        let true_solution = MapSolution::with(&[(bundle_formed, 1.0)]);
        let false_solution = MapSolution::with(&[(bundle_formed, 0.0)]);

        assert_eq!(vars.bundle_count(&true_solution), 1);
        assert_eq!(vars.bundle_count(&false_solution), 0);

        Ok(())
    }

    #[test]
    fn is_item_participating_requires_matching_index_and_active_var() {
        let mut pb = ProblemVariables::new();
        let v0 = pb.add(variable().binary());
        let v1 = pb.add(variable().binary());

        let vars = MixAndMatchVars {
            promotion_key: PromotionKey::default(),
            slot_vars: vec![smallvec![(0, v0), (1, v1)]],
            y_bundle: None,
            bundle_formed: None,
            target_vars: vec![None, None],
            slot_bounds: vec![(1, Some(1))],
            bundle_size: 1,
            sorted_items: SmallVec::new(),
            runtime_discount: MixAndMatchRuntimeDiscount::PercentAllItems(Percentage::from(0.0)),
            application_limit: None,
            monetary_limit_minor: None,
        };

        let solution = MapSolution::with(&[(v0, 0.0), (v1, 1.0)]);

        assert!(!vars.is_item_participating(&solution, 0));
        assert!(vars.is_item_participating(&solution, 1));
    }

    #[test]
    fn add_constraints_variable_arity_satisfies_recorded_constraints() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(150, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            Some(3),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        let slot0_var = vars.slot_vars[0][0].1;
        let slot1_var = vars.slot_vars[0][1].1;
        let slot2_var = vars.slot_vars[0][2].1;

        let bundle_formed = vars.bundle_formed.ok_or("Expected bundle_formed")?;

        let target0 = vars.target_vars[0].ok_or("Expected target var for item 0")?;
        let target1 = vars.target_vars[1].ok_or("Expected target var for item 1")?;
        let target2 = vars.target_vars[2].ok_or("Expected target var for item 2")?;

        let solution = MapSolution::with(&[
            (slot0_var, 1.0),
            (slot1_var, 1.0),
            (slot2_var, 0.0),
            (bundle_formed, 1.0),
            (target0, 1.0),
            (target1, 0.0),
            (target2, 0.0),
        ]);

        for record in &observer.promotion_constraints {
            let lhs = solution.eval(&record.expr);
            assert_relation_holds(lhs, &record.relation, record.rhs);
        }

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert_state_constraints_hold(&constraints, &solution);

        Ok(())
    }

    #[test]
    fn add_constraints_fixed_arity_emits_and_satisfies_cheapest_prefix_constraints() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(150, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            Some(2),
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(0.50)),
            PromotionBudget::unlimited(),
        );

        let mut observer = RecordingObserver::default();
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut observer)?;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        vars.add_model_constraints(promo.key(), &mut state, &mut observer);

        let cheapest_prefix_count = observer
            .promotion_constraints
            .iter()
            .filter(|record| record.constraint_type == "cheapest prefix")
            .count();

        assert_eq!(cheapest_prefix_count, vars.sorted_items.len());

        let slot0_var = vars.slot_vars[0][0].1;
        let slot1_var = vars.slot_vars[0][1].1;
        let slot2_var = vars.slot_vars[0][2].1;

        let y_bundle = vars.y_bundle.ok_or("Expected y_bundle")?;

        let target0 = vars.target_vars[0].ok_or("Expected target var for item 0")?;
        let target1 = vars.target_vars[1].ok_or("Expected target var for item 1")?;
        let target2 = vars.target_vars[2].ok_or("Expected target var for item 2")?;

        let solution = MapSolution::with(&[
            (slot0_var, 1.0),
            (slot1_var, 1.0),
            (slot2_var, 0.0),
            (y_bundle, 1.0),
            (target0, 1.0),
            (target1, 0.0),
            (target2, 0.0),
        ]);

        for record in &observer.promotion_constraints {
            let lhs = solution.eval(&record.expr);

            assert_relation_holds(lhs, &record.relation, record.rhs);
        }

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert_state_constraints_hold(&constraints, &solution);

        Ok(())
    }

    #[test]
    fn add_variables_works_with_percent_cheapest() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

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
            slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["snack"]),
                1,
                Some(1),
            ),
        ];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentCheapest(Percentage::from(1.0)), // 100% off cheapest
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Should have slot vars for all three slots
        assert_eq!(vars.slot_vars.len(), 3);

        // Should have target vars for cheapest detection
        assert_eq!(vars.target_vars.len(), 3);

        // Should use bundle counter (fixed arity)
        assert!(vars.y_bundle.is_some());

        Ok(())
    }

    #[test]
    fn add_variables_works_with_variable_arity() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["snack"]),
            ),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        // Variable arity: min=2, no max
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["snack"]),
            2,
            None,
        )];

        let promo = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            MixAndMatchDiscount::PercentAllItems(Percentage::from(0.5)), // 50% off all
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::with_presence_variables(&item_group)?;
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<MixAndMatchVars>())
            .expect("Expected mix-and-match vars");

        // Should have one slot
        assert_eq!(vars.slot_vars.len(), 1);

        // Should use bundle_formed (variable arity)
        assert!(vars.bundle_formed.is_some());
        assert!(vars.y_bundle.is_none());

        Ok(())
    }

    #[test]
    fn proportional_alloc_distributes_correctly() {
        // Test proportional allocation helper
        let result = proportional_alloc(150, 200, 300);
        assert_eq!(result, 100); // 150 * 200 / 300 = 100

        // Test with rounding
        let result = proportional_alloc(100, 1, 3);
        assert_eq!(result, 33); // (100*1 + 3/2) / 3 = 33

        // Test zero denominator
        let result = proportional_alloc(100, 50, 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn i32_from_usize_handles_overflow() {
        let result = i32_from_usize(100);
        assert_eq!(result, 100);

        let result = i32_from_usize(usize::MAX);
        assert_eq!(result, i32::MAX);
    }
}
