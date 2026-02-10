use std::collections::BTreeMap;

use petgraph::graph::NodeIndex;
use slotmap::{SecondaryMap, SlotMap};

use lattice::{
    fixtures::{
        graph::{GraphFixture, GraphNodeFixture},
        promotions::PromotionsFixture,
    },
    graph::{OutputMode, PromotionGraph, PromotionGraphBuilder},
    promotions::{Promotion, PromotionKey, PromotionMeta},
};

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

    /// Promotion key to display name.
    pub promotion_names: SecondaryMap<PromotionKey, String>,

    /// Promotion metadata keyed by promotion key.
    pub promotion_meta_map: SlotMap<PromotionKey, PromotionMeta>,
}

/// Load promotions and graph fixture content.
///
/// # Errors
///
/// Returns an error when promotion fixtures cannot be parsed, promotion
/// definitions are invalid, referenced graph promotions are missing, or graph
/// validation/building fails.
pub fn load_promotions(yaml: &str) -> Result<LoadedPromotions, String> {
    let promotions_fixture: PromotionsFixture = serde_norway::from_str(yaml)
        .map_err(|error| format!("Failed to parse promotions fixture: {error}"))?;

    let graph_fixture: GraphFixture = serde_norway::from_str(yaml)
        .map_err(|error| format!("Failed to parse promotion graph fixture: {error}"))?;

    let mut promotion_meta_map: SlotMap<PromotionKey, PromotionMeta> = SlotMap::with_key();
    let mut promotion_names: SecondaryMap<PromotionKey, String> = SecondaryMap::new();
    let mut promotions_by_fixture_key: BTreeMap<String, Promotion<'static>> = BTreeMap::new();

    for (fixture_key, promotion_fixture) in promotions_fixture.promotions {
        let promotion_key = promotion_meta_map.insert(PromotionMeta::default());

        let (promotion_meta, promotion) = promotion_fixture
            .try_into_promotion(promotion_key)
            .map_err(|error| format!("Failed to parse promotion '{fixture_key}': {error}"))?;

        promotion_names.insert(promotion_key, promotion_meta.name.clone());

        let Some(meta_slot) = promotion_meta_map.get_mut(promotion_key) else {
            return Err("Failed to store promotion metadata".to_string());
        };

        *meta_slot = promotion_meta;

        promotions_by_fixture_key.insert(fixture_key, promotion);
    }

    Ok(LoadedPromotions {
        graph: build_graph(&graph_fixture, &promotions_by_fixture_key)?,
        promotion_names,
        promotion_meta_map,
    })
}

/// Deterministic bundle color style derived from bundle id.
pub fn bundle_pill_style(bundle_id: usize) -> String {
    let bundle = bundle_id as u64;
    let hue = ((bundle * 137 + 47 + (bundle / 24) * 19) % 360) as u16;

    let tone_band = (bundle / 12) % 6;
    let (bg_sat, bg_light, border_sat, border_light, text_sat, text_light) = match tone_band {
        0 => (85, 92, 70, 74, 60, 24),
        1 => (78, 89, 66, 68, 62, 22),
        2 => (72, 86, 62, 62, 66, 20),
        3 => (68, 83, 58, 58, 68, 19),
        4 => (76, 90, 64, 70, 58, 23),
        _ => (82, 94, 68, 78, 55, 26),
    };

    format!(
        "background-color:hsl({hue},{bg_sat}%,{bg_light}%);border-color:hsl({hue},{border_sat}%,{border_light}%);color:hsl({hue},{text_sat}%,{text_light}%);"
    )
}

fn build_graph(
    graph_fixture: &GraphFixture,
    promotions_by_fixture_key: &BTreeMap<String, Promotion<'static>>,
) -> Result<PromotionGraph<'static>, String> {
    let mut builder = PromotionGraphBuilder::new();

    let node_indices = add_graph_nodes(&mut builder, graph_fixture, promotions_by_fixture_key)?;

    let root = node_indices
        .get(&graph_fixture.root)
        .copied()
        .ok_or_else(|| format!("Graph root node '{}' not found", graph_fixture.root))?;

    builder.set_root(root);
    connect_graph_edges(&mut builder, graph_fixture, &node_indices)?;

    PromotionGraph::from_builder(builder)
        .map_err(|error| format!("Failed to build promotion graph: {error}"))
}

