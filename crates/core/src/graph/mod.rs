//! Promotion Graph
//!
//! A DAG-based promotion layering system where each node is a "layer" containing
//! promotions that compete with each other (solved by a single ILP call).
//! Items flow between layers with updated prices, allowing discounts to stack
//! across layers.

use petgraph::{graph::NodeIndex, stable_graph::StableDiGraph};
use rustc_hash::FxHashMap;
use rusty_money::Money;
use smallvec::SmallVec;

use self::{
    edge::LayerEdge,
    evaluation::{TrackedItem, evaluate_node},
    node::LayerNode,
};
use crate::{items::groups::ItemGroup, promotions::Promotion, solvers::ilp::ILPObserver};

pub mod builder;
pub mod error;
pub mod result;

pub(crate) mod edge;
pub(crate) mod node;

pub use builder::PromotionGraphBuilder;
pub use error::GraphError;
pub use node::{OutputMode, PromotionLayerKey};
pub use result::LayeredSolverResult;

mod evaluation;

/// A validated promotion graph ready for evaluation.
///
/// Wraps a directed acyclic graph where each node is a promotion layer.
/// Items flow from the root node through the graph, accumulating discounts
/// as they pass through each layer.
#[derive(Debug)]
pub struct PromotionGraph<'a> {
    graph: StableDiGraph<LayerNode<'a>, LayerEdge>,
    root: NodeIndex,
}

