//! Graph edge weights

/// Edge weight in a promotion graph, describing which items flow along this edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerEdge {
    /// All items (promoted and unpromoted) flow along this edge.
    /// Used with [`super::node::OutputMode::PassThrough`] nodes.
    All,

    /// Only items that have participated in any promotion so far (including parent nodes).
    /// Used with [`super::node::OutputMode::Split`] nodes.
    Participating,

    /// Only items that have NOT participated in any promotion so far.
    /// Used with [`super::node::OutputMode::Split`] nodes.
    NonParticipating,
}
