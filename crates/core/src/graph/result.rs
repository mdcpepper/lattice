//! Layered solver result

use rustc_hash::FxHashMap;
use rusty_money::{Money, iso::Currency};
use smallvec::SmallVec;

use crate::promotions::applications::PromotionApplication;

/// Result of evaluating a promotion graph across all layers.
///
/// Each item in the original basket may accumulate promotion applications
/// from multiple layers as it flows through the graph.
#[derive(Debug, Clone)]
pub struct LayeredSolverResult<'a> {
    /// Final total after all layers have been evaluated
    pub total: Money<'a, Currency>,

    /// Per original-basket-index: ordered list of promotion applications
    /// (one per layer that touched this item)
    pub item_applications: FxHashMap<usize, SmallVec<[PromotionApplication<'a>; 3]>>,

    /// Original basket indices of items that received no promotion in any layer
    pub full_price_items: SmallVec<[usize; 10]>,
}
