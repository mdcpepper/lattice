//! ILP Observer

use std::any::Any;

use good_lp::{Expression, Variable};
use petgraph::graph::NodeIndex;

use crate::{graph::PromotionLayerKey, promotions::PromotionKey};

/// Observer trait for capturing ILP formulation as it's built.
///
/// This trait provides callbacks at key points during ILP construction,
/// allowing external observers to capture the complete mathematical formulation
/// (variables, objective, and constraints) without duplicating solver logic.
///
/// The observer pattern ensures a single source of truth: the solver remains
/// the only implementation of ILP construction, while observers passively
/// record what happens for rendering or analysis purposes.
///
/// # Zero Overhead
///
/// When no observer is provided (the default case), the solver uses a `NoopObserver`
/// and the observer calls are optimized away via monomorphization.
pub trait ILPObserver: Send + Sync + Any {
    /// Called when a presence variable is created for an item.
    ///
    /// Presence variables represent the baseline "buy at full price" option
    /// for each item in the basket.
    ///
    /// # Parameters
    ///
    /// - `item_idx`: Index of the item in the item group
    /// - `var`: The binary decision variable
    /// - `price_minor`: Full price in minor units (e.g., pence, cents)
    fn on_presence_variable(&mut self, item_idx: usize, var: Variable, price_minor: i64);

    /// Called when a promotion variable is created.
    ///
    /// Promotion variables represent the "apply discount" option for eligible items.
    ///
    /// # Parameters
    ///
    /// - `promotion_key`: Key identifying the promotion
    /// - `item_idx`: Index of the item in the item group
    /// - `var`: The binary decision variable
    /// - `discounted_price_minor`: Discounted price in minor units
    /// - `metadata`: Optional metadata about the variable (e.g., "participation", "discount")
    fn on_promotion_variable(
        &mut self,
        promotion_key: PromotionKey,
        item_idx: usize,
        var: Variable,
        discounted_price_minor: i64,
        metadata: Option<&str>,
    );

    /// Called when an auxiliary/internal variable is created.
    ///
    /// These variables are not tied directly to a specific item price, but are
    /// required to express the full ILP (e.g., DFA state/transition variables).
    ///
    /// # Parameters
    ///
    /// - `promotion_key`: Key identifying the promotion that owns the variable
    /// - `var`: The binary auxiliary variable
    /// - `role`: Human-readable role identifier (e.g., `"dfa_state"`, `"dfa_take"`)
    /// - `position`: Optional position index (e.g., DFA position)
    /// - `state`: Optional state index (e.g., DFA bundle state)
    fn on_auxiliary_variable(
        &mut self,
        _promotion_key: PromotionKey,
        _var: Variable,
        _role: &str,
        _position: Option<usize>,
        _state: Option<usize>,
    ) {
    }

    /// Called when a term is added to the objective function.
    ///
    /// # Parameters
    ///
    /// - `var`: The decision variable
    /// - `coefficient`: Coefficient in minor units (e.g., pence, cents)
    fn on_objective_term(&mut self, _var: Variable, _coefficient: f64) {}

    /// Called when an exclusivity constraint is added for an item.
    ///
    /// Exclusivity constraints ensure each item is purchased exactly once:
    /// either at full price OR via one promotion, never both.
    ///
    /// # Parameters
    ///
    /// - `item_idx`: Index of the item in the item group
    /// - `constraint_expr`: The constraint expression (sum of variables = 1)
    fn on_exclusivity_constraint(&mut self, item_idx: usize, constraint_expr: &Expression);

    /// Called when a promotion-specific constraint is added.
    ///
    /// These are constraints beyond exclusivity, such as minimum quantity
    /// requirements or DFA state transitions for bundled promotions.
    ///
    /// # Parameters
    ///
    /// - `promotion_key`: Key identifying the promotion
    /// - `constraint_type`: Human-readable constraint type (e.g., `"dfa_state_uniqueness"`, `"minimum_quantity"`)
    /// - `constraint_expr`: The left-hand side expression
    /// - `relation`: Relation operator ("=", "<=", ">=")
    /// - `rhs`: Right-hand side value
    fn on_promotion_constraint(
        &mut self,
        promotion_key: PromotionKey,
        constraint_type: &str,
        constraint_expr: &Expression,
        relation: &str,
        rhs: f64,
    );

    /// Called before solving a layer in graph evaluation.
    ///
    /// Allows multi-layer observers to track which layer is being solved.
    ///
    /// # Parameters
    ///
    /// - `layer_key`: Key identifying the layer
    /// - `node_idx`: Graph node index for the layer
    fn on_layer_begin(&mut self, _layer_key: PromotionLayerKey, _node_idx: NodeIndex) {}

    /// Called after solving a layer in graph evaluation.
    ///
    /// Allows multi-layer observers to finalize the current layer's formulation.
    fn on_layer_end(&mut self) {}
}

/// No-op observer for unobserved solves.
#[derive(Debug, Default)]
pub struct NoopObserver;

impl ILPObserver for NoopObserver {
    fn on_presence_variable(&mut self, _: usize, _: Variable, _: i64) {}

    fn on_promotion_variable(
        &mut self,
        _: PromotionKey,
        _: usize,
        _: Variable,
        _: i64,
        _: Option<&str>,
    ) {
    }

    fn on_exclusivity_constraint(&mut self, _: usize, _: &Expression) {}

    fn on_promotion_constraint(
        &mut self,
        _: PromotionKey,
        _: &str,
        _: &Expression,
        _: &str,
        _: f64,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, Variable};
    use petgraph::graph::NodeIndex;

    use super::*;

    struct MinimalObserver;

    impl ILPObserver for MinimalObserver {
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

        fn on_exclusivity_constraint(&mut self, _item_idx: usize, _constraint_expr: &Expression) {}

        fn on_promotion_constraint(
            &mut self,
            _promotion_key: PromotionKey,
            _constraint_type: &str,
            _constraint_expr: &Expression,
            _relation: &str,
            _rhs: f64,
        ) {
        }
    }

    #[test]
    fn default_layer_callbacks_are_callable() {
        let mut observer = MinimalObserver;
        let obs: &mut dyn ILPObserver = &mut observer;

        obs.on_layer_begin(PromotionLayerKey::default(), NodeIndex::new(0));
        obs.on_layer_end();
    }
}
