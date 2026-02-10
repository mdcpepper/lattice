use std::collections::HashMap;

use lattice::{
    fixtures::{graph::GraphFixture, promotions::PromotionsFixture},
    graph::{OutputMode, PromotionGraph, PromotionGraphBuilder},
    promotions::{Promotion, PromotionKey},
};
use slotmap::{SecondaryMap, SlotMap};

/// Render model for a promotion pill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionPill {
    /// Promotion name.
    pub label: String,

    /// Bundle id from solver application.
    pub bundle_id: usize,

    /// Inline style derived deterministically from `bundle_id`.
    pub style: String,
}

/// Loaded promotions fixture data needed by the app.
#[derive(Debug)]
pub struct LoadedPromotions {
    /// Promotion graph built from fixtures.
    pub graph: PromotionGraph<'static>,

    /// Promotion key -> display name.
    pub promotion_names: SecondaryMap<PromotionKey, String>,
}

/// Load promotions and graph fixture content.
pub fn load_promotions(yaml: &str) -> Result<LoadedPromotions, String> {
    let promotions_fixture: PromotionsFixture = serde_norway::from_str(yaml)
        .map_err(|error| format!("Failed to parse promotions fixture: {error}"))?;
    let graph_fixture: GraphFixture = serde_norway::from_str(yaml)
        .map_err(|error| format!("Failed to parse promotion graph fixture: {error}"))?;

    let mut promotion_key_slots: SlotMap<PromotionKey, ()> = SlotMap::with_key();
    let mut promotion_names: SecondaryMap<PromotionKey, String> = SecondaryMap::new();
    let mut promotions_by_fixture_key: HashMap<String, Promotion<'static>> = HashMap::new();

    for (fixture_key, promotion_fixture) in promotions_fixture.promotions {
        let promotion_key = promotion_key_slots.insert(());
        let (promotion_meta, promotion) = promotion_fixture
            .try_into_promotion(promotion_key)
            .map_err(|error| format!("Failed to parse promotion '{fixture_key}': {error}"))?;

        promotion_names.insert(promotion_key, promotion_meta.name.clone());
        promotions_by_fixture_key.insert(fixture_key, promotion);
    }

    Ok(LoadedPromotions {
        graph: build_graph(&graph_fixture, &promotions_by_fixture_key)?,
        promotion_names,
    })
}

/// Deterministic bundle color style derived from bundle id.
pub fn bundle_pill_style(bundle_id: usize) -> String {
    let hue = ((bundle_id as u64 * 137 + 47) % 360) as u16;

    format!(
        "background-color:hsl({hue},85%,92%);border-color:hsl({hue},70%,74%);color:hsl({hue},60%,24%);"
    )
}

fn build_graph(
    graph_fixture: &GraphFixture,
    promotions_by_fixture_key: &HashMap<String, Promotion<'static>>,
) -> Result<PromotionGraph<'static>, String> {
    let mut builder = PromotionGraphBuilder::new();
    let mut node_indices = HashMap::new();

    for (label, node_fixture) in &graph_fixture.nodes {
        let mut promotions_for_node: Vec<Promotion<'static>> = Vec::new();

        for promotion_key in &node_fixture.promotions {
            let promotion = promotions_by_fixture_key
                .get(promotion_key)
                .ok_or_else(|| {
                    format!("Unknown promotion key in graph node '{label}': {promotion_key}")
                })?
                .clone();

            promotions_for_node.push(promotion);
        }

        let node_idx = builder
            .add_layer(label.clone(), promotions_for_node, node_fixture.output)
            .map_err(|error| format!("Failed to add graph node '{label}': {error}"))?;

        node_indices.insert(label.clone(), node_idx);
    }

    let root = node_indices
        .get(&graph_fixture.root)
        .copied()
        .ok_or_else(|| format!("Graph root node '{}' not found", graph_fixture.root))?;

    builder.set_root(root);

    for (label, node_fixture) in &graph_fixture.nodes {
        let from_idx = node_indices
            .get(label)
            .copied()
            .ok_or_else(|| format!("Graph node '{label}' not found"))?;

        match node_fixture.output {
            OutputMode::PassThrough => {
                if let Some(next_label) = node_fixture.next.as_deref() {
                    let to_idx = node_indices
                        .get(next_label)
                        .copied()
                        .ok_or_else(|| format!("Pass-through target '{next_label}' not found"))?;

                    builder
                        .connect_pass_through(from_idx, to_idx)
                        .map_err(|error| {
                            format!("Failed to connect '{label}' -> '{next_label}': {error}")
                        })?;
                }
            }
            OutputMode::Split => {
                match (
                    node_fixture.participating.as_deref(),
                    node_fixture.non_participating.as_deref(),
                ) {
                    (Some(participating_label), Some(non_participating_label)) => {
                        let participating_idx = node_indices
                            .get(participating_label)
                            .copied()
                            .ok_or_else(|| {
                                format!("Participating target '{participating_label}' not found")
                            })?;
                        let non_participating_idx = node_indices
                            .get(non_participating_label)
                            .copied()
                            .ok_or_else(|| {
                                format!(
                                    "Non-participating target '{non_participating_label}' not found"
                                )
                            })?;

                        builder
                            .connect_split(from_idx, participating_idx, non_participating_idx)
                            .map_err(|error| {
                                format!("Failed to connect split node '{label}': {error}")
                            })?;
                    }
                    (Some(participating_label), None) => {
                        let participating_idx = node_indices
                            .get(participating_label)
                            .copied()
                            .ok_or_else(|| {
                                format!("Participating target '{participating_label}' not found")
                            })?;

                        builder
                        .connect_split_participating_only(from_idx, participating_idx)
                        .map_err(|error| {
                            format!("Failed to connect split (participating only) '{label}': {error}")
                        })?;
                    }
                    (None, Some(non_participating_label)) => {
                        let non_participating_idx = node_indices
                            .get(non_participating_label)
                            .copied()
                            .ok_or_else(|| {
                                format!(
                                    "Non-participating target '{non_participating_label}' not found"
                                )
                            })?;

                        builder
                        .connect_split_non_participating_only(from_idx, non_participating_idx)
                        .map_err(|error| {
                            format!(
                                "Failed to connect split (non-participating only) '{label}': {error}"
                            )
                        })?;
                    }
                    (None, None) => {
                        return Err(format!(
                            "Split node '{label}' must define at least one target"
                        ));
                    }
                }
            }
        }
    }

    PromotionGraph::from_builder(builder)
        .map_err(|error| format!("Failed to build promotion graph: {error}"))
}
