//! Mix-and-Match Bundle Promotions ILP

use decimal_percentage::Percentage;
use good_lp::{Expression, Solution, SolverModel, Variable, variable};
use num_traits::ToPrimitive;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use rusty_money::Money;

use crate::{
    discounts::percent_of_minor,
    items::{Item, groups::ItemGroup},
    promotions::{
        PromotionKey,
        applications::PromotionApplication,
        types::{MixAndMatchDiscount, MixAndMatchPromotion},
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

/// Solver variables for a mix-and-match promotion.
#[derive(Debug)]
pub struct MixAndMatchVars {
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
}

impl MixAndMatchVars {
    pub fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
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

    pub fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.slot_vars.iter().any(|slot| {
            slot.iter()
                .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
        })
    }

    pub fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        if let Some(var) = self.target_vars.get(item_idx).and_then(|v| *v) {
            return solution.value(var) > BINARY_THRESHOLD;
        }

        self.is_item_participating(solution, item_idx)
    }

    fn selected_exprs(&self) -> Vec<Expression> {
        let mut exprs = vec![Expression::default(); self.target_vars.len()];

        for slot in &self.slot_vars {
            for &(item_idx, var) in slot {
                if let Some(expr) = exprs.get_mut(item_idx) {
                    *expr += var;
                }
            }
        }

        exprs
    }

    /// Add mix-and-match constraints to the model.
    #[expect(
        clippy::too_many_lines,
        reason = "Constraint assembly is verbose by design."
    )]
    pub fn add_constraints<S: SolverModel, O: ILPObserver + ?Sized>(
        &self,
        mut model: S,
        promotion_key: PromotionKey,
        observer: &mut O,
    ) -> S {
        if self.slot_vars.is_empty() || (self.y_bundle.is_none() && self.bundle_formed.is_none()) {
            return model;
        }

        // Slot constraints
        for (slot_idx, slot_vars) in self.slot_vars.iter().enumerate() {
            let slot_sum: Expression = slot_vars.iter().map(|(_, var)| *var).sum();
            let (min, max) = self.slot_bounds.get(slot_idx).copied().unwrap_or((0, None));
            let min_i32 = i32_from_usize(min);

            if let Some(y_bundle) = self.y_bundle {
                let expr = slot_sum.clone() - min_i32 * y_bundle;

                observer.on_promotion_constraint(promotion_key, "slot min", &expr, ">=", 0.0);

                model = model.with(slot_sum.clone() >> (min_i32 * y_bundle));

                if let Some(max) = max {
                    let max_i32 = i32_from_usize(max);
                    let expr = slot_sum.clone() - max_i32 * y_bundle;

                    observer.on_promotion_constraint(promotion_key, "slot max", &expr, "<=", 0.0);

                    model = model.with(slot_sum.clone() << (max_i32 * y_bundle));
                }
            } else if let Some(bundle_formed) = self.bundle_formed {
                let expr = slot_sum.clone() - min_i32 * bundle_formed;

                observer.on_promotion_constraint(
                    promotion_key,
                    "slot min (formed)",
                    &expr,
                    ">=",
                    0.0,
                );

                model = model.with(slot_sum.clone() >> (min_i32 * bundle_formed));

                if let Some(max) = max {
                    let max_i32 = i32_from_usize(max);
                    let expr = slot_sum.clone() - max_i32 * bundle_formed;

                    observer.on_promotion_constraint(
                        promotion_key,
                        "slot max (formed)",
                        &expr,
                        "<=",
                        0.0,
                    );

                    model = model.with(slot_sum.clone() << (max_i32 * bundle_formed));
                }

                // If slot has enough items, bundle can be formed.
                let expr = Expression::from(bundle_formed) - slot_sum.clone() / min_i32;

                observer.on_promotion_constraint(promotion_key, "bundle formed", &expr, "<=", 0.0);

                model = model.with(bundle_formed << (slot_sum / min_i32));
            }
        }

        // Target constraints (cheapest item)
        let needs_target = self.target_vars.iter().any(Option::is_some);

        if !needs_target {
            return model;
        }

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

            model = model.with(target_var << selected_expr);
            target_sum += target_var;
        }

        if let Some(y_bundle) = self.y_bundle {
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

                    let expr = prefix_targets.clone()
                        - (prefix_selected.clone() - (k_total - 1) * y_bundle);

                    observer.on_promotion_constraint(
                        promotion_key,
                        "cheapest prefix",
                        &expr,
                        ">=",
                        0.0,
                    );

                    model = model.with(
                        prefix_targets.clone()
                            >> (prefix_selected.clone() - (k_total - 1) * y_bundle),
                    );
                }
            }

            let expr = target_sum.clone() - y_bundle;

            observer.on_promotion_constraint(promotion_key, "target count", &expr, "=", 0.0);

            model = model.with(target_sum.eq(y_bundle));
        } else if let Some(bundle_formed) = self.bundle_formed {
            let expr = target_sum.clone() - bundle_formed;

            observer.on_promotion_constraint(
                promotion_key,
                "target count (formed)",
                &expr,
                "=",
                0.0,
            );

            model = model.with(target_sum.eq(bundle_formed));
        }

        model
    }

    /// Add budget constraints for mix-and-match promotions
    pub fn add_budget_constraints<S, O: ILPObserver + ?Sized>(
        &self,
        model: S,
        promotion: &MixAndMatchPromotion<'_>,
        item_group: &ItemGroup<'_>,
        promotion_key: PromotionKey,
        observer: &mut O,
    ) -> Result<S, SolverError>
    where
        S: SolverModel,
    {
        let mut model = model;
        let budget = promotion.budget();

        // Application limit: For mix-and-match, this limits bundles
        if let Some(application_limit) = budget.application_limit {
            // Use bundle counter variable if available
            if let Some(y_bundle) = self.y_bundle {
                let limit_f64 = i64_to_f64_exact(i64::from(application_limit)).ok_or(
                    SolverError::MinorUnitsNotRepresentable(i64::from(application_limit)),
                )?;

                let expr = Expression::from(y_bundle);

                observer.on_promotion_constraint(
                    promotion_key,
                    "application count budget (bundle limit)",
                    &expr,
                    "<=",
                    limit_f64,
                );

                model = model.with(expr.leq(limit_f64));
            } else if let Some(bundle_formed) = self.bundle_formed {
                // Variable-arity bundles: bundle_formed is binary (0 or 1)
                // application_limit only makes sense if >= 1
                if application_limit == 0 {
                    let expr = Expression::from(bundle_formed);

                    observer.on_promotion_constraint(
                        promotion_key,
                        "application count budget (no bundles)",
                        &expr,
                        "=",
                        0.0,
                    );

                    model = model.with(expr.eq(0));
                }
                // If application_limit >= 1, no constraint needed (bundle_formed is already <= 1)
            }
        }

        // Monetary limit: sum(discount_amount * participation_var) <= limit
        if let Some(monetary_limit) = budget.monetary_limit {
            let mut discount_expr = Expression::default();

            // Iterate over all slot variables to compute total discount
            for slot in &self.slot_vars {
                for &(item_idx, var) in slot {
                    let item = item_group.get_item(item_idx).map_err(SolverError::from)?;

                    let full_minor = item.price().to_minor_units();

                    // Calculate discounted price based on discount type
                    let discounted_minor = calculate_discounted_price_for_item(
                        item,
                        promotion.discount(),
                        item_group,
                        &self.sorted_items,
                    )?
                    .to_minor_units();

                    let discount_amount = full_minor.saturating_sub(discounted_minor);
                    let coeff = i64_to_f64_exact(discount_amount)
                        .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                    discount_expr += var * coeff;
                }
            }

            let limit_minor = monetary_limit.to_minor_units();
            let limit_f64 = i64_to_f64_exact(limit_minor)
                .ok_or(SolverError::MinorUnitsNotRepresentable(limit_minor))?;

            observer.on_promotion_constraint(
                promotion_key,
                "monetary value budget",
                &discount_expr,
                "<=",
                limit_f64,
            );

            model = model.with(discount_expr.leq(limit_f64));
        }

        Ok(model)
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

fn discounted_minor_percent(pct: &Percentage, original_minor: i64) -> Result<i64, SolverError> {
    let discount_minor = percent_of_minor(pct, original_minor).map_err(SolverError::Discount)?;

    Ok(original_minor.saturating_sub(discount_minor))
}

fn i32_from_usize(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Calculate discounted price for a single item in mix-and-match context
fn calculate_discounted_price_for_item<'a>(
    item: &Item<'_>,
    discount: &MixAndMatchDiscount<'_>,
    item_group: &ItemGroup<'a>,
    _sorted_items: &[(usize, i64)],
) -> Result<Money<'a, rusty_money::iso::Currency>, SolverError> {
    let full_minor = item.price().to_minor_units();

    let discounted_minor = match discount {
        MixAndMatchDiscount::PercentAllItems(pct) => {
            let discount_amount =
                percent_of_minor(pct, full_minor).map_err(SolverError::Discount)?;

            full_minor.saturating_sub(discount_amount)
        }
        MixAndMatchDiscount::PercentCheapest(pct) => {
            // For cheapest-only discounts, worst case is full discount on this item
            // For budget calculation purposes, assume it could be the cheapest
            // This is conservative but ensures budget isn't violated
            let discount_amount =
                percent_of_minor(pct, full_minor).map_err(SolverError::Discount)?;

            full_minor.saturating_sub(discount_amount)
        }
        MixAndMatchDiscount::FixedTotal(_total) => {
            // For fixed total, distribute evenly across bundle
            // Conservative approximation: assume zero price (max discount)
            0
        }
        MixAndMatchDiscount::FixedCheapest(fixed_price) => {
            // Cheapest item gets fixed price, others full price
            // Conservative: assume this item could be cheapest
            fixed_price.to_minor_units()
        }
    };

    Ok(Money::from_minor(
        discounted_minor.max(0),
        item_group.currency(),
    ))
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

fn build_bundles(
    promotion: &MixAndMatchPromotion<'_>,
    solution: &dyn Solution,
    vars: &MixAndMatchVars,
) -> Vec<Vec<usize>> {
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

            for (slot_idx, slot) in promotion.slots().iter().enumerate() {
                let min = slot.min();
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

fn calculate_discounts_for_vars(
    promotion: &MixAndMatchPromotion<'_>,
    solution: &dyn Solution,
    vars: &MixAndMatchVars,
    item_group: &ItemGroup<'_>,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = FxHashMap::default();

    match promotion.discount() {
        MixAndMatchDiscount::PercentAllItems(pct) => {
            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();
                let discounted_minor = discounted_minor_percent(pct, original_minor)?;

                discounts.insert(item_idx, (original_minor, discounted_minor));
            }
        }
        MixAndMatchDiscount::PercentCheapest(pct) => {
            for (item_idx, item) in item_group.iter().enumerate() {
                if !vars.is_item_participating(solution, item_idx) {
                    continue;
                }

                let original_minor = item.price().to_minor_units();

                discounts.insert(item_idx, (original_minor, original_minor));

                if vars.is_item_priced_by_promotion(solution, item_idx) {
                    let discounted_minor = discounted_minor_percent(pct, original_minor)?;

                    discounts.insert(item_idx, (original_minor, discounted_minor));
                }
            }
        }
        MixAndMatchDiscount::FixedCheapest(amount) => {
            let fixed_minor = amount.to_minor_units().max(0);

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
        MixAndMatchDiscount::FixedTotal(amount) => {
            let bundle_price = amount.to_minor_units();
            let bundles = build_bundles(promotion, solution, vars);

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
    fn add_variables<O: ILPObserver + ?Sized>(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut O,
    ) -> Result<PromotionVars, SolverError> {
        if item_group.is_empty() {
            return Ok(PromotionVars::MixAndMatch(Box::new(MixAndMatchVars {
                slot_vars: Vec::new(),
                y_bundle: None,
                bundle_formed: None,
                target_vars: Vec::new(),
                slot_bounds: Vec::new(),
                bundle_size: 0,
                sorted_items: SmallVec::new(),
            })));
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
            return Ok(PromotionVars::MixAndMatch(Box::new(MixAndMatchVars {
                slot_vars: Vec::new(),
                y_bundle: None,
                bundle_formed: None,
                target_vars: Vec::new(),
                slot_bounds: Vec::new(),
                bundle_size: 0,
                sorted_items: SmallVec::new(),
            })));
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
                    MixAndMatchDiscount::FixedTotal(_) => 0,
                    MixAndMatchDiscount::PercentCheapest(_)
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

        Ok(PromotionVars::MixAndMatch(Box::new(MixAndMatchVars {
            slot_vars,
            y_bundle,
            bundle_formed,
            target_vars,
            slot_bounds,
            bundle_size,
            sorted_items,
        })))
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        vars: &PromotionVars,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        let vars = match vars {
            PromotionVars::MixAndMatch(vars) => vars.as_ref(),
            _ => return Ok(FxHashMap::default()),
        };

        calculate_discounts_for_vars(self, solution, vars, item_group)
    }

    fn calculate_item_applications<'a>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        vars: &PromotionVars,
        item_group: &'a ItemGroup<'_>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'a>; 10]>, SolverError> {
        let vars = match vars {
            PromotionVars::MixAndMatch(vars) => vars.as_ref(),
            _ => return Ok(SmallVec::new()),
        };

        let bundles = build_bundles(self, solution, vars);

        if bundles.is_empty() {
            return Ok(SmallVec::new());
        }

        let discounts = calculate_discounts_for_vars(self, solution, vars, item_group)?;
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

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use good_lp::{Solution, SolutionStatus, Variable};
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;
    use smallvec::SmallVec;
    use testresult::TestResult;

    #[cfg(feature = "solver-highs")]
    use good_lp::solvers::highs::highs as default_solver;
    #[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
    use good_lp::solvers::microlp::microlp as default_solver;

    use crate::{
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{PromotionKey, PromotionSlotKey, budget::PromotionBudget},
        solvers::ilp::{NoopObserver, state::ILPState},
        tags::string::StringTagCollection,
        utils::slot,
    };

    use super::*;

    #[derive(Debug, Default)]
    struct MapSolution {
        values: FxHashMap<Variable, f64>,
    }

    impl MapSolution {
        fn with(values: &[(Variable, f64)]) -> Self {
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

    fn item_group_from_prices(prices: &[i64]) -> ItemGroup<'_> {
        let items: SmallVec<[Item<'_>; 10]> = prices
            .iter()
            .map(|&price| Item::new(ProductKey::default(), Money::from_minor(price, GBP)))
            .collect();

        ItemGroup::new(items, GBP)
    }

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;
        let (pb, cost, _presence) = state.into_parts();
        let model = pb.minimise(cost).using(default_solver);

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

        let _model = vars.add_constraints(model, promo.key(), &mut observer);

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let discounts = promo.calculate_item_discounts(
            &solution,
            &PromotionVars::MixAndMatch(vars),
            &item_group,
        )?;

        assert_eq!(discounts.len(), 2);

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let discounts = promo.calculate_item_discounts(
            &solution,
            &PromotionVars::MixAndMatch(vars),
            &item_group,
        )?;

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let discounts = promo.calculate_item_discounts(
            &solution,
            &PromotionVars::MixAndMatch(vars),
            &item_group,
        )?;

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let discounts = promo.calculate_item_discounts(
            &solution,
            &PromotionVars::MixAndMatch(vars),
            &item_group,
        )?;

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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

        let applications = promo.calculate_item_applications(
            promo.key(),
            &solution,
            &PromotionVars::MixAndMatch(vars),
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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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

        let applications = promo.calculate_item_applications(
            promo.key(),
            &solution,
            &PromotionVars::MixAndMatch(vars),
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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;
        let (pb, cost, _presence) = state.into_parts();
        let model = pb.minimise(cost).using(default_solver);

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

        // Should use bundle_formed for variable arity
        assert!(vars.bundle_formed.is_some());

        let _model = vars.add_constraints(model, promo.key(), &mut observer);

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

        // Don't select any items
        let solution = MapSolution::default();
        let mut next_bundle_id = 0;

        let applications = promo.calculate_item_applications(
            promo.key(),
            &solution,
            &PromotionVars::MixAndMatch(vars),
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
            slot_vars: vec![SmallVec::new()],
            y_bundle: None,
            bundle_formed: None,
            target_vars: Vec::new(),
            slot_bounds: Vec::new(),
            bundle_size: 0,
            sorted_items: SmallVec::new(),
        };

        let solution = MapSolution::default();
        assert!(!vars.is_item_priced_by_promotion(&solution, 0));
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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

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
        let vars = promo.add_variables(promo.key(), &item_group, &mut state, &mut observer)?;

        let PromotionVars::MixAndMatch(vars) = vars else {
            panic!("Expected mix-and-match vars");
        };

        // Should have one slot
        assert_eq!(vars.slot_vars.len(), 1);

        // Should use bundle_formed (variable arity)
        assert!(vars.bundle_formed.is_some());
        assert!(vars.y_bundle.is_none());

        Ok(())
    }
}
