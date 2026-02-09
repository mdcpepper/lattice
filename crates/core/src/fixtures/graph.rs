//! Graph Fixtures
//!
//! Loads promotion graph definitions from YAML files in the `fixtures/promotions/` directory.

use std::fs;

use petgraph::graph::NodeIndex;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    fixtures::{Fixture, FixtureError},
    graph::{
        PromotionGraph,
        builder::PromotionGraphBuilder,
        node::{OutputMode, PromotionLayerKey},
    },
    promotions::{Promotion, PromotionKey},
};

/// Top-level graph fixture from YAML.
#[derive(Debug, Deserialize)]
pub struct GraphFixture {
    /// Key of the root node
    pub root: String,

    /// Node definitions keyed by label
    pub nodes: FxHashMap<String, GraphNodeFixture>,
}

/// A single node in the graph fixture.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GraphNodeFixture {
    /// Promotion keys that belong to this layer (must match keys from promotions fixture)
    pub promotions: Vec<String>,

    /// Output mode: "split" or "pass-through"
    pub output: OutputMode,

    /// Target node for participating items (only used with "split" output)
    pub participating: Option<String>,

    /// Target node for non-participating items (only used with "split" output)
    #[serde(alias = "non_participating")]
    pub non_participating: Option<String>,

    /// Target node for all items (only used with "pass-through" output, optional for leaf nodes)
    pub next: Option<String>,
}

impl Fixture<'_> {
    /// Load a graph fixture and build/store a [`PromotionGraph`] from it.
    ///
    /// The graph YAML file references promotion keys that must already be loaded
    /// via [`Fixture::load_promotions`].
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] if the file cannot be read or parsed, or if
    /// referenced promotion keys are not found.
    pub fn load_graph(&mut self, name: &str) -> Result<&mut Self, FixtureError> {
        let file_path = self
            .base_path
            .join("promotions")
            .join(format!("{name}.yml"));

        let contents = fs::read_to_string(&file_path)?;
        let fixture: GraphFixture = serde_norway::from_str(&contents)?;
        let graph = build_graph_from_fixture(&fixture, self)?;

        self.graph = Some(graph);

        Ok(self)
    }
}

fn build_graph_from_fixture<'a>(
    fixture: &GraphFixture,
    loaded: &mut Fixture<'a>,
) -> Result<PromotionGraph<'a>, FixtureError> {
    reset_layer_name_mappings(loaded);

    let mut builder = PromotionGraphBuilder::new();
    let mut node_indices: FxHashMap<String, NodeIndex> = FxHashMap::default();
    let mut layer_keys = SlotMap::<PromotionLayerKey, ()>::with_key();

    create_layer_nodes(
        fixture,
        loaded,
        &mut builder,
        &mut node_indices,
        &mut layer_keys,
    )?;

    set_root_node(fixture, &node_indices, &mut builder)?;

    connect_layer_edges(fixture, &node_indices, &mut builder)?;

    PromotionGraph::from_builder(builder)
        .map_err(|e| FixtureError::InvalidPromotionData(format!("graph validation error: {e}")))
}

fn reset_layer_name_mappings(loaded: &mut Fixture<'_>) {
    for (_promotion_key, promotion_meta) in &mut loaded.promotion_meta {
        promotion_meta.layer_names = SecondaryMap::new();
    }
}

fn create_layer_nodes<'a>(
    fixture: &GraphFixture,
    loaded: &mut Fixture<'a>,
    builder: &mut PromotionGraphBuilder<'a>,
    node_indices: &mut FxHashMap<String, NodeIndex>,
    layer_keys: &mut SlotMap<PromotionLayerKey, ()>,
) -> Result<(), FixtureError> {
    for (label, node_fixture) in &fixture.nodes {
        let layer_key = layer_keys.insert(());
        let promotion_keys = resolve_promotion_keys(node_fixture, loaded)?;
        let promotions = resolve_promotions(node_fixture, loaded)?;

        let node_idx = builder
            .add_layer_with_key(layer_key, promotions, node_fixture.output)
            .map_err(|e| FixtureError::InvalidPromotionData(format!("graph build error: {e}")))?;

        register_layer_name(loaded, &promotion_keys, layer_key, label)?;

        node_indices.insert(label.clone(), node_idx);
    }

    Ok(())
}

fn resolve_promotion_keys(
    node_fixture: &GraphNodeFixture,
    loaded: &Fixture<'_>,
) -> Result<Vec<PromotionKey>, FixtureError> {
    node_fixture
        .promotions
        .iter()
        .map(|key| {
            loaded
                .promotion_keys
                .get(key)
                .copied()
                .ok_or_else(|| FixtureError::PromotionNotFound(key.clone()))
        })
        .collect()
}

fn resolve_promotions<'a>(
    node_fixture: &GraphNodeFixture,
    loaded: &Fixture<'a>,
) -> Result<Vec<Promotion<'a>>, FixtureError> {
    node_fixture
        .promotions
        .iter()
        .map(|key| loaded.promotion(key).cloned())
        .collect()
}

