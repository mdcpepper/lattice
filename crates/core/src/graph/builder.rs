//! Builder for constructing validated promotion graphs.

use std::collections::hash_map::RandomState;

use petgraph::{
    algo::{is_cyclic_directed, simple_paths::all_simple_paths},
    graph::NodeIndex,
    stable_graph::StableDiGraph,
    visit::Dfs,
};
use rustc_hash::FxHashSet;
use slotmap::SlotMap;
use smallvec::SmallVec;

use crate::{
    graph::{
        edge::LayerEdge,
        error::GraphError,
        node::{LayerNode, OutputMode, PromotionLayerKey},
    },
    promotions::Promotion,
};

/// Builder for constructing a validated [`super::PromotionGraph`].
///
/// Ensures the graph satisfies all structural invariants before producing
/// a `PromotionGraph`.
#[derive(Debug)]
pub struct PromotionGraphBuilder<'a> {
    graph: StableDiGraph<LayerNode<'a>, LayerEdge>,
    root: Option<NodeIndex>,
    layer_keys: SlotMap<PromotionLayerKey, ()>,
}

impl<'a> PromotionGraphBuilder<'a> {
    /// Create a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            root: None,
            layer_keys: SlotMap::with_key(),
        }
    }

    /// Add a layer node to the graph.
    ///
    /// Promotion key uniqueness is validated per-path during graph finalization.
    ///
    /// # Errors
    ///
    /// Currently this method does not return errors, but returns `Result`
    /// for future extensibility.
    pub fn add_layer(
        &mut self,
        _label: impl Into<String>,
        promotions: impl IntoIterator<Item = Promotion<'a>>,
        output_mode: OutputMode,
    ) -> Result<NodeIndex, GraphError> {
        let layer_key = self.layer_keys.insert(());
        self.add_layer_with_key(layer_key, promotions, output_mode)
    }

    /// Add a layer node to the graph with an explicit layer key.
    ///
    /// Promotion key uniqueness is validated per-path during graph finalization.
    ///
    /// # Errors
    ///
    /// Currently this method does not return errors, but returns `Result`
    /// for future extensibility.
    pub fn add_layer_with_key(
        &mut self,
        key: PromotionLayerKey,
        promotions: impl IntoIterator<Item = Promotion<'a>>,
        output_mode: OutputMode,
    ) -> Result<NodeIndex, GraphError> {
        let promotions: SmallVec<[Promotion<'a>; 5]> = promotions.into_iter().collect();

        let node = LayerNode {
            key,
            promotions,
            output_mode,
        };

        Ok(self.graph.add_node(node))
    }

    /// Set the root node of the graph (evaluation starts here).
    pub fn set_root(&mut self, node: NodeIndex) {
        self.root = Some(node);
    }

    /// Connect a `PassThrough` node to its single successor via an `All` edge.
    ///
    /// # Errors
    ///
    /// Returns an error if the source node already has an outgoing edge.
    pub fn connect_pass_through(
        &mut self,
        from: NodeIndex,
        to: NodeIndex,
    ) -> Result<(), GraphError> {
        let existing_count = self.graph.edges(from).count();

        if existing_count > 0 {
            return Err(GraphError::PassThroughMultipleSuccessors(from.index()));
        }

        self.graph.add_edge(from, to, LayerEdge::All);

        Ok(())
    }

    /// Connect a `Split` node to its two successors: participating items flow to
    /// `participating_to`, non-participating items flow to `non_participating_to`.
    ///
    /// # Errors
    ///
    /// Returns an error if the source node already has outgoing edges.
    pub fn connect_split(
        &mut self,
        from: NodeIndex,
        participating_to: NodeIndex,
        non_participating_to: NodeIndex,
    ) -> Result<(), GraphError> {
        let existing_count = self.graph.edges(from).count();

        if existing_count > 0 {
            return Err(GraphError::SplitSuccessorMismatch);
        }

        self.graph
            .add_edge(from, participating_to, LayerEdge::Participating);

        self.graph
            .add_edge(from, non_participating_to, LayerEdge::NonParticipating);

        Ok(())
    }

    /// Connect a `Split` node with only a participating items target.
    /// Non-participating items will stop at this layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the source node already has outgoing edges.
    pub fn connect_split_participating_only(
        &mut self,
        from: NodeIndex,
        participating_to: NodeIndex,
    ) -> Result<(), GraphError> {
        let existing_count = self.graph.edges(from).count();

        if existing_count > 0 {
            return Err(GraphError::SplitSuccessorMismatch);
        }

        self.graph
            .add_edge(from, participating_to, LayerEdge::Participating);

        Ok(())
    }

    /// Connect a `Split` node with only an non-participating items target.
    /// Participating items will stop at this layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the source node already has outgoing edges.
    pub fn connect_split_non_participating_only(
        &mut self,
        from: NodeIndex,
        non_participating_to: NodeIndex,
    ) -> Result<(), GraphError> {
        let existing_count = self.graph.edges(from).count();

        if existing_count > 0 {
            return Err(GraphError::SplitSuccessorMismatch);
        }

        self.graph
            .add_edge(from, non_participating_to, LayerEdge::NonParticipating);

        Ok(())
    }

    /// Build and validate the promotion graph.
    ///
    /// # Validation rules
    ///
    /// 1. A root node must be set
    /// 2. The graph must not contain cycles
    /// 3. All nodes must be reachable from the root
    /// 4. `PassThrough` nodes must have 0 or 1 outgoing `All` edges
    /// 5. `Split` nodes must have 1 or 2 edges: at least one of `Participating` or `NonParticipating`
    /// 6. No promotion key appears more than once in any single root-to-leaf path
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] if any validation rule is violated.
    pub(crate) fn build(
        self,
    ) -> Result<(StableDiGraph<LayerNode<'a>, LayerEdge>, NodeIndex), GraphError> {
        // 1. Root must be set
        let root = self.root.ok_or(GraphError::NoRoot)?;

        // 2. No cycles
        if is_cyclic_directed(&self.graph) {
            return Err(GraphError::CycleDetected);
        }

        // 3. All nodes reachable from root
        let mut dfs = Dfs::new(&self.graph, root);
        let mut reachable_count = 0_usize;

        while dfs.next(&self.graph).is_some() {
            reachable_count = reachable_count.saturating_add(1);
        }

        if reachable_count != self.graph.node_count() {
            return Err(GraphError::UnreachableNode);
        }

        // 4 & 5. Validate output mode vs edges for each node
        for node_idx in self.graph.node_indices() {
            let Some(node) = self.graph.node_weight(node_idx) else {
                continue;
            };

            let edges: SmallVec<[&LayerEdge; 3]> =
                self.graph.edges(node_idx).map(|e| e.weight()).collect();

            match node.output_mode {
                OutputMode::PassThrough => {
                    if edges.len() > 1 {
                        return Err(GraphError::PassThroughMultipleSuccessors(node_idx.index()));
                    }

                    if edges.len() == 1 && edges.first() != Some(&&LayerEdge::All) {
                        return Err(GraphError::PassThroughMultipleSuccessors(node_idx.index()));
                    }
                }
                OutputMode::Split => {
                    let has_participating = edges.iter().any(|e| **e == LayerEdge::Participating);
                    let has_non_participating =
                        edges.iter().any(|e| **e == LayerEdge::NonParticipating);

                    // Split nodes must have 1-2 edges: at least one of Participating or Non-Participating
                    if edges.is_empty()
                        || edges.len() > 2
                        || (!has_participating && !has_non_participating)
                    {
                        return Err(GraphError::SplitSuccessorMismatch);
                    }

                    // Ensure only valid edge types
                    for edge in &edges {
                        if **edge != LayerEdge::Participating
                            && **edge != LayerEdge::NonParticipating
                        {
                            return Err(GraphError::SplitSuccessorMismatch);
                        }
                    }
                }
            }
        }

        // 6. Per-path promotion uniqueness
        validate_path_promotion_uniqueness(&self.graph, root)?;

        Ok((self.graph, root))
    }
}

