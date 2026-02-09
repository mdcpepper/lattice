//! DFS graph evaluation engine.

use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;
use rusty_money::{Money, iso::Currency};
use smallvec::SmallVec;

use crate::{
    graph::{
        edge::LayerEdge,
        error::GraphError,
        node::{LayerNode, OutputMode},
    },
    items::{Item, groups::ItemGroup},
    promotions::applications::PromotionApplication,
    solvers::{
        Solver,
        ilp::{ILPSolver, observer::ILPObserver},
    },
};

type TrackedItems<'b> = SmallVec<[TrackedItem<'b>; 8]>;

/// An item flowing through the graph, carrying provenance information.
#[derive(Debug, Clone)]
pub(super) struct TrackedItem<'b> {
    /// Index of this item in the original basket/item group
    pub original_basket_idx: usize,

    /// The item with its current (possibly discounted) price
    pub item: Item<'b>,

    /// Promotion applications accumulated across layers
    pub applications: SmallVec<[PromotionApplication<'b>; 3]>,
}

/// Evaluate a single node in the promotion graph.
///
/// Solves the ILP for the node's promotions, then routes items to successors
/// based on the node's output mode.
///
/// # Errors
///
/// Returns a [`GraphError`] if the solver fails or if item group construction fails.
pub fn evaluate_node<'b>(
    graph: &StableDiGraph<LayerNode<'_>, LayerEdge>,
    node_idx: NodeIndex,
    tracked_items: TrackedItems<'b>,
    currency: &'b Currency,
    next_bundle_id: &mut usize,
    mut observer: Option<&mut dyn ILPObserver>,
) -> Result<TrackedItems<'b>, GraphError> {
    if tracked_items.is_empty() {
        return Ok(TrackedItems::new());
    }

    let Some(node) = graph.node_weight(node_idx) else {
        return Ok(tracked_items);
    };

    // If this layer has no promotions, skip the solve and just route items through.
    // This avoids pointless ILP solver invocations for pure routing layers.
    if node.promotions.is_empty() {
        return route_to_successors(
            graph,
            node_idx,
            node.output_mode,
            tracked_items,
            currency,
            next_bundle_id,
            observer,
        );
    }

    // Build a temporary ItemGroup from the tracked items' current prices
    let temp_items: SmallVec<[Item<'b, _>; 10]> =
        tracked_items.iter().map(|ti| ti.item.clone()).collect();

    let temp_group = ItemGroup::new(temp_items, currency);

    // Notify observer of layer entry
    if let Some(obs) = observer.as_deref_mut() {
        obs.on_layer_begin(node.key, node_idx);
    }

    // Solve the ILP for this layer.
    let applications = solve_layer(node, &temp_group, observer.as_deref_mut())?;

    // Notify observer of layer completion
    if let Some(obs) = observer.as_deref_mut() {
        obs.on_layer_end();
    }

    // Update tracked items with the solver results
    let mut updated_items = tracked_items;

    let bundle_id_offset = *next_bundle_id;

    let mut max_bundle: Option<usize> = None;

    for app in applications {
        max_bundle = Some(max_bundle.map_or(app.bundle_id, |max| max.max(app.bundle_id)));
        let local_idx = app.item_idx;
        let final_price_minor = app.final_price.to_minor_units();

        let Some(tracked) = updated_items.get_mut(local_idx) else {
            continue;
        };

        // Update item price to the discounted price
        tracked.item = Item::with_tags(
            tracked.item.product(),
            Money::from_minor(final_price_minor, currency),
            tracked.item.tags().clone(),
        );

        // Record the application with remapped indices
        tracked.applications.push(PromotionApplication {
            promotion_key: app.promotion_key,
            item_idx: tracked.original_basket_idx,
            bundle_id: app.bundle_id.saturating_add(bundle_id_offset),
            original_price: app.original_price,
            final_price: app.final_price,
        });
    }

    // Advance next_bundle_id past all bundles used in this layer
    if let Some(max) = max_bundle {
        *next_bundle_id = bundle_id_offset.saturating_add(max).saturating_add(1);
    }

    // Route items to successors based on output mode
    route_to_successors(
        graph,
        node_idx,
        node.output_mode,
        updated_items,
        currency,
        next_bundle_id,
        observer,
    )
}

