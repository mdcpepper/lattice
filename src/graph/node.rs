//! Graph node weights

use serde::Deserialize;
use slotmap::new_key_type;
use smallvec::SmallVec;

use crate::promotions::Promotion;

/// How items are routed to successor nodes after solving a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    /// All items (promoted + unpromoted) flow to a single successor via an `All` edge.
    /// The node may have zero or one outgoing edge.
    #[serde(alias = "pass_through", alias = "passthrough")]
    PassThrough,

    /// Promoted items (discounted at any point up to and including this layer)
    /// flow to one successor, unpromoted items to another.
    /// The node may have one or two outgoing edges:
    /// `Participating`, `NonParticipating`, or both.
    Split,
}

new_key_type! {
    /// Key identifying a promotion layer in the graph.
    pub struct PromotionLayerKey;
}

/// A node in the promotion graph representing a layer of competing promotions.
#[derive(Debug, Clone)]
pub struct LayerNode<'a> {
    /// Key for the human-readable name for this layer
    pub key: PromotionLayerKey,

    /// Promotions that compete within this layer (solved by a single ILP call)
    pub promotions: SmallVec<[Promotion<'a>; 5]>,

    /// How items are routed to successor nodes
    pub output_mode: OutputMode,
}
