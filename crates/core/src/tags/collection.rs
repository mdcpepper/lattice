//! Tag Collection
//!
//! A collection-based tagging system for efficient intersection operations.

use std::{
    fmt,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign},
};

/// Trait for tag collections that support intersection operations.
pub trait TagCollection:
    Clone
    + fmt::Debug
    + PartialEq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + BitAndAssign
    + BitOrAssign
    + BitXorAssign
{
    /// Check if this collection intersects with another collection.
    /// Returns true if they have at least one tag in common.
    fn intersects(&self, other: &Self) -> bool;

    /// Get the intersection of this collection with another.
    /// Returns a new collection containing only the common tags.
    #[must_use]
    fn intersection(&self, other: &Self) -> Self;

    /// Check if this collection contains a specific tag.
    fn contains(&self, tag: &str) -> bool;

    /// Check if this collection is empty.
    fn is_empty(&self) -> bool;

    /// Get the number of tags in this collection.
    fn len(&self) -> usize;

    /// Create an empty collection.
    fn empty() -> Self;

    /// Add a tag to this collection.
    fn add(&mut self, tag: &str);

    /// Remove a tag from this collection.
    fn remove(&mut self, tag: &str);
}
