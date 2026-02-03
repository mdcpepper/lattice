//! Solvers for Promotions

use good_lp::ResolutionError;
use rusty_money::{Money, MoneyError, iso::Currency};
use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    discounts::DiscountError,
    items::groups::{ItemGroup, ItemGroupError},
    promotions::{Promotion, applications::PromotionApplication},
};

pub mod ilp;

/// Solver Errors
#[derive(Debug, Error)]
pub enum SolverError {
    /// Money amount in minor units cannot be represented exactly as a solver coefficient.
    #[error(
        "money amount in minor units cannot be represented exactly as a solver coefficient: {minor_units}"
    )]
    MinorUnitsNotRepresentable {
        /// Money amount in minor units
        minor_units: i64,
    },

    /// Wrapped item group error
    #[error(transparent)]
    ItemGroup(#[from] ItemGroupError),

    /// Wrapped money arithmetic or currency mismatch error.
    #[error(transparent)]
    Money(#[from] MoneyError),

    /// Wrapped discount calculation error.
    #[error(transparent)]
    Discount(#[from] DiscountError),

    /// Wrapped solver resolution error
    #[error(transparent)]
    ResolutionError(#[from] ResolutionError),

    /// Internal solver invariant was violated (this is a bug).
    #[error("solver invariant violated: {message}")]
    InvariantViolation {
        /// What invariant was violated
        message: &'static str,
    },
}

/// Result of the promotion solution for the given item group
#[derive(Debug, Clone)]
pub struct SolverResult<'a> {
    /// Indexes of item group entries that were affected by promotions
    pub affected_items: SmallVec<[usize; 10]>,

    /// Indexes of item group entries that were not affected by promotions
    pub unaffected_items: SmallVec<[usize; 10]>,

    /// Total cost of the items after applying promotions
    pub total: Money<'a, Currency>,

    /// Details of each promotion application (item, bundle, original/final price)
    pub promotion_applications: SmallVec<[PromotionApplication<'a>; 10]>,
}

/// Trait for solving promotion problems on a set of items
pub trait Solver {
    /// Solve the promotions for the given item group
    ///
    /// # Errors
    ///
    /// Returns a [`SolverError`] if the solver encounters an error.
    fn solve<'group>(
        promotions: &[Promotion<'_>],
        item_group: &'group ItemGroup<'_>,
    ) -> Result<SolverResult<'group>, SolverError>;
}