/// Validate that no promotion key appears more than once in any single path.
fn validate_path_promotion_uniqueness(
    graph: &StableDiGraph<LayerNode<'_>, LayerEdge>,
    root: NodeIndex,
) -> Result<(), GraphError> {
    // Find all leaf nodes (no outgoing edges)
    let leaf_nodes: SmallVec<[NodeIndex; 10]> = graph
        .node_indices()
        .filter(|&node_idx| graph.edges(node_idx).count() == 0)
        .collect();

    // Special case: single-node graph (root is leaf)
    if leaf_nodes.is_empty() || (leaf_nodes.len() == 1 && leaf_nodes.first().copied() == Some(root))
    {
        return validate_single_node(graph, root);
    }

    // For each path from root to leaf, check promotion uniqueness
    for leaf in &leaf_nodes {
        let paths = all_simple_paths::<Vec<NodeIndex>, _, RandomState>(graph, root, *leaf, 0, None);

        for path in paths {
            validate_path(&path, graph)?;
        }
    }

    Ok(())
}

/// Validate promotion uniqueness for a single path.
fn validate_path(
    path: &[NodeIndex],
    graph: &StableDiGraph<LayerNode<'_>, LayerEdge>,
) -> Result<(), GraphError> {
    let mut seen_in_path = FxHashSet::default();
    let mut path_keys: SmallVec<[PromotionLayerKey; 5]> = SmallVec::new();

    for &node_idx in path {
        let Some(node) = graph.node_weight(node_idx) else {
            continue;
        };

        path_keys.push(node.key);

        for promo in &node.promotions {
            let key = promo.key();

            if !seen_in_path.insert(key) {
                return Err(GraphError::DuplicatePromotionInPath {
                    key,
                    path: path_keys.into_vec(),
                });
            }
        }
    }

    Ok(())
}

