//! Tiered Threshold Promotions ILP

use decimal_percentage::Percentage;
use good_lp::{Expression, Solution, Variable, variable};
use rustc_hash::FxHashMap;
use rusty_money::Money;
use smallvec::SmallVec;

use crate::{
    discounts::percent_of_minor,
    items::groups::ItemGroup,
    promotions::{
        PromotionKey,
        applications::PromotionApplication,
        types::{ThresholdDiscount, TierThreshold, TieredThresholdPromotion},
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

/// Per-qualifying-tier data captured during variable creation.
#[derive(Debug)]
struct QualifyingTier {
    /// Optional lower spend threshold in minor units.
    lower_monetary_threshold_minor: Option<i64>,

    /// Optional lower minimum number of contributing items required.
    lower_item_count_threshold: Option<u32>,

    /// Optional upper spend threshold in minor units.
    upper_monetary_threshold_minor: Option<i64>,

    /// Optional upper maximum number of contributing items.
    upper_item_count_threshold: Option<u32>,

    /// Binary auxiliary variable: is this tier active?
    tier_var: Variable,

    /// All participating item variables (contribution and/or discount items).
    item_vars: SmallVec<[(usize, Variable); 10]>,

    /// Subset of `item_vars` that count toward threshold spend.
    contribution_vars: SmallVec<[(usize, Variable); 10]>,

    /// Subset of `item_vars` that can receive this tier's discount.
    discount_vars: SmallVec<[(usize, Variable); 10]>,

    /// Pre-computed discounted prices for discount-eligible items (per-item discount types only).
    discounted_minor_by_item: FxHashMap<usize, i64>,

    /// Target variables for cheapest-item discounts, sorted by price ascending.
    target_vars: SmallVec<[(usize, Variable); 10]>,

    /// Whether this tier uses per-item pricing from `discounted_minor_by_item`.
    has_per_item_discount: bool,

    /// Bundle-level fixed amount off total discount.
    amount_off_total_minor: Option<i64>,

    /// Bundle-level fixed total discount.
    fixed_total_minor: Option<i64>,

    /// Cheapest-item percent discount.
    percent_cheapest: Option<Percentage>,

    /// Cheapest-item fixed-price discount.
    fixed_cheapest_minor: Option<i64>,

    /// Cheapest item free discount.
    cheapest_free: bool,
}

impl QualifyingTier {
    fn has_bundle_total_discount(&self) -> bool {
        self.amount_off_total_minor.is_some() || self.fixed_total_minor.is_some()
    }

    fn has_per_item_discount(&self) -> bool {
        self.has_per_item_discount
    }
}

/// Solver variables for a tiered threshold promotion.
#[derive(Debug)]
pub struct TieredThresholdPromotionVars {
    /// Promotion key for observer/application output.
    promotion_key: PromotionKey,

    /// Qualifying tiers with their solver variables.
    qualifying_tiers: Vec<QualifyingTier>,

    /// Budget: optional max applications.
    application_limit: Option<u32>,

    /// Budget: optional max total discount value in minor units.
    monetary_limit_minor: Option<i64>,
}

impl TieredThresholdPromotionVars {
    /// Collect all item participation variables across all qualifying tiers.
    fn all_item_vars(&self) -> impl Iterator<Item = &(usize, Variable)> {
        self.qualifying_tiers
            .iter()
            .flat_map(|t| t.item_vars.iter())
    }

    /// Return the single active tier in the solved model, if any.
    fn active_tier<'a>(&'a self, solution: &dyn Solution) -> Option<&'a QualifyingTier> {
        self.qualifying_tiers
            .iter()
            .find(|qt| solution.value(qt.tier_var) > BINARY_THRESHOLD)
    }
}

impl ILPPromotionVars for TieredThresholdPromotionVars {
    fn add_item_participation_term(&self, expr: Expression, item_idx: usize) -> Expression {
        let mut updated_expr = expr;

        for (idx, var) in self.all_item_vars() {
            if *idx == item_idx {
                updated_expr += *var;
            }
        }

        updated_expr
    }

    fn is_item_participating(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        self.all_item_vars()
            .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
    }

    fn is_item_priced_by_promotion(&self, solution: &dyn Solution, item_idx: usize) -> bool {
        let Some(active_tier) = self.active_tier(solution) else {
            return false;
        };

        if active_tier.has_per_item_discount() || active_tier.has_bundle_total_discount() {
            return is_item_selected(&active_tier.discount_vars, solution, item_idx);
        }

        // Cheapest-item variants only alter the targeted item's final price.
        is_item_selected(&active_tier.target_vars, solution, item_idx)
    }

    fn add_constraints(
        &self,
        _promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_at_most_one_tier_constraint(state, observer);

        for qt in &self.qualifying_tiers {
            self.add_constraints_for_tier(qt, item_group, state, observer)?;
        }

        // Budget constraints
        self.add_budget_constraints(self.promotion_key, item_group, state, observer)?;

        Ok(())
    }

    fn calculate_item_discounts(
        &self,
        solution: &dyn Solution,
        item_group: &ItemGroup<'_>,
    ) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
        if let Some(active_tier) = self.active_tier(solution) {
            return calculate_discounts_for_tier(active_tier, solution, item_group);
        }

        Ok(FxHashMap::default())
    }

    fn calculate_item_applications<'b>(
        &self,
        promotion_key: PromotionKey,
        solution: &dyn Solution,
        item_group: &ItemGroup<'b>,
        next_bundle_id: &mut usize,
    ) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, SolverError> {
        let discounts = self.calculate_item_discounts(solution, item_group)?;

        if discounts.is_empty() {
            return Ok(SmallVec::new());
        }

        let bundle_id = *next_bundle_id;
        *next_bundle_id += 1;

        let currency = item_group.currency();

        let mut applications = SmallVec::new();

        let mut sorted_discounts: SmallVec<[(usize, (i64, i64)); 10]> = discounts
            .iter()
            .map(|(&item_idx, &(original_minor, final_minor))| {
                (item_idx, (original_minor, final_minor))
            })
            .collect();

        sorted_discounts.sort_by_key(|(item_idx, _)| *item_idx);

        for (item_idx, (original_minor, final_minor)) in sorted_discounts {
            applications.push(PromotionApplication {
                promotion_key,
                item_idx,
                bundle_id,
                original_price: Money::from_minor(original_minor, currency),
                final_price: Money::from_minor(final_minor, currency),
            });
        }

        Ok(applications)
    }
}

impl TieredThresholdPromotionVars {
    fn add_at_most_one_tier_constraint(
        &self,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        if self.qualifying_tiers.len() <= 1 {
            return;
        }

        let tier_sum: Expression = self.qualifying_tiers.iter().map(|t| t.tier_var).sum();

        observer.on_promotion_constraint(
            self.promotion_key,
            "at most one tier",
            &tier_sum,
            "<=",
            1.0,
        );

        state.add_leq_constraint(tier_sum, 1.0);
    }

    fn add_constraints_for_tier(
        &self,
        qt: &QualifyingTier,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        self.add_tier_item_link_constraints(qt, state, observer);
        self.add_lower_threshold_constraints(qt, item_group, state, observer)?;
        self.add_upper_threshold_constraints(qt, item_group, state, observer)?;
        self.add_tier_activation_constraint(qt, state, observer);

        if !qt.target_vars.is_empty() {
            add_cheapest_constraints(qt, self.promotion_key, state, observer);
        }

        Ok(())
    }