/// Solve the ILP for a layer.
fn solve_layer<'b>(
    node: &LayerNode<'_>,
    temp_group: &ItemGroup<'b>,
    observer: Option<&mut dyn ILPObserver>,
) -> Result<SmallVec<[PromotionApplication<'b>; 10]>, GraphError> {
    let result = match observer {
        Some(obs) => ILPSolver::solve_with_observer(&node.promotions, temp_group, obs),
        None => ILPSolver::solve(&node.promotions, temp_group),
    }
    .map_err(|source| GraphError::Solver {
        layer_key: node.key,
        source,
    })?;

    Ok(result.promotion_applications)
}

/// Route items to successor nodes based on output mode.
fn route_to_successors<'b>(
    graph: &StableDiGraph<LayerNode<'_>, LayerEdge>,
    node_idx: NodeIndex,
    output_mode: OutputMode,
    updated_items: TrackedItems<'b>,
    currency: &'b Currency,
    next_bundle_id: &mut usize,
    mut observer: Option<&mut dyn ILPObserver>,
) -> Result<TrackedItems<'b>, GraphError> {
    let edges: SmallVec<[(NodeIndex, LayerEdge); 2]> = graph
        .edges(node_idx)
        .map(|e| (e.target(), *e.weight()))
        .collect();

    match output_mode {
        OutputMode::PassThrough => {
            let successor = edges.iter().find(|(_, w)| *w == LayerEdge::All);

            match successor {
                Some((target, _)) => evaluate_node(
                    graph,
                    *target,
                    updated_items,
                    currency,
                    next_bundle_id,
                    observer.as_deref_mut(),
                ),
                None => Ok(updated_items),
            }
        }
        OutputMode::Split => {
            let mut promoted_items: TrackedItems<'b> = TrackedItems::new();
            let mut unpromoted_items: TrackedItems<'b> = TrackedItems::new();

            for item in updated_items {
                let was_discounted = !item.applications.is_empty();

                if was_discounted {
                    promoted_items.push(item);
                } else {
                    unpromoted_items.push(item);
                }
            }

            let promoted_target = edges
                .iter()
                .find(|(_, w)| *w == LayerEdge::Participating)
                .map(|(t, _)| *t);

            let unpromoted_target = edges
                .iter()
                .find(|(_, w)| *w == LayerEdge::NonParticipating)
                .map(|(t, _)| *t);

            let mut final_items: TrackedItems<'b> = TrackedItems::new();

            if let Some(target) = promoted_target
                && !promoted_items.is_empty()
            {
                let result_items = evaluate_node(
                    graph,
                    target,
                    promoted_items,
                    currency,
                    next_bundle_id,
                    observer.as_deref_mut(),
                )?;
                final_items.extend(result_items);
            } else {
                final_items.extend(promoted_items);
            }

            if let Some(target) = unpromoted_target
                && !unpromoted_items.is_empty()
            {
                let result_items = evaluate_node(
                    graph,
                    target,
                    unpromoted_items,
                    currency,
                    next_bundle_id,
                    observer,
                )?;
                final_items.extend(result_items);
            } else {
                final_items.extend(unpromoted_items);
            }

            Ok(final_items)
        }
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, Variable};
    use petgraph::stable_graph::StableDiGraph;
    use rusty_money::{Money, iso::GBP};
    use smallvec::SmallVec;

    use crate::{
        discounts::SimpleDiscount,
        graph::{
            edge::LayerEdge,
            node::{LayerNode, PromotionLayerKey},
        },
        items::Item,
        products::ProductKey,
        promotions::{
            Promotion, PromotionKey, budget::PromotionBudget, promotion,
            qualification::Qualification, types::DirectDiscountPromotion,
        },
        solvers::ilp::observer::ILPObserver,
    };

    use super::*;

    #[derive(Default)]
    struct CountingObserver {
        layer_begin_calls: usize,
        layer_end_calls: usize,
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

        fn on_layer_begin(&mut self, _layer_key: PromotionLayerKey, _node_idx: NodeIndex) {
            self.layer_begin_calls += 1;
        }

        fn on_layer_end(&mut self) {
            self.layer_end_calls += 1;
        }
    }

    fn tracked_item(price_minor: i64) -> TrackedItem<'static> {
        TrackedItem {
            original_basket_idx: 0,
            item: Item::new(ProductKey::default(), Money::from_minor(price_minor, GBP)),
            applications: SmallVec::new(),
        }
    }

    fn direct_discount_promotion() -> Promotion<'static> {
        promotion(DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ))
    }

    #[test]
    fn evaluate_node_returns_original_items_when_node_missing() {
        let graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();
        let items: TrackedItems<'static> = SmallVec::from_vec(vec![tracked_item(100)]);

        let mut next_bundle_id = 0;

        let result = evaluate_node(
            &graph,
            NodeIndex::new(999),
            items,
            GBP,
            &mut next_bundle_id,
            None,
        )
        .expect("evaluation should succeed");

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn evaluate_node_notifies_observer_for_non_empty_promotion_layer() {
        let mut graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();
        let expected_layer_key = PromotionLayerKey::default();

        let node = graph.add_node(LayerNode {
            key: expected_layer_key,
            promotions: SmallVec::from_vec(vec![direct_discount_promotion()]),
            output_mode: OutputMode::PassThrough,
        });

        let mut observer = CountingObserver::default();

        let mut next_bundle_id = 0;

        let _ = evaluate_node(
            &graph,
            node,
            SmallVec::from_vec(vec![tracked_item(100)]),
            GBP,
            &mut next_bundle_id,
            Some(&mut observer),
        )
        .expect("evaluation should succeed");

        assert_eq!(observer.layer_begin_calls, 1);
        assert_eq!(observer.layer_end_calls, 1);
    }

    #[test]
    fn evaluate_node_wraps_solver_errors_with_layer_key() {
        let mut graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();
        let layer_key = PromotionLayerKey::default();

        let node = graph.add_node(LayerNode {
            key: layer_key,
            promotions: SmallVec::from_vec(vec![direct_discount_promotion()]),
            output_mode: OutputMode::PassThrough,
        });

        let mut next_bundle_id = 0;

        let err = evaluate_node(
            &graph,
            node,
            SmallVec::from_vec(vec![tracked_item(9_007_199_254_740_993)]),
            GBP,
            &mut next_bundle_id,
            None,
        )
        .expect_err("expected solver error");

        match err {
            GraphError::Solver {
                layer_key: err_layer_key,
                ..
            } => assert_eq!(err_layer_key, layer_key),
            other => panic!("expected GraphError::Solver, got {other:?}"),
        }
    }

    #[test]
    fn route_pass_through_without_successor_returns_items() {
        let mut graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();

        let node = graph.add_node(LayerNode {
            key: PromotionLayerKey::default(),
            promotions: SmallVec::new(),
            output_mode: OutputMode::PassThrough,
        });

        let mut next_bundle_id = 0;

        let result = route_to_successors(
            &graph,
            node,
            OutputMode::PassThrough,
            SmallVec::from_vec(vec![tracked_item(100)]),
            GBP,
            &mut next_bundle_id,
            None,
        )
        .expect("routing should succeed");

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn route_split_without_targets_keeps_items_in_output() {
        let mut graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();

        let node = graph.add_node(LayerNode {
            key: PromotionLayerKey::default(),
            promotions: SmallVec::new(),
            output_mode: OutputMode::Split,
        });

        let mut discounted = tracked_item(100);

        discounted
            .applications
            .push(crate::promotions::applications::PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 0,
                bundle_id: 0,
                original_price: Money::from_minor(100, GBP),
                final_price: Money::from_minor(90, GBP),
            });

        let mut next_bundle_id = 0;

        let result = route_to_successors(
            &graph,
            node,
            OutputMode::Split,
            SmallVec::from_vec(vec![discounted, tracked_item(200)]),
            GBP,
            &mut next_bundle_id,
            None,
        )
        .expect("routing should succeed");

        assert_eq!(result.len(), 2);
    }
}