impl<'a> PromotionGraph<'a> {
    /// Create a promotion graph from a builder.
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] if the graph fails validation.
    pub fn from_builder(builder: PromotionGraphBuilder<'a>) -> Result<Self, GraphError> {
        let (graph, root) = builder.build()?;

        Ok(Self { graph, root })
    }

    /// Create a single-layer graph equivalent to the flat solver.
    ///
    /// This is a convenience constructor that creates a graph with one
    /// `PassThrough` node containing all provided promotions.
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] if any promotion key is duplicated.
    pub fn single_layer(
        promotions: impl IntoIterator<Item = Promotion<'a>>,
    ) -> Result<Self, GraphError> {
        let mut builder = PromotionGraphBuilder::new();
        let root = builder.add_layer("Default", promotions, OutputMode::PassThrough)?;
        builder.set_root(root);

        Self::from_builder(builder)
    }

    /// Evaluate the promotion graph against an item group.
    ///
    /// Starting from the root, each layer solves its ILP formulation and routes
    /// items to successor layers with updated prices. Returns the accumulated
    /// result across all layers.
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] if any layer's solver fails or if item group
    /// construction fails.
    pub fn evaluate<'b>(
        &self,
        item_group: &ItemGroup<'b>,
    ) -> Result<LayeredSolverResult<'b>, GraphError> {
        self.evaluate_with_observer(item_group, None)
    }

    /// Evaluate the promotion graph with an observer.
    ///
    /// Same as [`evaluate()`](Self::evaluate), but passes an observer through to capture
    /// ILP formulations from all layers.
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] if any layer's solver fails or if item group
    /// construction fails.
    pub fn evaluate_with_observer<'b>(
        &self,
        item_group: &ItemGroup<'b>,
        observer: Option<&mut dyn ILPObserver>,
    ) -> Result<LayeredSolverResult<'b>, GraphError> {
        let currency = item_group.currency();

        // Create initial tracked items from the item group.
        let mut tracked_items: SmallVec<[TrackedItem<'b>; 8]> =
            SmallVec::with_capacity(item_group.len());

        for idx in 0..item_group.len() {
            let item = item_group.get_item(idx)?;
            tracked_items.push(TrackedItem {
                original_basket_idx: idx,
                item: item.clone(),
                applications: SmallVec::new(),
            });
        }

        let mut next_bundle_id: usize = 0;

        // Evaluate the graph starting from the root
        let final_items = evaluate_node(
            &self.graph,
            self.root,
            tracked_items,
            currency,
            &mut next_bundle_id,
            observer,
        )?;

        // Build the result from final tracked items
        let mut total = Money::from_minor(0, currency);

        let mut item_applications: FxHashMap<
            usize,
            SmallVec<[crate::promotions::applications::PromotionApplication<'b>; 3]>,
        > = FxHashMap::default();

        let mut full_price_items: SmallVec<[usize; 10]> = SmallVec::new();

        for tracked in &final_items {
            total = total.add(*tracked.item.price())?;

            if tracked.applications.is_empty() {
                full_price_items.push(tracked.original_basket_idx);
            } else {
                item_applications.insert(tracked.original_basket_idx, tracked.applications.clone());
            }
        }

        Ok(LayeredSolverResult {
            total,
            item_applications,
            full_price_items,
        })
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use rusty_money::iso::GBP;
    use smallvec::smallvec;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            Promotion, PromotionKey, budget::PromotionBudget, types::DirectDiscountPromotion,
        },
        solvers::{Solver, ilp::ILPSolver},
        tags::string::StringTagCollection,
    };

    use super::*;

    fn tagged_items<'a>() -> SmallVec<[Item<'a>; 10]> {
        smallvec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(1000, GBP),
                StringTagCollection::from_strs(&["food"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(500, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, GBP),
                StringTagCollection::from_strs(&["food", "snack"]),
            ),
        ]
    }

    fn make_promo(key: PromotionKey, tags: &[&str], pct: f64) -> Promotion<'static> {
        crate::promotions::promotion(DirectDiscountPromotion::new(
            key,
            StringTagCollection::from_strs(tags),
            SimpleDiscount::PercentageOff(Percentage::from(pct)),
            PromotionBudget::unlimited(),
        ))
    }

    #[test]
    fn single_layer_matches_flat_solver() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());

        let promo = make_promo(k1, &["food"], 0.20);

        // Flat solver
        let flat_result = ILPSolver::solve(std::slice::from_ref(&promo), &item_group)?;

        // Graph solver (single layer)
        let graph = PromotionGraph::single_layer([promo])?;
        let graph_result = graph.evaluate(&item_group)?;

        assert_eq!(
            flat_result.total.to_minor_units(),
            graph_result.total.to_minor_units(),
            "single layer graph should match flat solver total"
        );

        Ok(())
    }

    #[test]
    fn multi_layer_stacks_discounts() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());

        let food_promo = make_promo(k1, &["food"], 0.50); // 50% off food
        let everything_promo = make_promo(k2, &[], 0.10); // 10% off everything

        // Build a two-layer graph: food discount first, then 10% on everything
        let mut builder = PromotionGraphBuilder::new();

        let layer1 = builder.add_layer("Food Deals", [food_promo], OutputMode::PassThrough)?;
        let layer2 = builder.add_layer("Loyalty", [everything_promo], OutputMode::PassThrough)?;

        builder.set_root(layer1);
        builder.connect_pass_through(layer1, layer2)?;

        let graph = PromotionGraph::from_builder(builder)?;

        let result = graph.evaluate(&item_group)?;

        // Layer 1: food items (1000, 300) get 50% off -> (500, 150), drink (500) unchanged
        // Layer 2: everything gets 10% off -> (450, 135, 450)
        // Total = 450 + 450 + 135 = 1035
        assert_eq!(
            result.total.to_minor_units(),
            1035,
            "multi-layer should stack discounts"
        );

        Ok(())
    }

    #[test]
    fn split_routing_separates_promoted_and_unpromoted() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());
        let k3 = keys.insert(());

        let food_promo = make_promo(k1, &["food"], 0.50); // 50% off food
        let loyalty_promo = make_promo(k2, &[], 0.10); // 10% off (for promoted items)
        let coupon_promo = make_promo(k3, &[], 0.20); // 20% off (for unpromoted items)

        let mut builder = PromotionGraphBuilder::new();
        let root = builder.add_layer("Food Deals", [food_promo], OutputMode::Split)?;
        let promoted = builder.add_layer("Loyalty", [loyalty_promo], OutputMode::PassThrough)?;
        let unpromoted = builder.add_layer("Coupons", [coupon_promo], OutputMode::PassThrough)?;

        builder.set_root(root);
        builder.connect_split(root, promoted, unpromoted)?;

        let graph = PromotionGraph::from_builder(builder)?;

        let result = graph.evaluate(&item_group)?;

        // Layer 1 (Food Deals): items 0 (food, 1000) and 2 (food+snack, 300) get 50% off
        //   promoted: item 0 -> 500, item 2 -> 150
        //   unpromoted: item 1 (drink, 500) stays 500
        // Promoted path (Loyalty 10%): 500 -> 450, 150 -> 135
        // Unpromoted path (Coupons 20%): 500 -> 400
        // Total = 450 + 135 + 400 = 985
        assert_eq!(
            result.total.to_minor_units(),
            985,
            "split routing should apply different discounts to promoted vs unpromoted"
        );

        // Items 0 and 2 should have 2 applications each (food deal + loyalty)
        let apps_0 = result.item_applications.get(&0);

        assert!(apps_0.is_some(), "item 0 should have applications");
        assert_eq!(
            apps_0.map_or(0, SmallVec::len),
            2,
            "item 0 should have 2 applications"
        );

        let apps_2 = result.item_applications.get(&2);

        assert!(apps_2.is_some(), "item 2 should have applications");
        assert_eq!(
            apps_2.map_or(0, SmallVec::len),
            2,
            "item 2 should have 2 applications"
        );

        // Item 1 should have 1 application (coupon only)
        let apps_1 = result.item_applications.get(&1);

        assert!(apps_1.is_some(), "item 1 should have applications");
        assert_eq!(
            apps_1.map_or(0, SmallVec::len),
            1,
            "item 1 should have 1 application"
        );

        // No full price items
        assert!(
            result.full_price_items.is_empty(),
            "all items should have promotions applied"
        );

        Ok(())
    }

    #[test]
    fn split_routing_uses_prior_discounts() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());
        let k3 = keys.insert(());

        let food_promo = make_promo(k1, &["food"], 0.50); // 50% off food
        let loyalty_promo = make_promo(k2, &[], 0.10); // 10% off (for promoted items)
        let coupon_promo = make_promo(k3, &[], 0.20); // 20% off (for unpromoted items)

        let mut builder = PromotionGraphBuilder::new();
        let root = builder.add_layer("Food Deals", [food_promo], OutputMode::PassThrough)?;
        let router = builder.add_layer(
            "Router",
            std::iter::empty::<Promotion<'static>>(),
            OutputMode::Split,
        )?;
        let promoted = builder.add_layer("Loyalty", [loyalty_promo], OutputMode::PassThrough)?;
        let unpromoted = builder.add_layer("Coupons", [coupon_promo], OutputMode::PassThrough)?;

        builder.set_root(root);
        builder.connect_pass_through(root, router)?;
        builder.connect_split(router, promoted, unpromoted)?;

        let graph = PromotionGraph::from_builder(builder)?;

        let result = graph.evaluate(&item_group)?;

        // Layer 1 (Food Deals): items 0 (food, 1000) and 2 (food+snack, 300) get 50% off
        // Router (Split): uses prior discounts to route to Loyalty/Coupons
        // Promoted path (Loyalty 10%): 500 -> 450, 150 -> 135
        // Unpromoted path (Coupons 20%): 500 -> 400
        // Total = 450 + 135 + 400 = 985
        assert_eq!(
            result.total.to_minor_units(),
            985,
            "split routing should consider prior discounts"
        );

        Ok(())
    }

    #[test]
    fn empty_item_group_returns_zero_total() -> TestResult {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let graph = PromotionGraph::single_layer(std::iter::empty())?;
        let result = graph.evaluate(&item_group)?;

        assert_eq!(
            result.total.to_minor_units(),
            0,
            "empty group should be zero"
        );
        assert!(
            result.item_applications.is_empty(),
            "no applications expected"
        );
        assert!(
            result.full_price_items.is_empty(),
            "no full price items expected"
        );

        Ok(())
    }

    #[test]
    fn no_promotions_yields_full_price() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let graph = PromotionGraph::single_layer(std::iter::empty())?;
        let result = graph.evaluate(&item_group)?;

        assert_eq!(
            result.total.to_minor_units(),
            1800,
            "no promotions should yield full price total"
        );
        assert_eq!(
            result.full_price_items.len(),
            3,
            "all items should be full price"
        );

        Ok(())
    }

    #[test]
    fn bundle_ids_are_globally_unique_across_layers() -> TestResult {
        let items = tagged_items();
        let item_group = ItemGroup::new(items, GBP);

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());

        let food_promo = make_promo(k1, &["food"], 0.50);
        let all_promo = make_promo(k2, &[], 0.10);

        let mut builder = PromotionGraphBuilder::new();
        let layer1 = builder.add_layer("Food", [food_promo], OutputMode::PassThrough)?;
        let layer2 = builder.add_layer("All", [all_promo], OutputMode::PassThrough)?;

        builder.set_root(layer1);
        builder.connect_pass_through(layer1, layer2)?;

        let graph = PromotionGraph::from_builder(builder)?;

        let result = graph.evaluate(&item_group)?;

        // Collect all bundle IDs
        let mut all_bundle_ids: Vec<usize> = Vec::new();
        for apps in result.item_applications.values() {
            for app in apps {
                all_bundle_ids.push(app.bundle_id);
            }
        }

        // Layer 1 assigns bundles for 2 food items, Layer 2 assigns for 3 items
        // All bundle IDs should be unique across layers
        let unique_count = {
            let mut unique = all_bundle_ids.clone();
            unique.sort_unstable();
            unique.dedup();
            unique.len()
        };

        // Each application gets its own bundle ID for DirectDiscount
        assert_eq!(
            all_bundle_ids.len(),
            unique_count,
            "bundle IDs should be globally unique across layers"
        );

        Ok(())
    }
}