    fn add_tier_item_link_constraints(
        &self,
        qt: &QualifyingTier,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        // Link items to their tier: d_{t,i} <= tier_t
        for &(_item_idx, item_var) in &qt.item_vars {
            let link_expr = Expression::from(item_var) - Expression::from(qt.tier_var);

            observer.on_promotion_constraint(
                self.promotion_key,
                "tier-item link",
                &link_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(link_expr, 0.0);
        }
    }

    fn add_lower_threshold_constraints(
        &self,
        qt: &QualifyingTier,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        if let Some(lower_monetary_threshold_minor) = qt.lower_monetary_threshold_minor {
            // Threshold spend: sum(price_i * c_{t,i}) >= threshold_t * tier_t
            let contribution_expr = weighted_price_sum_expr(item_group, &qt.contribution_vars)?;

            let threshold_coeff = i64_to_f64_exact(lower_monetary_threshold_minor).ok_or(
                SolverError::MinorUnitsNotRepresentable(lower_monetary_threshold_minor),
            )?;

            let threshold_expr =
                contribution_expr - Expression::from(qt.tier_var) * threshold_coeff;

            observer.on_promotion_constraint(
                self.promotion_key,
                "lower threshold spend",
                &threshold_expr,
                ">=",
                0.0,
            );

            state.add_geq_constraint(threshold_expr, 0.0);
        }

        if let Some(lower_item_count_threshold) = qt.lower_item_count_threshold {
            // Threshold item count: sum(c_{t,i}) >= item_count_t * tier_t
            let contribution_count_expr: Expression =
                qt.contribution_vars.iter().map(|(_, v)| *v).sum();

            let item_count_coeff = u32_to_f64_exact(lower_item_count_threshold)?;

            let item_count_expr =
                contribution_count_expr - Expression::from(qt.tier_var) * item_count_coeff;

            observer.on_promotion_constraint(
                self.promotion_key,
                "lower threshold item count",
                &item_count_expr,
                ">=",
                0.0,
            );

            state.add_geq_constraint(item_count_expr, 0.0);
        }

        Ok(())
    }

    fn add_upper_threshold_constraints(
        &self,
        qt: &QualifyingTier,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        // Optional upper caps: once reached, additional items stop contributing
        // to this tier instance's qualification and discountable value.
        if let Some(upper_monetary_threshold_minor) = qt.upper_monetary_threshold_minor {
            let contribution_expr = weighted_price_sum_expr(item_group, &qt.contribution_vars)?;
            let discountable_expr = weighted_price_sum_expr(item_group, &qt.discount_vars)?;

            let upper_coeff = i64_to_f64_exact(upper_monetary_threshold_minor).ok_or(
                SolverError::MinorUnitsNotRepresentable(upper_monetary_threshold_minor),
            )?;

            let contribution_cap_expr =
                contribution_expr - Expression::from(qt.tier_var) * upper_coeff;

            let discountable_cap_expr =
                discountable_expr - Expression::from(qt.tier_var) * upper_coeff;

            observer.on_promotion_constraint(
                self.promotion_key,
                "upper threshold spend (contribution)",
                &contribution_cap_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(contribution_cap_expr, 0.0);

            observer.on_promotion_constraint(
                self.promotion_key,
                "upper threshold spend (discountable)",
                &discountable_cap_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(discountable_cap_expr, 0.0);
        }

        if let Some(upper_item_count_threshold) = qt.upper_item_count_threshold {
            let contribution_count_expr: Expression =
                qt.contribution_vars.iter().map(|(_, v)| *v).sum();

            let discountable_count_expr: Expression =
                qt.discount_vars.iter().map(|(_, v)| *v).sum();

            let upper_count_coeff = u32_to_f64_exact(upper_item_count_threshold)?;

            let contribution_count_cap_expr =
                contribution_count_expr - Expression::from(qt.tier_var) * upper_count_coeff;

            let discountable_count_cap_expr =
                discountable_count_expr - Expression::from(qt.tier_var) * upper_count_coeff;

            observer.on_promotion_constraint(
                self.promotion_key,
                "upper threshold item count (contribution)",
                &contribution_count_cap_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(contribution_count_cap_expr, 0.0);

            observer.on_promotion_constraint(
                self.promotion_key,
                "upper threshold item count (discountable)",
                &discountable_count_cap_expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(discountable_count_cap_expr, 0.0);
        }

        Ok(())
    }

    fn add_tier_activation_constraint(
        &self,
        qt: &QualifyingTier,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) {
        // Tier activation: tier_t <= sum(d_{t,i})
        // Required for bundle-total discounts to prevent free savings when no
        // discount-target item is selected.
        if !qt.has_bundle_total_discount() {
            return;
        }

        let discount_sum: Expression = qt.discount_vars.iter().map(|(_, v)| *v).sum();
        let expr = Expression::from(qt.tier_var) - discount_sum;

        observer.on_promotion_constraint(self.promotion_key, "tier activation", &expr, "<=", 0.0);

        state.add_leq_constraint(expr, 0.0);
    }

    /// Add budget constraints to the ILP state.
    fn add_budget_constraints(
        &self,
        promotion_key: PromotionKey,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<(), SolverError> {
        // Application count limit: sum(active tiers) <= limit
        if let Some(application_limit) = self.application_limit {
            let tier_sum: Expression = self.qualifying_tiers.iter().map(|qt| qt.tier_var).sum();

            let limit_f64 = i64_to_f64_exact(i64::from(application_limit)).ok_or(
                SolverError::MinorUnitsNotRepresentable(i64::from(application_limit)),
            )?;

            observer.on_promotion_constraint(
                promotion_key,
                "application count budget (tier limit)",
                &tier_sum,
                "<=",
                limit_f64,
            );

            state.add_leq_constraint(tier_sum, limit_f64);
        }

        // Monetary limit: sum((full_price - discounted_price) * var) <= limit
        if let Some(limit_minor) = self.monetary_limit_minor {
            let mut discount_expr = Expression::default();

            for qt in &self.qualifying_tiers {
                if qt.percent_cheapest.is_some()
                    || qt.fixed_cheapest_minor.is_some()
                    || qt.cheapest_free
                {
                    // Cheapest-item modes are exact with target vars: only targets consume budget.
                    for &(item_idx, target_var) in &qt.target_vars {
                        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
                        let full_minor = item.price().to_minor_units();
                        let discounted_minor =
                            estimate_target_discounted_minor_for_budget(qt, full_minor)?;

                        let discount_amount = full_minor.saturating_sub(discounted_minor);
                        let coeff = i64_to_f64_exact(discount_amount)
                            .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                        discount_expr += target_var * coeff;
                    }
                } else {
                    for &(item_idx, var) in &qt.item_vars {
                        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
                        let full_minor = item.price().to_minor_units();
                        let discounted_minor =
                            estimate_discounted_minor_for_budget(qt, item_idx, var, full_minor)?;

                        let discount_amount = full_minor.saturating_sub(discounted_minor);
                        let coeff = i64_to_f64_exact(discount_amount)
                            .ok_or(SolverError::MinorUnitsNotRepresentable(discount_amount))?;

                        discount_expr += var * coeff;
                    }
                }
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

fn weighted_price_sum_expr(
    item_group: &ItemGroup<'_>,
    vars: &SmallVec<[(usize, Variable); 10]>,
) -> Result<Expression, SolverError> {
    let mut expr = Expression::default();

    for &(item_idx, var) in vars {
        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
        let minor = item.price().to_minor_units();
        let coeff =
            i64_to_f64_exact(minor).ok_or(SolverError::MinorUnitsNotRepresentable(minor))?;

        expr += var * coeff;
    }

    Ok(expr)
}

fn u32_to_f64_exact(value: u32) -> Result<f64, SolverError> {
    let as_i64 = i64::from(value);

    i64_to_f64_exact(as_i64).ok_or(SolverError::MinorUnitsNotRepresentable(as_i64))
}

fn is_item_selected(
    vars: &SmallVec<[(usize, Variable); 10]>,
    solution: &dyn Solution,
    item_idx: usize,
) -> bool {
    vars.iter()
        .any(|&(idx, var)| idx == item_idx && solution.value(var) > BINARY_THRESHOLD)
}

fn estimate_discounted_minor_for_budget(
    tier: &QualifyingTier,
    item_idx: usize,
    item_var: Variable,
    full_minor: i64,
) -> Result<i64, SolverError> {
    let is_discount_item = tier
        .discount_vars
        .iter()
        .any(|(idx, discount_var)| *idx == item_idx && *discount_var == item_var);

    if !is_discount_item {
        return Ok(full_minor);
    }

    // For per-item discounts, use the exact pre-computed per-item price.
    if tier.has_per_item_discount() {
        return tier.discounted_minor_by_item.get(&item_idx).copied().ok_or(
            SolverError::InvariantViolation {
                message: "missing discounted value for discount-target item",
            },
        );
    }

    // For fixed-cheapest, the target item cannot drop below the fixed price.
    if let Some(fixed) = tier.fixed_cheapest_minor {
        return Ok(fixed.max(0));
    }

    // Conservative for other bundle-level modes: assume item could be free.
    Ok(0)
}

fn estimate_target_discounted_minor_for_budget(
    tier: &QualifyingTier,
    full_minor: i64,
) -> Result<i64, SolverError> {
    if tier.cheapest_free {
        return Ok(0);
    }

    if let Some(pct) = tier.percent_cheapest {
        let discount_amount = percent_of_minor(&pct, full_minor).map_err(SolverError::Discount)?;

        return Ok(full_minor.saturating_sub(discount_amount));
    }

    if let Some(fixed) = tier.fixed_cheapest_minor {
        return Ok(fixed.max(0));
    }

    Err(SolverError::InvariantViolation {
        message: "missing cheapest-item budget mode",
    })
}

/// Add cheapest-item constraints for a qualifying tier.
fn add_cheapest_constraints(
    qt: &QualifyingTier,
    promotion_key: PromotionKey,
    state: &mut ILPState,
    observer: &mut dyn ILPObserver,
) {
    // target_i <= d_{t,i} (can only target a claimed item)
    for &(item_idx, target_var) in &qt.target_vars {
        if let Some(&(_, item_var)) = qt.discount_vars.iter().find(|(idx, _)| *idx == item_idx) {
            let expr = Expression::from(target_var) - Expression::from(item_var);

            observer.on_promotion_constraint(
                promotion_key,
                "target implies claimed",
                &expr,
                "<=",
                0.0,
            );

            state.add_leq_constraint(expr, 0.0);
        }
    }

    // sum(target_i) <= tier_t (at most one target when tier active)
    let target_sum: Expression = qt.target_vars.iter().map(|(_, v)| *v).sum();
    let expr = target_sum - Expression::from(qt.tier_var);

    observer.on_promotion_constraint(promotion_key, "target count", &expr, "<=", 0.0);
    state.add_leq_constraint(expr, 0.0);

    // Cheapest ordering: target_k + d_{k-1} <= 1
    // (target_vars are sorted by price ascending)
    for k in 1..qt.target_vars.len() {
        let Some(&(prev_idx, _)) = qt.target_vars.get(k - 1) else {
            continue;
        };

        let Some(&(_, curr_target)) = qt.target_vars.get(k) else {
            continue;
        };

        let Some(&(_, prev_d)) = qt.discount_vars.iter().find(|(idx, _)| *idx == prev_idx) else {
            continue;
        };

        let expr = Expression::from(curr_target) + Expression::from(prev_d);

        observer.on_promotion_constraint(promotion_key, "cheapest ordering", &expr, "<=", 1.0);

        state.add_leq_constraint(expr, 1.0);
    }
}

/// Compute final per-item prices for the active tier.
fn calculate_discounts_for_tier(
    qt: &QualifyingTier,
    solution: &dyn Solution,
    item_group: &ItemGroup<'_>,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = if qt.has_per_item_discount() {
        calculate_per_item_discounts(qt, solution, item_group)?
    } else if let Some(amount) = qt.amount_off_total_minor {
        calculate_total_discounts(&qt.discount_vars, solution, item_group, &|total| {
            total.saturating_sub(amount)
        })?
    } else if let Some(fixed) = qt.fixed_total_minor {
        calculate_total_discounts(&qt.discount_vars, solution, item_group, &|_total| {
            fixed.max(0)
        })?
    } else if qt.cheapest_free {
        calculate_cheapest_discounts(
            &qt.discount_vars,
            &qt.target_vars,
            solution,
            item_group,
            &|_price| 0,
        )?
    } else if let Some(pct) = qt.percent_cheapest {
        calculate_cheapest_discounts(
            &qt.discount_vars,
            &qt.target_vars,
            solution,
            item_group,
            &|price| {
                let savings = percent_of_minor(&pct, price).unwrap_or(0);

                (price - savings).max(0)
            },
        )?
    } else if let Some(fixed) = qt.fixed_cheapest_minor {
        calculate_cheapest_discounts(
            &qt.discount_vars,
            &qt.target_vars,
            solution,
            item_group,
            &|_price| fixed.max(0),
        )?
    } else {
        return Err(SolverError::InvariantViolation {
            message: "qualifying tier has no discount mode configured",
        });
    };

    // Participation is exclusive across promotions even for non-discounted
    // contribution items; include them with full prices.
    for &(item_idx, item_var) in &qt.item_vars {
        if solution.value(item_var) <= BINARY_THRESHOLD || discounts.contains_key(&item_idx) {
            continue;
        }

        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
        let full_minor = item.price().to_minor_units();

        discounts.insert(item_idx, (full_minor, full_minor));
    }

    Ok(discounts)
}

/// Per-item discount: use pre-computed discounted prices.
fn calculate_per_item_discounts(
    qt: &QualifyingTier,
    solution: &dyn Solution,
    item_group: &ItemGroup<'_>,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = FxHashMap::default();

    for &(item_idx, item_var) in &qt.discount_vars {
        if solution.value(item_var) <= BINARY_THRESHOLD {
            continue;
        }

        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;

        let discounted = qt.discounted_minor_by_item.get(&item_idx).copied().ok_or(
            SolverError::InvariantViolation {
                message: "missing discounted value for participating item",
            },
        )?;

        discounts.insert(item_idx, (item.price().to_minor_units(), discounted));
    }

    Ok(discounts)
}

/// Bundle-total discount: distribute the new total proportionally.
fn calculate_total_discounts(
    discount_vars: &SmallVec<[(usize, Variable); 10]>,
    solution: &dyn Solution,
    item_group: &ItemGroup<'_>,
    new_total: &dyn Fn(i64) -> i64,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = FxHashMap::default();

    let mut claimed: SmallVec<[(usize, i64); 10]> = SmallVec::new();

    for &(item_idx, item_var) in discount_vars {
        if solution.value(item_var) <= BINARY_THRESHOLD {
            continue;
        }

        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;

        claimed.push((item_idx, item.price().to_minor_units()));
    }

    if claimed.is_empty() {
        return Ok(discounts);
    }

    let original_total: i64 = claimed.iter().map(|(_, p)| p).sum();
    let target_total = new_total(original_total);

    let mut remaining = target_total;

    for (i, &(item_idx, full_minor)) in claimed.iter().enumerate() {
        let final_minor = if i == claimed.len() - 1 {
            remaining
        } else if original_total == 0 {
            0
        } else {
            proportional_alloc(target_total, full_minor, original_total)
        };

        remaining -= final_minor;

        discounts.insert(item_idx, (full_minor, final_minor));
    }

    Ok(discounts)
}

/// Cheapest-item discount: target item gets the discount, others at full price.
fn calculate_cheapest_discounts(
    discount_vars: &SmallVec<[(usize, Variable); 10]>,
    target_vars: &SmallVec<[(usize, Variable); 10]>,
    solution: &dyn Solution,
    item_group: &ItemGroup<'_>,
    target_price: &dyn Fn(i64) -> i64,
) -> Result<FxHashMap<usize, (i64, i64)>, SolverError> {
    let mut discounts = FxHashMap::default();

    let target_idx = target_vars
        .iter()
        .find(|(_, var)| solution.value(*var) > BINARY_THRESHOLD)
        .map(|(idx, _)| *idx);

    for &(item_idx, item_var) in discount_vars {
        if solution.value(item_var) <= BINARY_THRESHOLD {
            continue;
        }

        let item = item_group.get_item(item_idx).map_err(SolverError::from)?;
        let full = item.price().to_minor_units();

        let final_minor = if Some(item_idx) == target_idx {
            target_price(full)
        } else {
            full
        };

        discounts.insert(item_idx, (full, final_minor));
    }

    Ok(discounts)
}

/// Proportionally allocate a total across items by their share of the denominator.
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

/// Create target variables for cheapest-item discount types.
fn build_target_vars(
    eligible: &SmallVec<[(usize, i64); 10]>,
    percent_cheapest: Option<Percentage>,
    fixed_cheapest_minor: Option<i64>,
    cheapest_free: bool,
    promotion_key: PromotionKey,
    state: &mut ILPState,
    observer: &mut dyn ILPObserver,
) -> Result<SmallVec<[(usize, Variable); 10]>, SolverError> {
    // Sort by price ascending for cheapest ordering constraints
    let mut sorted: SmallVec<[(usize, i64); 10]> = eligible.clone();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    let mut target_vars = SmallVec::new();

    for &(item_idx, price) in &sorted {
        let var = state.problem_variables_mut().add(variable().binary());

        target_vars.push((item_idx, var));

        let savings = if cheapest_free {
            price
        } else if let Some(pct) = percent_cheapest {
            percent_of_minor(&pct, price).map_err(SolverError::Discount)?
        } else if let Some(fixed) = fixed_cheapest_minor {
            price.saturating_sub(fixed.max(0))
        } else {
            0
        };

        if savings != 0 {
            let coeff = i64_to_f64_exact(savings)
                .ok_or(SolverError::MinorUnitsNotRepresentable(savings))?;

            state.add_to_objective(var, -coeff);
            observer.on_objective_term(var, -coeff);
        }

        let discounted = price - savings;

        observer.on_promotion_variable(promotion_key, item_idx, var, discounted, Some("target"));
    }

    Ok(target_vars)
}

impl ILPPromotion for TieredThresholdPromotion<'_> {
    fn key(&self) -> PromotionKey {
        TieredThresholdPromotion::key(self)
    }

    fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        if item_group.is_empty() || self.tiers().is_empty() {
            return false;
        }

        // At least one tier must have items matching its discount qualification.
        self.tiers().iter().any(|tier| {
            item_group
                .iter()
                .any(|item| tier.discount_qualification().matches(item.tags()))
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Variable creation for multiple discount types"
    )]
    fn add_variables(
        &self,
        item_group: &ItemGroup<'_>,
        state: &mut ILPState,
        observer: &mut dyn ILPObserver,
    ) -> Result<PromotionVars, SolverError> {
        let promotion_key = self.key();
        let mut qualifying_tiers = Vec::new();

        for (tier_idx, tier) in self.tiers().iter().enumerate() {
            let lower_monetary_threshold_minor = tier
                .lower_threshold()
                .monetary_threshold()
                .map(Money::to_minor_units);

            let lower_item_count_threshold = tier.lower_threshold().item_count_threshold();

            let upper_monetary_threshold_minor = tier
                .upper_threshold()
                .and_then(TierThreshold::monetary_threshold)
                .map(Money::to_minor_units);

            let upper_item_count_threshold = tier
                .upper_threshold()
                .and_then(TierThreshold::item_count_threshold);

            let contribution_qualification = tier.contribution_qualification();
            let discount_qualification = tier.discount_qualification();

            let contribution_total: i64 = item_group
                .iter()
                .filter(|item| contribution_qualification.matches(item.tags()))
                .map(|item| item.price().to_minor_units())
                .sum();

            let contribution_count = item_group
                .iter()
                .filter(|item| contribution_qualification.matches(item.tags()))
                .count();

            let contribution_count_u32 = u32::try_from(contribution_count).unwrap_or(u32::MAX);

            // Skip tiers that can never meet their thresholds even if they claim all
            // available contribution items.
            if lower_monetary_threshold_minor
                .is_some_and(|threshold| contribution_total < threshold)
            {
                continue;
            }

            if lower_item_count_threshold
                .is_some_and(|threshold| contribution_count_u32 < threshold)
            {
                continue;
            }

            if lower_monetary_threshold_minor
                .zip(upper_monetary_threshold_minor)
                .is_some_and(|(lower, upper)| upper < lower)
            {
                continue;
            }

            if lower_item_count_threshold
                .zip(upper_item_count_threshold)
                .is_some_and(|(lower, upper)| upper < lower)
            {
                continue;
            }

            // Create tier auxiliary variable
            let tier_var = state.problem_variables_mut().add(variable().binary());

            // Determine discount mode and tier_var objective coefficient.
            let (
                has_per_item_discount,
                amount_off_total_minor,
                fixed_total_minor,
                percent_cheapest,
                fixed_cheapest_minor,
                cheapest_free,
                tier_var_coeff,
            ) = match tier.discount() {
                ThresholdDiscount::PercentEachItem(_)
                | ThresholdDiscount::AmountOffEachItem(_)
                | ThresholdDiscount::FixedPriceEachItem(_) => {
                    (true, None, None, None, None, false, 0_i64)
                }
                ThresholdDiscount::AmountOffTotal(a) => {
                    let m = a.to_minor_units();

                    (false, Some(m), None, None, None, false, -m)
                }
                ThresholdDiscount::FixedTotal(a) => {
                    let m = a.to_minor_units();

                    (false, None, Some(m), None, None, false, m)
                }

                ThresholdDiscount::PercentCheapest(pct) => {
                    (false, None, None, Some(*pct), None, false, 0)
                }
                ThresholdDiscount::FixedCheapest(a) => {
                    (false, None, None, None, Some(a.to_minor_units()), false, 0)
                }
            };

            // Register tier_var with observer and objective
            observer.on_promotion_variable(
                promotion_key,
                tier_idx,
                tier_var,
                tier_var_coeff,
                Some("tier-selector"),
            );

            if tier_var_coeff != 0 {
                let coeff = i64_to_f64_exact(tier_var_coeff)
                    .ok_or(SolverError::MinorUnitsNotRepresentable(tier_var_coeff))?;

                state.add_to_objective(tier_var, coeff);

                observer.on_objective_term(tier_var, coeff);
            }

            // Create participation variables. Items that contribute to the
            // threshold and/or receive discount are participating and therefore
            // exclusive against other promotions in this layer.
            let mut item_vars = SmallVec::new();
            let mut contribution_vars = SmallVec::new();
            let mut discount_vars = SmallVec::new();
            let mut discount_eligible: SmallVec<[(usize, i64); 10]> = SmallVec::new();
            let mut discounted_minor_by_item = FxHashMap::default();

            for (item_idx, item) in item_group.iter().enumerate() {
                let price = item.price().to_minor_units();

                let contributes = contribution_qualification.matches(item.tags());
                let discountable = discount_qualification.matches(item.tags());

                if !contributes && !discountable {
                    continue;
                }

                let item_var = state.problem_variables_mut().add(variable().binary());
                item_vars.push((item_idx, item_var));

                if contributes {
                    contribution_vars.push((item_idx, item_var));
                }

                if discountable {
                    discount_vars.push((item_idx, item_var));
                    discount_eligible.push((item_idx, price));
                }

                let coeff_minor = if has_per_item_discount {
                    if discountable {
                        let discounted =
                            TieredThresholdPromotion::calculate_discounted_price(tier, item)
                                .map_err(SolverError::from)?
                                .to_minor_units();

                        discounted_minor_by_item.insert(item_idx, discounted);

                        discounted
                    } else {
                        // Contribution-only item in this tier.
                        price
                    }
                } else if fixed_total_minor.is_some() {
                    if discountable {
                        // Discount targets are priced via tier_var.
                        0
                    } else {
                        // Contribution-only item remains full price.
                        price
                    }
                } else {
                    // Full-price coefficient (amount-off-total, cheapest variants).
                    price
                };

                if coeff_minor != 0 {
                    let coeff = i64_to_f64_exact(coeff_minor)
                        .ok_or(SolverError::MinorUnitsNotRepresentable(coeff_minor))?;

                    state.add_to_objective(item_var, coeff);

                    observer.on_objective_term(item_var, coeff);
                }

                observer.on_promotion_variable(
                    promotion_key,
                    item_idx,
                    item_var,
                    coeff_minor,
                    None,
                );
            }

            // Create target variables for cheapest-item discounts
            let has_cheapest_discount =
                cheapest_free || percent_cheapest.is_some() || fixed_cheapest_minor.is_some();

            let target_vars = if has_cheapest_discount {
                build_target_vars(
                    &discount_eligible,
                    percent_cheapest,
                    fixed_cheapest_minor,
                    cheapest_free,
                    promotion_key,
                    state,
                    observer,
                )?
            } else {
                SmallVec::new()
            };

            qualifying_tiers.push(QualifyingTier {
                lower_monetary_threshold_minor,
                lower_item_count_threshold,
                upper_monetary_threshold_minor,
                upper_item_count_threshold,
                tier_var,
                item_vars,
                contribution_vars,
                discount_vars,
                discounted_minor_by_item,
                target_vars,
                has_per_item_discount,
                amount_off_total_minor,
                fixed_total_minor,
                percent_cheapest,
                fixed_cheapest_minor,
                cheapest_free,
            });
        }

        Ok(Box::new(TieredThresholdPromotionVars {
            promotion_key,
            qualifying_tiers,
            application_limit: self.budget().application_limit,
            monetary_limit_minor: self.budget().monetary_limit.map(|v| v.to_minor_units()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use decimal_percentage::Percentage;
    use good_lp::{Expression, IntoAffineExpression, ProblemVariables, Variable, variable};
    use rusty_money::{
        Money,
        iso::{self, GBP},
    };
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            PromotionKey,
            budget::PromotionBudget,
            qualification::Qualification,
            types::{ThresholdDiscount, ThresholdTier},
        },
        solvers::{
            SolverError,
            ilp::{
                NoopObserver,
                promotions::{
                    ILPPromotion,
                    test_support::{
                        CountingObserver, MapSolution, PromotionVarCaptureObserver,
                        RecordingObserver, SelectAllSolution, SelectNoneSolution,
                        assert_relation_holds, assert_state_constraints_hold,
                        item_group_from_items, item_group_from_prices,
                    },
                },
            },
        },
        tags::string::StringTagCollection,
    };

    use super::*;

    fn make_tier_with_tags<'a>(
        threshold_minor: i64,
        contribution_tags: &[&str],
        discount_tags: &[&str],
        discount: ThresholdDiscount<'a>,
    ) -> ThresholdTier<'a, StringTagCollection> {
        ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(threshold_minor, GBP)),
            None,
            Qualification::match_any(StringTagCollection::from_strs(contribution_tags)),
            Qualification::match_any(StringTagCollection::from_strs(discount_tags)),
            discount,
        )
    }

    fn make_tier_with_tags_and_item_count<'a>(
        threshold_minor: i64,
        item_count_threshold: u32,
        contribution_tags: &[&str],
        discount_tags: &[&str],
        discount: ThresholdDiscount<'a>,
    ) -> ThresholdTier<'a, StringTagCollection> {
        ThresholdTier::new(
            TierThreshold::with_both_thresholds(
                Money::from_minor(threshold_minor, GBP),
                item_count_threshold,
            ),
            None,
            Qualification::match_any(StringTagCollection::from_strs(contribution_tags)),
            Qualification::match_any(StringTagCollection::from_strs(discount_tags)),
            discount,
        )
    }

    #[test]
    fn is_applicable_returns_false_for_empty_items() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn is_applicable_returns_false_for_no_tiers() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![],
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn is_applicable_returns_false_when_no_discount_tag_matches() {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["wine"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &["no-match"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn is_applicable_returns_true_with_empty_discount_tags() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &[],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        assert!(promo.is_applicable(&item_group));
    }

    #[test]
    fn add_variables_produces_no_vars_when_no_tiers_qualify() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                5000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let expr = vars.add_item_participation_term(Expression::default(), 0);

        assert!(
            good_lp::IntoAffineExpression::linear_coefficients(&expr)
                .next()
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn add_variables_skips_tier_when_item_count_threshold_unreachable() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags_and_item_count(
                1000,
                2,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let expr = vars.add_item_participation_term(Expression::default(), 0);

        assert!(
            IntoAffineExpression::linear_coefficients(&expr)
                .next()
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn add_variables_correctly_filters_qualifying_tiers() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![
                make_tier_with_tags(
                    3000,
                    &["wine"],
                    &["cheese"],
                    ThresholdDiscount::PercentEachItem(Percentage::from(0.05)),
                ),
                make_tier_with_tags(
                    8000,
                    &["wine"],
                    &["cheese"],
                    ThresholdDiscount::PercentEachItem(Percentage::from(0.12)),
                ),
            ],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = CountingObserver::default();

        let _vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        // Only one tier qualifies: 1 tier-selector + 1 item var = 2 promotion variables
        assert_eq!(observer.promotion_variables, 2);

        // Only the item var contributes to the objective
        assert_eq!(observer.objective_terms, 1);

        Ok(())
    }

    #[test]
    fn participation_terms_contribute_variables_for_eligible_items() -> TestResult {
        let items = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(3000, GBP),
                StringTagCollection::from_strs(&["wine"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["cheese"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["bread"]),
            ),
        ];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                2000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        let expr1 = vars.add_item_participation_term(Expression::default(), 1);

        assert!(
            IntoAffineExpression::linear_coefficients(&expr1)
                .next()
                .is_some()
        );

        let expr0 = vars.add_item_participation_term(Expression::default(), 0);

        assert!(
            IntoAffineExpression::linear_coefficients(&expr0)
                .next()
                .is_some()
        );

        let expr2 = vars.add_item_participation_term(Expression::default(), 2);

        assert!(
            IntoAffineExpression::linear_coefficients(&expr2)
                .next()
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn is_item_participating_with_select_all_solution() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                3000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        assert!(vars.is_item_participating(&SelectAllSolution, 0));

        Ok(())
    }

    #[test]
    fn contribution_only_item_is_not_priced_by_promotion() -> TestResult {
        let items = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(3000, GBP),
                StringTagCollection::from_strs(&["wine"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["cheese"]),
            ),
        ];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                2000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        assert!(vars.is_item_participating(&SelectAllSolution, 0));
        assert!(vars.is_item_participating(&SelectAllSolution, 1));
        assert!(!vars.is_item_priced_by_promotion(&SelectAllSolution, 0));
        assert!(vars.is_item_priced_by_promotion(&SelectAllSolution, 1));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_with_select_all() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
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

        assert_eq!(discounts.get(&0), Some(&(1000, 900)));

        Ok(())
    }

    #[test]
    fn calculate_item_discounts_skips_unselected_items() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
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
    fn calculate_item_applications_shares_bundle_id() -> TestResult {
        let items = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(3000, GBP),
                StringTagCollection::from_strs(&["wine"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["cheese"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(800, GBP),
                StringTagCollection::from_strs(&["cheese"]),
            ),
        ];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                2000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
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

        assert_eq!(apps.len(), 3);

        let first_bundle = apps.first().map(|a| a.bundle_id);
        let second_bundle = apps.get(1).map(|a| a.bundle_id);

        assert_eq!(first_bundle, second_bundle);
        assert_eq!(first_bundle, Some(0));

        let item_indices: Vec<usize> = apps.iter().map(|a| a.item_idx).collect();

        assert_eq!(item_indices, vec![0, 1, 2]);
        assert_eq!(next_bundle_id, 1);

        Ok(())
    }

    #[test]
    fn items_matching_multiple_tiers_get_variables_per_tier() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(10000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![
                make_tier_with_tags(
                    3000,
                    &["wine"],
                    &["cheese"],
                    ThresholdDiscount::PercentEachItem(Percentage::from(0.05)),
                ),
                make_tier_with_tags(
                    5000,
                    &["wine"],
                    &["cheese"],
                    ThresholdDiscount::PercentEachItem(Percentage::from(0.12)),
                ),
            ],
            PromotionBudget::unlimited(),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = CountingObserver::default();

        let _vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        // 2 tier-selector vars + 2 item vars (one per tier) = 4
        assert_eq!(observer.promotion_variables, 4);

        // 2 item vars contribute to objective
        assert_eq!(observer.objective_terms, 2);

        Ok(())
    }

    #[test]
    fn add_variables_errors_on_discount_error() {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::AmountOffEachItem(Money::from_minor(50, iso::USD)),
            )],
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
    fn budget_constraints_are_emitted() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::with_both_limits(2, Money::from_minor(500, GBP)),
        );

        let pb = ProblemVariables::new();
        let cost = Expression::default();

        let mut state = ILPState::new(pb, cost);
        let mut observer = CountingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        // Tier-item link (1) + lower threshold spend (1) + application budget (1) + monetary budget (1) = 4
        assert_eq!(observer.promotion_constraints, 4);

        Ok(())
    }

    #[test]
    fn lower_item_count_threshold_constraint_is_emitted_when_configured() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(5000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags_and_item_count(
                1000,
                1,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        assert!(
            observer
                .promotion_constraints
                .iter()
                .any(|record| record.constraint_type == "lower threshold item count")
        );

        Ok(())
    }

    #[test]
    fn is_applicable_empty_items_even_when_discount_tags_are_empty() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &[],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn is_item_participating_requires_active_matching_variable() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1200, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                1000,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = NoopObserver;

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        assert!(!vars.is_item_participating(&SelectNoneSolution, 0));
        assert!(!vars.is_item_participating(&SelectAllSolution, 1));

        Ok(())
    }

    #[test]
    fn qualifying_tier_discount_mode_helpers_report_configured_modes() {
        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let tier_var = state.problem_variables_mut().add(variable().binary());

        let bundle_tier = QualifyingTier {
            lower_monetary_threshold_minor: Some(1000),
            lower_item_count_threshold: None,
            upper_monetary_threshold_minor: None,
            upper_item_count_threshold: None,
            tier_var,
            item_vars: SmallVec::new(),
            contribution_vars: SmallVec::new(),
            discount_vars: SmallVec::new(),
            discounted_minor_by_item: FxHashMap::default(),
            target_vars: SmallVec::new(),
            has_per_item_discount: false,
            amount_off_total_minor: Some(50),
            fixed_total_minor: None,
            percent_cheapest: None,
            fixed_cheapest_minor: None,
            cheapest_free: false,
        };

        assert!(bundle_tier.has_bundle_total_discount());
        assert!(!bundle_tier.has_per_item_discount());

        let per_item_tier = QualifyingTier {
            has_per_item_discount: true,
            amount_off_total_minor: None,
            ..bundle_tier
        };

        assert!(!per_item_tier.has_bundle_total_discount());
        assert!(per_item_tier.has_per_item_discount());
    }

    #[test]
    fn proportional_alloc_handles_rounding_and_zero_denominator() {
        assert_eq!(proportional_alloc(150, 200, 300), 100);
        assert_eq!(proportional_alloc(1, 3, 5), 1);
        assert_eq!(proportional_alloc(100, 50, 0), 0);
    }

    #[test]
    fn calculate_total_discounts_distributes_and_preserves_total() -> TestResult {
        let item_group = item_group_from_prices(&[200, 100]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let v0 = state.problem_variables_mut().add(variable().binary());
        let v1 = state.problem_variables_mut().add(variable().binary());

        let discount_vars = SmallVec::from_vec(vec![(0, v0), (1, v1)]);
        let solution = MapSolution::with(&[(v0, 1.0), (v1, 1.0)]);

        let discounts = calculate_total_discounts(&discount_vars, &solution, &item_group, &|t| {
            t.saturating_sub(60)
        })?;

        assert_eq!(discounts.get(&0), Some(&(200, 160)));
        assert_eq!(discounts.get(&1), Some(&(100, 80)));

        let final_total: i64 = discounts
            .values()
            .map(|(_, final_minor)| *final_minor)
            .sum();

        assert_eq!(final_total, 240);

        Ok(())
    }

    #[test]
    fn calculate_total_discounts_uses_remaining_for_last_item() -> TestResult {
        let item_group = item_group_from_prices(&[100, 100, 100]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let v0 = state.problem_variables_mut().add(variable().binary());
        let v1 = state.problem_variables_mut().add(variable().binary());
        let v2 = state.problem_variables_mut().add(variable().binary());

        let discount_vars = SmallVec::from_vec(vec![(0, v0), (1, v1), (2, v2)]);
        let solution = MapSolution::with(&[(v0, 1.0), (v1, 1.0), (v2, 1.0)]);

        let discounts =
            calculate_total_discounts(&discount_vars, &solution, &item_group, &|_| 100)?;

        assert_eq!(discounts.get(&0), Some(&(100, 33)));
        assert_eq!(discounts.get(&1), Some(&(100, 33)));
        assert_eq!(discounts.get(&2), Some(&(100, 34)));

        let final_total: i64 = discounts
            .values()
            .map(|(_, final_minor)| *final_minor)
            .sum();

        assert_eq!(final_total, 100);

        Ok(())
    }

    #[test]
    fn calculate_cheapest_discounts_applies_target_only() -> TestResult {
        let item_group = item_group_from_prices(&[250, 150]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let d0 = state.problem_variables_mut().add(variable().binary());
        let d1 = state.problem_variables_mut().add(variable().binary());
        let t0 = state.problem_variables_mut().add(variable().binary());
        let t1 = state.problem_variables_mut().add(variable().binary());

        let discount_vars = SmallVec::from_vec(vec![(0, d0), (1, d1)]);
        let target_vars = SmallVec::from_vec(vec![(0, t0), (1, t1)]);
        let solution = MapSolution::with(&[(d0, 1.0), (d1, 1.0), (t0, 0.0), (t1, 1.0)]);

        let discounts = calculate_cheapest_discounts(
            &discount_vars,
            &target_vars,
            &solution,
            &item_group,
            &|price| price.saturating_sub(40),
        )?;

        assert_eq!(discounts.get(&0), Some(&(250, 250)));
        assert_eq!(discounts.get(&1), Some(&(150, 110)));

        Ok(())
    }

    #[test]
    fn estimate_discounted_minor_for_budget_covers_modes() -> TestResult {
        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let tier_var = state.problem_variables_mut().add(variable().binary());
        let discount_var = state.problem_variables_mut().add(variable().binary());
        let other_var = state.problem_variables_mut().add(variable().binary());

        let per_item_tier = QualifyingTier {
            lower_monetary_threshold_minor: Some(1000),
            lower_item_count_threshold: None,
            upper_monetary_threshold_minor: None,
            upper_item_count_threshold: None,
            tier_var,
            item_vars: SmallVec::new(),
            contribution_vars: SmallVec::new(),
            discount_vars: SmallVec::from_vec(vec![(0, discount_var)]),
            discounted_minor_by_item: FxHashMap::from_iter([(0, 75)]),
            target_vars: SmallVec::new(),
            has_per_item_discount: true,
            amount_off_total_minor: None,
            fixed_total_minor: None,
            percent_cheapest: None,
            fixed_cheapest_minor: None,
            cheapest_free: false,
        };

        assert_eq!(
            estimate_discounted_minor_for_budget(&per_item_tier, 0, discount_var, 100)?,
            75
        );
        assert_eq!(
            estimate_discounted_minor_for_budget(&per_item_tier, 1, other_var, 120)?,
            120
        );
        assert_eq!(
            estimate_discounted_minor_for_budget(&per_item_tier, 0, other_var, 120)?,
            120
        );

        let fixed_cheapest_tier = QualifyingTier {
            has_per_item_discount: false,
            fixed_cheapest_minor: Some(-10),
            discounted_minor_by_item: FxHashMap::default(),
            ..per_item_tier
        };

        assert_eq!(
            estimate_discounted_minor_for_budget(&fixed_cheapest_tier, 0, discount_var, 200)?,
            0
        );

        Ok(())
    }

    #[test]
    fn build_target_vars_sorts_and_adds_negative_objective_terms() -> TestResult {
        let eligible = SmallVec::from_vec(vec![(2, 300), (0, 100), (1, 200)]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let targets = build_target_vars(
            &eligible,
            Some(Percentage::from(0.50)),
            None,
            false,
            PromotionKey::default(),
            &mut state,
            &mut observer,
        )?;

        let ordered_indices: Vec<usize> = targets.iter().map(|(idx, _)| *idx).collect();

        assert_eq!(ordered_indices, vec![0, 1, 2]);

        let coeffs: Vec<f64> = observer
            .objective_terms
            .iter()
            .map(|(_, coeff)| *coeff)
            .collect();

        assert_eq!(coeffs, vec![-50.0, -100.0, -150.0]);

        Ok(())
    }

    #[test]
    fn build_target_vars_reports_discounted_minor_values_for_targets() -> TestResult {
        let eligible = SmallVec::from_vec(vec![(2, 300), (0, 100), (1, 200)]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = PromotionVarCaptureObserver::default();

        let _targets = build_target_vars(
            &eligible,
            Some(Percentage::from(0.50)),
            None,
            false,
            PromotionKey::default(),
            &mut state,
            &mut observer,
        )?;

        let mut target_discounted: Vec<(usize, i64)> = observer
            .discounted_by_item
            .iter()
            .filter(|(_, _, metadata)| metadata.as_deref() == Some("target"))
            .map(|(idx, discounted, _)| (*idx, *discounted))
            .collect();

        target_discounted.sort_by_key(|(idx, _)| *idx);

        assert_eq!(target_discounted, vec![(0, 50), (1, 100), (2, 150)]);

        Ok(())
    }

    #[test]
    fn build_target_vars_state_objective_uses_negative_coefficients() -> TestResult {
        let eligible = SmallVec::from_vec(vec![(0, 100), (1, 200)]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let targets = build_target_vars(
            &eligible,
            Some(Percentage::from(0.50)),
            None,
            false,
            PromotionKey::default(),
            &mut state,
            &mut observer,
        )?;

        let (_pb, cost, _presence, _constraints) = state.into_parts_with_constraints();
        let solution = MapSolution::with(&[(targets[0].1, 1.0), (targets[1].1, 1.0)]);
        let objective_value = solution.eval(&cost);

        assert!((objective_value - -150.0).abs() < f64::EPSILON);

        Ok(())
    }

    #[test]
    fn add_variables_amount_off_total_uses_negative_tier_objective_term() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::AmountOffTotal(Money::from_minor(100, GBP)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let _vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        assert!(
            observer
                .objective_terms
                .iter()
                .any(|(_, coeff)| (*coeff - -100.0).abs() < f64::EPSILON)
        );

        Ok(())
    }

    #[test]
    fn add_variables_percent_cheapest_builds_target_variables() -> TestResult {
        let items = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, GBP),
                StringTagCollection::from_strs(&["wine", "cheese"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["wine", "cheese"]),
            ),
        ];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                100,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentCheapest(Percentage::from(0.25)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = CountingObserver::default();

        let _vars = promo.add_variables(&item_group, &mut state, &mut observer)?;

        // 1 tier var + 2 item vars + 2 target vars
        assert_eq!(observer.promotion_variables, 5);

        Ok(())
    }

    #[test]
    fn calculate_discounts_for_tier_percent_cheapest_subtracts_savings() -> TestResult {
        let item_group = item_group_from_prices(&[100, 200]);

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let tier_var = state.problem_variables_mut().add(variable().binary());

        let d0 = state.problem_variables_mut().add(variable().binary());
        let d1 = state.problem_variables_mut().add(variable().binary());
        let t0 = state.problem_variables_mut().add(variable().binary());
        let t1 = state.problem_variables_mut().add(variable().binary());

        let qt = QualifyingTier {
            lower_monetary_threshold_minor: Some(100),
            lower_item_count_threshold: None,
            upper_monetary_threshold_minor: None,
            upper_item_count_threshold: None,
            tier_var,
            item_vars: SmallVec::from_vec(vec![(0, d0), (1, d1)]),
            contribution_vars: SmallVec::new(),
            discount_vars: SmallVec::from_vec(vec![(0, d0), (1, d1)]),
            discounted_minor_by_item: FxHashMap::default(),
            target_vars: SmallVec::from_vec(vec![(0, t0), (1, t1)]),
            has_per_item_discount: false,
            amount_off_total_minor: None,
            fixed_total_minor: None,
            percent_cheapest: Some(Percentage::from(0.25)),
            fixed_cheapest_minor: None,
            cheapest_free: false,
        };

        let solution = MapSolution::with(&[(d0, 1.0), (d1, 1.0), (t0, 1.0), (t1, 0.0)]);
        let discounts = calculate_discounts_for_tier(&qt, &solution, &item_group)?;

        assert_eq!(discounts.get(&0), Some(&(100, 75)));
        assert_eq!(discounts.get(&1), Some(&(200, 200)));

        Ok(())
    }

    #[test]
    fn add_constraints_single_tier_does_not_emit_at_most_one_tier_constraint() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        let has_at_most_one = observer
            .promotion_constraints
            .iter()
            .any(|record| record.constraint_type == "at most one tier");

        assert!(!has_at_most_one);

        Ok(())
    }

    #[test]
    fn add_constraints_bundle_total_tier_activation_uses_subtraction() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::AmountOffTotal(Money::from_minor(100, GBP)),
            )],
            PromotionBudget::unlimited(),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<TieredThresholdPromotionVars>())
            .ok_or("Expected tiered vars")?;

        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        let tier_var = vars.qualifying_tiers[0].tier_var;
        let discount_var = vars.qualifying_tiers[0].discount_vars[0].1;
        let solution = MapSolution::with(&[(tier_var, 1.0), (discount_var, 1.0)]);

        let tier_activation = observer
            .promotion_constraints
            .iter()
            .find(|record| record.constraint_type == "tier activation")
            .ok_or("Expected tier activation constraint")?;

        let lhs = solution.eval(&tier_activation.expr);

        assert_relation_holds(lhs, &tier_activation.relation, tier_activation.rhs);
        assert!((lhs - 0.0).abs() < f64::EPSILON);

        Ok(())
    }

    #[test]
    fn add_cheapest_constraints_emit_expected_relations() {
        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());

        let tier_var = state.problem_variables_mut().add(variable().binary());
        let d0 = state.problem_variables_mut().add(variable().binary());
        let d1 = state.problem_variables_mut().add(variable().binary());
        let t0 = state.problem_variables_mut().add(variable().binary());
        let t1 = state.problem_variables_mut().add(variable().binary());

        let qt = QualifyingTier {
            lower_monetary_threshold_minor: Some(100),
            lower_item_count_threshold: None,
            upper_monetary_threshold_minor: None,
            upper_item_count_threshold: None,
            tier_var,
            item_vars: SmallVec::new(),
            contribution_vars: SmallVec::new(),
            discount_vars: SmallVec::from_vec(vec![(0, d0), (1, d1)]),
            discounted_minor_by_item: FxHashMap::default(),
            target_vars: SmallVec::from_vec(vec![(0, t0), (1, t1)]),
            has_per_item_discount: false,
            amount_off_total_minor: None,
            fixed_total_minor: None,
            percent_cheapest: Some(Percentage::from(0.25)),
            fixed_cheapest_minor: None,
            cheapest_free: false,
        };

        let mut observer = RecordingObserver::default();

        add_cheapest_constraints(&qt, PromotionKey::default(), &mut state, &mut observer);

        assert_eq!(observer.promotion_constraints.len(), 4);

        let satisfied =
            MapSolution::with(&[(tier_var, 1.0), (d0, 1.0), (d1, 0.0), (t0, 1.0), (t1, 0.0)]);

        for record in &observer.promotion_constraints {
            let lhs = satisfied.eval(&record.expr);

            assert_relation_holds(lhs, &record.relation, record.rhs);
        }

        let ordering_lhs = observer
            .promotion_constraints
            .iter()
            .find(|record| record.constraint_type == "cheapest ordering")
            .map_or(f64::NAN, |record| satisfied.eval(&record.expr));

        assert!((ordering_lhs - 1.0).abs() < f64::EPSILON);

        let (_pb, _cost, _presence, constraints) = state.into_parts_with_constraints();

        assert_state_constraints_hold(&constraints, &satisfied);
    }

    #[test]
    fn budget_monetary_constraint_uses_discount_amount_coefficient() -> TestResult {
        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(1000, GBP),
            StringTagCollection::from_strs(&["wine", "cheese"]),
        )];

        let item_group = item_group_from_items(items);

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![make_tier_with_tags(
                500,
                &["wine"],
                &["cheese"],
                ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
            )],
            PromotionBudget::with_monetary_limit(Money::from_minor(50, GBP)),
        );

        let mut state = ILPState::new(ProblemVariables::new(), Expression::default());
        let mut observer = RecordingObserver::default();

        let vars = promo.add_variables(&item_group, &mut state, &mut observer)?;
        let vars = ((vars.as_ref() as &dyn Any).downcast_ref::<TieredThresholdPromotionVars>())
            .ok_or("Expected tiered vars")?;

        vars.add_constraints(promo.key(), &item_group, &mut state, &mut observer)?;

        let selected_var: Variable = vars.qualifying_tiers[0].item_vars[0].1;
        let solution = MapSolution::with(&[(selected_var, 1.0)]);

        let monetary_lhs = observer
            .promotion_constraints
            .iter()
            .find(|record| record.constraint_type == "monetary value budget")
            .map(|record| solution.eval(&record.expr))
            .ok_or("Missing monetary budget constraint")?;

        assert!((monetary_lhs - 100.0).abs() < f64::EPSILON);

        Ok(())
    }
}
