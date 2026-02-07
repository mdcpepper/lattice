//! Graph errors

use rusty_money::MoneyError;
use thiserror::Error;

use crate::{
    graph::PromotionLayerKey, items::groups::ItemGroupError, promotions::PromotionKey,
    solvers::SolverError,
};

/// Errors that can occur when building or evaluating a promotion graph.
#[derive(Debug, Error)]
pub enum GraphError {
    /// No root node was set before building the graph.
    #[error("no root node set on the promotion graph")]
    NoRoot,

    /// The graph contains a cycle, which is not allowed.
    #[error("promotion graph contains a cycle")]
    CycleDetected,

    /// A promotion key appears in more than one layer node.
    #[error("promotion {0:?} appears in more than one layer")]
    DuplicatePromotion(PromotionKey),

    /// A promotion key appears more than once in a single path through the graph.
    #[error("promotion {key:?} appears multiple times in path: {path:?}")]
    DuplicatePromotionInPath {
        /// The duplicate promotion key
        key: PromotionKey,

        /// Keys of nodes in the path where duplication occurred
        path: Vec<PromotionLayerKey>,
    },

    /// A `PassThrough` node has more than one outgoing edge.
    #[error("pass-through node {0} has more than one successor")]
    PassThroughMultipleSuccessors(usize),

    /// A `Split` node does not have one or two valid outgoing split edges.
    #[error(
        "split node has incorrect successor edges (need one or two: Participating and/or NonParticipating)"
    )]
    SplitSuccessorMismatch,

    /// A node in the graph is not reachable from the root.
    #[error("graph contains unreachable nodes")]
    UnreachableNode,

    /// The ILP solver returned an error while evaluating a layer.
    #[error("solver error in layer {layer_key:?}: {source}")]
    Solver {
        /// Key of the layer that failed
        layer_key: PromotionLayerKey,

        /// The underlying solver error
        source: SolverError,
    },

    /// Error constructing an item group for a layer.
    #[error(transparent)]
    ItemGroup(#[from] ItemGroupError),

    /// Money arithmetic error during evaluation.
    #[error(transparent)]
    Money(#[from] MoneyError),
}