/// Validate a single-node graph (root is leaf).
fn validate_single_node(
    graph: &StableDiGraph<LayerNode<'_>, LayerEdge>,
    root: NodeIndex,
) -> Result<(), GraphError> {
    let Some(node) = graph.node_weight(root) else {
        return Ok(());
    };

    let mut seen = FxHashSet::default();

    for promo in &node.promotions {
        let key = promo.key();

        if !seen.insert(key) {
            return Err(GraphError::DuplicatePromotionInPath {
                key,
                path: vec![node.key],
            });
        }
    }

    Ok(())
}

impl Default for PromotionGraphBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso::GBP};

    use crate::{
        discounts::SimpleDiscount,
        promotions::{
            Promotion, PromotionKey, budget::PromotionBudget, types::DirectDiscountPromotion,
        },
        tags::string::StringTagCollection,
    };

    use super::*;

    fn test_promotion(key: PromotionKey) -> Promotion<'static> {
        crate::promotions::promotion(DirectDiscountPromotion::new(
            key,
            StringTagCollection::from_strs(&["a"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ))
    }

    #[test]
    fn build_single_node_graph() {
        let mut builder = PromotionGraphBuilder::new();
        let key = PromotionKey::default();
        let node = builder
            .add_layer("Store", [test_promotion(key)], OutputMode::PassThrough)
            .ok();

        assert!(node.is_some(), "add_layer should succeed");

        let node = node.unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(node);

        let result = builder.build();

        assert!(result.is_ok(), "build should succeed for single node graph");
    }

    #[test]
    fn build_rejects_no_root() {
        let builder = PromotionGraphBuilder::new();
        let result = builder.build();

        assert!(
            matches!(result, Err(GraphError::NoRoot)),
            "build should fail without root"
        );
    }

    #[test]
    fn allow_same_promotion_in_different_paths() {
        let mut builder = PromotionGraphBuilder::new();
        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let shared_key = keys.insert(());
        let k1 = keys.insert(());
        let k2 = keys.insert(());

        // Root splits into two paths, and each path uses the same promotion (shared_key)
        let root = builder
            .add_layer("Root", [test_promotion(k1)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let promo_path = builder
            .add_layer(
                "PromoPath",
                [test_promotion(k2), test_promotion(shared_key)],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let no_promo_path = builder
            .add_layer(
                "NoPromoPath",
                [test_promotion(shared_key)],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);
        assert!(
            builder
                .connect_split(root, promo_path, no_promo_path)
                .is_ok()
        );

        let result = builder.build();

        // Should succeed - same promotion in different branches is OK
        assert!(
            result.is_ok(),
            "same promotion in different paths should be allowed, got: {:?}",
            result.as_ref().err()
        );
    }

    #[test]
    fn reject_duplicate_in_same_path() {
        let mut builder = PromotionGraphBuilder::new();
        let key = PromotionKey::default();

        let layer1 = builder
            .add_layer("Layer1", [test_promotion(key)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let layer2 = builder
            .add_layer("Layer2", [test_promotion(key)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(layer1);
        assert!(builder.connect_pass_through(layer1, layer2).is_ok());

        let result = builder.build();

        assert!(
            matches!(result, Err(GraphError::DuplicatePromotionInPath { .. })),
            "duplicate in same path should fail"
        );
    }

    #[test]
    fn reject_duplicate_in_single_node() {
        let mut builder = PromotionGraphBuilder::new();
        let key = PromotionKey::default();

        // A single node with the same promotion twice should fail
        let node = builder
            .add_layer(
                "Store",
                [test_promotion(key), test_promotion(key)],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(node);

        let result = builder.build();

        assert!(
            matches!(result, Err(GraphError::DuplicatePromotionInPath { .. })),
            "duplicate in single node should fail"
        );
    }

    #[test]
    fn build_rejects_unreachable_nodes() {
        let mut builder = PromotionGraphBuilder::new();

        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());

        let n1 = builder
            .add_layer("Root", [test_promotion(k1)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let _n2 = builder
            .add_layer("Isolated", [test_promotion(k2)], OutputMode::PassThrough)
            .ok();

        builder.set_root(n1);

        let result = builder.build();

        assert!(
            matches!(result, Err(GraphError::UnreachableNode)),
            "should reject unreachable nodes"
        );
    }

    #[test]
    fn build_split_node_without_edges_fails() {
        let mut builder = PromotionGraphBuilder::new();
        let key = PromotionKey::default();

        let node = builder
            .add_layer("Store", [test_promotion(key)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(node);

        let result = builder.build();

        assert!(
            matches!(result, Err(GraphError::SplitSuccessorMismatch)),
            "split node without edges should fail"
        );
    }

    #[test]
    fn build_valid_split_graph() {
        let mut builder = PromotionGraphBuilder::new();
        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());
        let k3 = keys.insert(());

        let root = builder
            .add_layer("Store", [test_promotion(k1)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let promo_path = builder
            .add_layer("Loyalty", [test_promotion(k2)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let no_promo_path = builder
            .add_layer("Coupons", [test_promotion(k3)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        assert!(
            builder
                .connect_split(root, promo_path, no_promo_path)
                .is_ok(),
            "connect_split should succeed"
        );

        let result = builder.build();

        assert!(
            result.is_ok(),
            "valid split graph should build successfully"
        );
    }

    #[test]
    fn connect_pass_through_rejects_second_edge() {
        let mut builder = PromotionGraphBuilder::new();
        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();
        let k1 = keys.insert(());
        let k2 = keys.insert(());
        let k3 = keys.insert(());

        let root = builder
            .add_layer("Root", [test_promotion(k1)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let n2 = builder
            .add_layer("N2", [test_promotion(k2)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let n3 = builder
            .add_layer("N3", [test_promotion(k3)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        assert!(
            builder.connect_pass_through(root, n2).is_ok(),
            "first connection should work"
        );

        let result = builder.connect_pass_through(root, n3);

        assert!(
            matches!(result, Err(GraphError::PassThroughMultipleSuccessors(_))),
            "second connection should fail"
        );
    }

    #[test]
    fn connect_split_rejects_second_edge() {
        let mut builder = PromotionGraphBuilder::new();

        let key = PromotionKey::default();

        let root = builder
            .add_layer("Root", [test_promotion(key)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let a = builder
            .add_layer(
                "A",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let b = builder
            .add_layer(
                "B",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        assert!(builder.connect_split(root, a, b).is_ok());

        let result = builder.connect_split(root, b, a);

        assert!(matches!(result, Err(GraphError::SplitSuccessorMismatch)));
    }

    #[test]
    fn connect_split_single_sided_edges_validate() {
        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();

        let k1 = keys.insert(());
        let k2 = keys.insert(());

        let mut builder = PromotionGraphBuilder::new();

        let root = builder
            .add_layer("Root", [test_promotion(k1)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let leaf = builder
            .add_layer("Leaf", [test_promotion(k2)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        assert!(builder.connect_split_participating_only(root, leaf).is_ok());
        assert!(builder.build().is_ok());

        let mut builder = PromotionGraphBuilder::new();
        let mut keys = slotmap::SlotMap::<PromotionKey, ()>::with_key();

        let k1 = keys.insert(());
        let k2 = keys.insert(());

        let root = builder
            .add_layer("Root", [test_promotion(k1)], OutputMode::Split)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let leaf = builder
            .add_layer("Leaf", [test_promotion(k2)], OutputMode::PassThrough)
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        assert!(
            builder
                .connect_split_non_participating_only(root, leaf)
                .is_ok()
        );
        assert!(builder.build().is_ok());
    }

    #[test]
    fn build_rejects_cycle() {
        let mut builder = PromotionGraphBuilder::new();

        let root = builder
            .add_layer(
                "Root",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let leaf = builder
            .add_layer(
                "Leaf",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        builder.graph.add_edge(root, leaf, LayerEdge::All);
        builder.graph.add_edge(leaf, root, LayerEdge::All);

        let result = builder.build();

        assert!(matches!(result, Err(GraphError::CycleDetected)));
    }

    #[test]
    fn build_rejects_invalid_edge_types_for_output_mode() {
        let mut builder = PromotionGraphBuilder::new();

        let root = builder
            .add_layer(
                "Root",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let child = builder
            .add_layer(
                "Child",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        builder
            .graph
            .add_edge(root, child, LayerEdge::Participating);

        let result = builder.build();

        assert!(matches!(
            result,
            Err(GraphError::PassThroughMultipleSuccessors(_))
        ));

        let mut builder = PromotionGraphBuilder::new();

        let root = builder
            .add_layer(
                "Root",
                [test_promotion(PromotionKey::default())],
                OutputMode::Split,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        let child = builder
            .add_layer(
                "Child",
                [test_promotion(PromotionKey::default())],
                OutputMode::PassThrough,
            )
            .ok()
            .unwrap_or_else(|| NodeIndex::new(0));

        builder.set_root(root);

        builder.graph.add_edge(root, child, LayerEdge::All);

        let result = builder.build();

        assert!(matches!(result, Err(GraphError::SplitSuccessorMismatch)));
    }

    #[test]
    fn validate_helpers_skip_missing_node_weights() {
        let mut graph: StableDiGraph<LayerNode<'_>, LayerEdge> = StableDiGraph::new();

        let node = graph.add_node(LayerNode {
            key: PromotionLayerKey::default(),
            promotions: SmallVec::new(),
            output_mode: OutputMode::PassThrough,
        });

        let removed = graph.add_node(LayerNode {
            key: PromotionLayerKey::default(),
            promotions: SmallVec::new(),
            output_mode: OutputMode::PassThrough,
        });

        let removed_idx = removed;

        graph.remove_node(removed);

        assert!(validate_path(&[node, removed_idx], &graph).is_ok());
        assert!(validate_single_node(&graph, NodeIndex::new(999)).is_ok());
    }

    #[test]
    fn default_builder_matches_new() {
        let mut a = PromotionGraphBuilder::new();
        let mut b = PromotionGraphBuilder::default();

        let key = PromotionKey::default();

        let na = a
            .add_layer("N", [test_promotion(key)], OutputMode::PassThrough)
            .expect("layer");

        let nb = b
            .add_layer("N", [test_promotion(key)], OutputMode::PassThrough)
            .expect("layer");

        a.set_root(na);
        b.set_root(nb);

        assert!(a.build().is_ok());
        assert!(b.build().is_ok());
    }
}