fn add_graph_nodes(
    builder: &mut PromotionGraphBuilder<'static>,
    graph_fixture: &GraphFixture,
    promotions_by_fixture_key: &BTreeMap<String, Promotion<'static>>,
) -> Result<BTreeMap<String, NodeIndex>, String> {
    let mut node_indices = BTreeMap::new();

    for (label, node_fixture) in &graph_fixture.nodes {
        let promotions_for_node = node_fixture
            .promotions
            .iter()
            .map(|promotion_key| {
                promotions_by_fixture_key
                    .get(promotion_key)
                    .cloned()
                    .ok_or_else(|| {
                        format!("Unknown promotion key in graph node '{label}': {promotion_key}")
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let node_idx = builder
            .add_layer(label.clone(), promotions_for_node, node_fixture.output)
            .map_err(|error| format!("Failed to add graph node '{label}': {error}"))?;

        node_indices.insert(label.clone(), node_idx);
    }

    Ok(node_indices)
}

fn connect_graph_edges(
    builder: &mut PromotionGraphBuilder<'static>,
    graph_fixture: &GraphFixture,
    node_indices: &BTreeMap<String, NodeIndex>,
) -> Result<(), String> {
    for (label, node_fixture) in &graph_fixture.nodes {
        let from_idx = node_indices
            .get(label)
            .copied()
            .ok_or_else(|| format!("Graph node '{label}' not found"))?;

        match node_fixture.output {
            OutputMode::PassThrough => {
                connect_pass_through_edge(builder, node_indices, from_idx, label, node_fixture)?;
            }
            OutputMode::Split => {
                connect_split_edges(builder, node_indices, from_idx, label, node_fixture)?;
            }
        }
    }

    Ok(())
}

fn connect_pass_through_edge(
    builder: &mut PromotionGraphBuilder<'static>,
    node_indices: &BTreeMap<String, NodeIndex>,
    from_idx: NodeIndex,
    label: &str,
    node_fixture: &GraphNodeFixture,
) -> Result<(), String> {
    if let Some(next_label) = node_fixture.next.as_deref() {
        let to_idx = node_indices
            .get(next_label)
            .copied()
            .ok_or_else(|| format!("Pass-through target '{next_label}' not found"))?;

        builder
            .connect_pass_through(from_idx, to_idx)
            .map_err(|error| format!("Failed to connect '{label}' -> '{next_label}': {error}"))?;
    }

    Ok(())
}

fn connect_split_edges(
    builder: &mut PromotionGraphBuilder<'static>,
    node_indices: &BTreeMap<String, NodeIndex>,
    from_idx: NodeIndex,
    label: &str,
    node_fixture: &GraphNodeFixture,
) -> Result<(), String> {
    match (
        node_fixture.participating.as_deref(),
        node_fixture.non_participating.as_deref(),
    ) {
        (Some(participating_label), Some(non_participating_label)) => {
            let participating_idx = node_indices
                .get(participating_label)
                .copied()
                .ok_or_else(|| format!("Participating target '{participating_label}' not found"))?;

            let non_participating_idx = node_indices
                .get(non_participating_label)
                .copied()
                .ok_or_else(|| {
                    format!("Non-participating target '{non_participating_label}' not found")
                })?;

            builder
                .connect_split(from_idx, participating_idx, non_participating_idx)
                .map_err(|error| format!("Failed to connect split node '{label}': {error}"))?;
        }
        (Some(participating_label), None) => {
            let participating_idx = node_indices
                .get(participating_label)
                .copied()
                .ok_or_else(|| format!("Participating target '{participating_label}' not found"))?;

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
                    format!("Non-participating target '{non_participating_label}' not found")
                })?;

            builder
                .connect_split_non_participating_only(from_idx, non_participating_idx)
                .map_err(|error| {
                    format!("Failed to connect split (non-participating only) '{label}': {error}")
                })?;
        }
        (None, None) => {
            return Err(format!(
                "Split node '{label}' must define at least one target"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    // Test bundle_pill_style function
    #[test]
    fn test_bundle_pill_style_deterministic() {
        let style1 = bundle_pill_style(0);
        let style2 = bundle_pill_style(0);

        assert_eq!(style1, style2);
    }

    #[test]
    fn test_bundle_pill_style_different_bundles() {
        let style1 = bundle_pill_style(0);
        let style2 = bundle_pill_style(1);

        assert_ne!(style1, style2);
    }

    #[test]
    fn test_bundle_pill_style_tone_bands() {
        // Test different tone bands (based on bundle / 12)
        let style_band_0 = bundle_pill_style(0);
        let style_band_1 = bundle_pill_style(12);
        let style_band_2 = bundle_pill_style(24);
        let style_band_3 = bundle_pill_style(36);
        let style_band_4 = bundle_pill_style(48);
        let style_band_5 = bundle_pill_style(60);

        // All should be different due to different saturation/lightness values
        assert_ne!(style_band_0, style_band_1);
        assert_ne!(style_band_1, style_band_2);
        assert_ne!(style_band_2, style_band_3);
        assert_ne!(style_band_3, style_band_4);
        assert_ne!(style_band_4, style_band_5);
    }

    // Test load_promotions function
    #[test]
    fn test_load_promotions_empty() -> TestResult {
        let yaml = r"
promotions: {}
root: layer1
nodes:
  layer1:
    promotions: []
    output: pass-through
";

        let result = load_promotions(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.promotion_names.len(), 0);

        Ok(())
    }

    #[test]
    fn test_load_promotions_invalid_yaml() {
        let yaml = "invalid: yaml: structure: [[[";

        let result = load_promotions(yaml);

        assert!(result.is_err());
    }

    #[test]
    fn test_load_promotions_single_promotion() -> TestResult {
        let yaml = r#"
promotions:
  promo1:
    type: direct_discount
    name: "Test Promo"
    tags: [test]
    discount:
      type: percentage_off
      amount: 10%
root: layer1
nodes:
  layer1:
    promotions: ["promo1"]
    output: pass-through
"#;

        let result = load_promotions(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.promotion_names.len(), 1);

        Ok(())
    }

    #[test]
    fn test_load_promotions_graph_missing_root() {
        let yaml = r"
promotions: {}
nodes:
  layer1:
    promotions: []
    output: pass-through
";

        let result = load_promotions(yaml);

        // Should fail due to missing root field
        assert!(result.is_err());
    }

    #[test]
    fn test_load_promotions_graph_unknown_promotion() {
        let yaml = r#"
promotions: {}
root: layer1
nodes:
  layer1:
    promotions: ["unknown"]
    output: pass-through
"#;

        let result = load_promotions(yaml);

        assert!(result.is_err());
        assert!(result.is_err_and(|error| error.contains("Unknown promotion key")));
    }

    #[test]
    fn test_load_promotions_multiple_layers() -> TestResult {
        let yaml = r#"
promotions:
  promo1:
    type: direct_discount
    name: "Promo 1"
    tags: [a]
    discount:
      type: percentage_off
      amount: 10%
  promo2:
    type: direct_discount
    name: "Promo 2"
    tags: [b]
    discount:
      type: percentage_off
      amount: 20%
root: layer1
nodes:
  layer1:
    promotions: ["promo1"]
    output: pass-through
    next: layer2
  layer2:
    promotions: ["promo2"]
    output: pass-through
"#;

        let result = load_promotions(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.promotion_names.len(), 2);

        Ok(())
    }

    #[test]
    fn test_load_promotions_split_mode() {
        let yaml = r"
promotions: {}
root: layer1
nodes:
  layer1:
    promotions: []
    output: split
    participating: layer2
    non_participating: layer3
  layer2:
    promotions: []
    output: pass-through
  layer3:
    promotions: []
    output: pass-through
";

        let result = load_promotions(yaml);

        assert!(result.is_ok());
    }

    #[test]
    fn test_load_promotions_invalid_promotion_structure() {
        let yaml = r#"
promotions:
  promo1:
    name: "Test Promo"
    type: invalid_type
root: layer1
nodes:
  layer1:
    promotions: ["promo1"]
    output: pass-through
"#;

        let result = load_promotions(yaml);

        assert!(result.is_err());
    }
}