fn register_layer_name(
    loaded: &mut Fixture<'_>,
    promotion_keys: &[PromotionKey],
    layer_key: PromotionLayerKey,
    label: &str,
) -> Result<(), FixtureError> {
    for &promotion_key in promotion_keys {
        let Some(promotion_meta) = loaded.promotion_meta.get_mut(promotion_key) else {
            return Err(FixtureError::InvalidPromotionData(format!(
                "missing promotion metadata for key {promotion_key:?}"
            )));
        };

        promotion_meta
            .layer_names
            .insert(layer_key, label.to_string());
    }

    Ok(())
}

fn set_root_node(
    fixture: &GraphFixture,
    node_indices: &FxHashMap<String, NodeIndex>,
    builder: &mut PromotionGraphBuilder<'_>,
) -> Result<(), FixtureError> {
    let root_idx = node_indices.get(&fixture.root).copied().ok_or_else(|| {
        FixtureError::InvalidPromotionData(format!(
            "root node '{}' not found in graph",
            fixture.root
        ))
    })?;

    builder.set_root(root_idx);

    Ok(())
}

fn connect_layer_edges(
    fixture: &GraphFixture,
    node_indices: &FxHashMap<String, NodeIndex>,
    builder: &mut PromotionGraphBuilder<'_>,
) -> Result<(), FixtureError> {
    for (label, node_fixture) in &fixture.nodes {
        let from_idx = node_indices.get(label).copied().ok_or_else(|| {
            FixtureError::InvalidPromotionData(format!("node '{label}' not found"))
        })?;

        match node_fixture.output {
            OutputMode::PassThrough => connect_pass_through_edge(
                node_indices,
                builder,
                from_idx,
                node_fixture.next.as_deref(),
            )?,
            OutputMode::Split => {
                connect_split_edges(node_indices, builder, from_idx, label, node_fixture)?;
            }
        }
    }

    Ok(())
}

fn connect_pass_through_edge(
    node_indices: &FxHashMap<String, NodeIndex>,
    builder: &mut PromotionGraphBuilder<'_>,
    from_idx: NodeIndex,
    next: Option<&str>,
) -> Result<(), FixtureError> {
    let Some(next_label) = next else {
        return Ok(());
    };

    let to_idx = node_indices.get(next_label).copied().ok_or_else(|| {
        FixtureError::InvalidPromotionData(format!("pass-through target '{next_label}' not found"))
    })?;

    builder
        .connect_pass_through(from_idx, to_idx)
        .map_err(|e| FixtureError::InvalidPromotionData(format!("graph build error: {e}")))
}

fn connect_split_edges(
    node_indices: &FxHashMap<String, NodeIndex>,
    builder: &mut PromotionGraphBuilder<'_>,
    from_idx: NodeIndex,
    label: &str,
    node_fixture: &GraphNodeFixture,
) -> Result<(), FixtureError> {
    match (
        node_fixture.participating.as_deref(),
        node_fixture.non_participating.as_deref(),
    ) {
        (Some(participating_label), Some(non_participating_label)) => {
            let participating_idx =
                lookup_target(node_indices, participating_label, "participating")?;
            let non_participating_idx =
                lookup_target(node_indices, non_participating_label, "non-participating")?;

            builder
                .connect_split(from_idx, participating_idx, non_participating_idx)
                .map_err(|e| FixtureError::InvalidPromotionData(format!("graph build error: {e}")))
        }
        (Some(participating_label), None) => {
            let participating_idx =
                lookup_target(node_indices, participating_label, "participating")?;

            builder
                .connect_split_participating_only(from_idx, participating_idx)
                .map_err(|e| FixtureError::InvalidPromotionData(format!("graph build error: {e}")))
        }
        (None, Some(non_participating_label)) => {
            let non_participating_idx =
                lookup_target(node_indices, non_participating_label, "non-participating")?;

            builder
                .connect_split_non_participating_only(from_idx, non_participating_idx)
                .map_err(|e| FixtureError::InvalidPromotionData(format!("graph build error: {e}")))
        }
        (None, None) => Err(FixtureError::InvalidPromotionData(format!(
            "split node '{label}' must have at least one target (participating or non-participating)"
        ))),
    }
}

fn lookup_target(
    node_indices: &FxHashMap<String, NodeIndex>,
    target_label: &str,
    target_type: &str,
) -> Result<NodeIndex, FixtureError> {
    node_indices.get(target_label).copied().ok_or_else(|| {
        FixtureError::InvalidPromotionData(format!(
            "{target_type} target '{target_label}' not found"
        ))
    })
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;
    use testresult::TestResult;

    use super::{GraphFixture, GraphNodeFixture, build_graph_from_fixture};
    use crate::{
        fixtures::{Fixture, FixtureError},
        graph::OutputMode,
    };

    #[test]
    fn load_graph_fixture_succeeds() -> TestResult {
        let fixture = Fixture::from_set("layered")?;
        let graph = fixture.graph()?;

        // Graph should evaluate without error
        let item_group = fixture.item_group()?;
        let result = graph.evaluate(&item_group)?;

        // All items should have some result
        let total_items = result.item_applications.len() + result.full_price_items.len();
        assert_eq!(total_items, 7, "all 7 items should be accounted for");

        Ok(())
    }

    #[test]
    fn graph_fixture_missing_file_returns_error() {
        let fixture = Fixture::from_set("layered");

        assert!(fixture.is_ok(), "layered fixture should load");

        let mut fixture = fixture.unwrap_or_else(|_| Fixture::new());
        let result = fixture.load_graph("nonexistent");

        assert!(result.is_err(), "missing graph file should return error");
    }

    fn layered_promotions_fixture() -> Fixture<'static> {
        let mut fixture = Fixture::new();

        fixture
            .load_products("layered")
            .expect("products fixture should load")
            .load_items("layered")
            .expect("items fixture should load")
            .load_promotions("layered")
            .expect("promotions fixture should load");

        fixture
    }

    fn node(promotions: &[&str], output: OutputMode) -> GraphNodeFixture {
        GraphNodeFixture {
            promotions: promotions.iter().map(|s| (*s).to_string()).collect(),
            output,
            participating: None,
            non_participating: None,
            next: None,
        }
    }

    #[test]
    fn build_graph_from_fixture_rejects_missing_root() {
        let mut loaded = layered_promotions_fixture();
        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();

        nodes.insert(
            "only".to_string(),
            node(&["lunch-deal"], OutputMode::PassThrough),
        );

        let fixture = GraphFixture {
            root: "missing-root".to_string(),
            nodes,
        };

        let err = build_graph_from_fixture(&fixture, &mut loaded).expect_err("expected root error");

        assert!(
            matches!(err, FixtureError::InvalidPromotionData(message) if message.contains("root node 'missing-root' not found"))
        );
    }

    #[test]
    fn build_graph_from_fixture_rejects_missing_targets() {
        let mut loaded = layered_promotions_fixture();

        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();
        let mut pass_through = node(&["lunch-deal"], OutputMode::PassThrough);

        pass_through.next = Some("missing".to_string());
        nodes.insert("root".to_string(), pass_through);

        let fixture = GraphFixture {
            root: "root".to_string(),
            nodes,
        };

        let err =
            build_graph_from_fixture(&fixture, &mut loaded).expect_err("expected target error");

        assert!(
            matches!(err, FixtureError::InvalidPromotionData(message) if message.contains("pass-through target 'missing' not found"))
        );

        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();
        let mut split = node(&["lunch-deal"], OutputMode::Split);

        split.participating = Some("missing-p".to_string());
        split.non_participating = Some("missing-n".to_string());

        nodes.insert("root".to_string(), split);

        let fixture = GraphFixture {
            root: "root".to_string(),
            nodes,
        };

        let err = build_graph_from_fixture(&fixture, &mut loaded)
            .expect_err("expected split target error");

        assert!(
            matches!(err, FixtureError::InvalidPromotionData(message) if message.contains("participating target 'missing-p' not found"))
        );
    }

    #[test]
    fn build_graph_from_fixture_supports_single_sided_split_and_rejects_no_targets() {
        let mut loaded = layered_promotions_fixture();

        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();
        let mut split = node(&["lunch-deal"], OutputMode::Split);

        split.participating = Some("leaf".to_string());

        nodes.insert("root".to_string(), split);
        nodes.insert("leaf".to_string(), node(&[], OutputMode::PassThrough));

        let fixture = GraphFixture {
            root: "root".to_string(),
            nodes,
        };

        assert!(build_graph_from_fixture(&fixture, &mut loaded).is_ok());

        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();
        let mut split = node(&["lunch-deal"], OutputMode::Split);

        split.non_participating = Some("leaf".to_string());

        nodes.insert("root".to_string(), split);
        nodes.insert("leaf".to_string(), node(&[], OutputMode::PassThrough));

        let fixture = GraphFixture {
            root: "root".to_string(),
            nodes,
        };

        assert!(build_graph_from_fixture(&fixture, &mut loaded).is_ok());

        let mut nodes: FxHashMap<String, GraphNodeFixture> = FxHashMap::default();
        nodes.insert("root".to_string(), node(&["lunch-deal"], OutputMode::Split));

        let fixture = GraphFixture {
            root: "root".to_string(),
            nodes,
        };

        let err =
            build_graph_from_fixture(&fixture, &mut loaded).expect_err("expected no-target error");

        assert!(
            matches!(err, FixtureError::InvalidPromotionData(message) if message.contains("must have at least one target"))
        );
    }
}
