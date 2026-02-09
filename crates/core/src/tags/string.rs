//! String-based Tag Collection
//!
//! A `Vec<String>`-based implementation of [`TagCollection`] for simple or test operations.

use std::{
    cmp::Ordering,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign},
    string::ToString,
};

use smallvec::SmallVec;

use crate::tags::collection::TagCollection;

/// A string-based tag collection using `SmallVec<[String; 5]>` for simple operations.
#[derive(Debug, Clone, PartialEq)]
pub struct StringTagCollection {
    tags: SmallVec<[String; 5]>,
}

impl StringTagCollection {
    /// Create a new string tag collection from a vector of strings.
    #[must_use]
    pub fn new(tags: SmallVec<[String; 5]>) -> Self {
        let mut collection = Self { tags };

        collection.tags.sort();
        collection.tags.dedup();

        collection
    }

    /// Create a new string tag collection from string slices.
    pub fn from_strs(tags: &[&str]) -> Self {
        Self::new(
            tags.iter()
                .map(ToString::to_string)
                .collect::<SmallVec<[String; 5]>>(),
        )
    }

    /// Convert the tag collection to a vector of strings.
    #[must_use]
    pub fn to_strs(&self) -> SmallVec<[String; 5]> {
        self.tags.clone()
    }
}

impl TagCollection for StringTagCollection {
    fn empty() -> Self {
        Self {
            tags: SmallVec::with_capacity(0),
        }
    }

    fn intersects(&self, other: &Self) -> bool {
        // Use two pointers approach on sorted vectors for O(n + m) performance.
        let mut left = self.tags.iter();
        let mut right = other.tags.iter();
        let mut left_tag = left.next();
        let mut right_tag = right.next();

        while let (Some(left_tag_ref), Some(right_tag_ref)) = (left_tag, right_tag) {
            match left_tag_ref.cmp(right_tag_ref) {
                Ordering::Equal => return true,
                Ordering::Less => left_tag = left.next(),
                Ordering::Greater => right_tag = right.next(),
            }
        }

        false
    }

    fn intersection(&self, other: &Self) -> Self {
        let mut result = SmallVec::new();
        let mut left = self.tags.iter();
        let mut right = other.tags.iter();
        let mut left_tag = left.next();
        let mut right_tag = right.next();

        while let (Some(left_tag_ref), Some(right_tag_ref)) = (left_tag, right_tag) {
            match left_tag_ref.cmp(right_tag_ref) {
                Ordering::Equal => {
                    result.push(left_tag_ref.clone());
                    left_tag = left.next();
                    right_tag = right.next();
                }
                Ordering::Less => left_tag = left.next(),
                Ordering::Greater => right_tag = right.next(),
            }
        }

        Self { tags: result }
    }

    fn contains(&self, tag: &str) -> bool {
        self.tags.binary_search(&tag.to_string()).is_ok()
    }

    fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    fn len(&self) -> usize {
        self.tags.len()
    }

    fn add(&mut self, tag: &str) {
        let tag_string = tag.to_string();

        if let Err(pos) = self.tags.binary_search(&tag_string) {
            self.tags.insert(pos, tag_string);
        }
    }

    fn remove(&mut self, tag: &str) {
        let tag_string = tag.to_string();

        if let Ok(pos) = self.tags.binary_search(&tag_string) {
            self.tags.remove(pos);
        }
    }
}

impl BitAnd for StringTagCollection {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(&rhs)
    }
}

impl BitOr for StringTagCollection {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let capacity = self.tags.len().saturating_add(rhs.tags.len());
        let mut result = SmallVec::with_capacity(capacity);
        let mut left = self.tags.iter().peekable();
        let mut right = rhs.tags.iter().peekable();

        // Merge two sorted vectors (union).
        while let (Some(left_tag), Some(right_tag)) = (left.peek(), right.peek()) {
            match left_tag.cmp(right_tag) {
                Ordering::Less => {
                    if let Some(tag) = left.next() {
                        result.push(tag.clone());
                    }
                }
                Ordering::Greater => {
                    if let Some(tag) = right.next() {
                        result.push(tag.clone());
                    }
                }
                Ordering::Equal => {
                    if let Some(tag) = left.next() {
                        result.push(tag.clone());
                    }
                    right.next();
                }
            }
        }

        result.extend(left.cloned());
        result.extend(right.cloned());

        Self { tags: result }
    }
}

impl BitXor for StringTagCollection {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        let mut result = SmallVec::new();
        let mut left = self.tags.iter().peekable();
        let mut right = rhs.tags.iter().peekable();

        // Symmetric difference (elements in either but not both)
        while let (Some(left_tag), Some(right_tag)) = (left.peek(), right.peek()) {
            match left_tag.cmp(right_tag) {
                Ordering::Less => {
                    if let Some(tag) = left.next() {
                        result.push(tag.clone());
                    }
                }
                Ordering::Greater => {
                    if let Some(tag) = right.next() {
                        result.push(tag.clone());
                    }
                }
                Ordering::Equal => {
                    // Skip elements that are in both
                    left.next();
                    right.next();
                }
            }
        }

        result.extend(left.cloned());
        result.extend(right.cloned());

        Self { tags: result }
    }
}

impl BitAndAssign for StringTagCollection {
    fn bitand_assign(&mut self, rhs: Self) {
        let mut result = SmallVec::new();
        let mut left = std::mem::take(&mut self.tags).into_iter().peekable();
        let mut right = rhs.tags.into_iter().peekable();

        while let (Some(left_tag), Some(right_tag)) = (left.peek(), right.peek()) {
            match left_tag.cmp(right_tag) {
                Ordering::Equal => {
                    if let Some(tag) = left.next() {
                        result.push(tag);
                    }
                    right.next();
                }
                Ordering::Less => {
                    left.next();
                }
                Ordering::Greater => {
                    right.next();
                }
            }
        }

        self.tags = result;
    }
}

impl BitOrAssign for StringTagCollection {
    fn bitor_assign(&mut self, rhs: Self) {
        // For union, we can use the fact that both vectors are sorted.
        let left_len = self.tags.len();
        let right_len = rhs.tags.len();
        let capacity = left_len.saturating_add(right_len);
        let mut result = SmallVec::with_capacity(capacity);
        let mut left = std::mem::take(&mut self.tags).into_iter().peekable();
        let mut right = rhs.tags.into_iter().peekable();

        while let (Some(left_tag), Some(right_tag)) = (left.peek(), right.peek()) {
            match left_tag.cmp(right_tag) {
                Ordering::Less => {
                    if let Some(tag) = left.next() {
                        result.push(tag);
                    }
                }
                Ordering::Greater => {
                    if let Some(tag) = right.next() {
                        result.push(tag);
                    }
                }
                Ordering::Equal => {
                    if let Some(tag) = left.next() {
                        result.push(tag);
                    }
                    right.next();
                }
            }
        }

        result.extend(left);
        result.extend(right);

        self.tags = result;
    }
}

impl BitXorAssign for StringTagCollection {
    fn bitxor_assign(&mut self, rhs: Self) {
        let left_len = self.tags.len();
        let right_len = rhs.tags.len();
        let capacity = left_len.saturating_add(right_len);
        let mut result = SmallVec::with_capacity(capacity);
        let mut left = std::mem::take(&mut self.tags).into_iter().peekable();
        let mut right = rhs.tags.into_iter().peekable();

        // Symmetric difference (elements in either but not both).
        while let (Some(left_tag), Some(right_tag)) = (left.peek(), right.peek()) {
            match left_tag.cmp(right_tag) {
                Ordering::Less => {
                    if let Some(tag) = left.next() {
                        result.push(tag);
                    }
                }
                Ordering::Greater => {
                    if let Some(tag) = right.next() {
                        result.push(tag);
                    }
                }
                Ordering::Equal => {
                    // Skip elements that are in both.
                    left.next();
                    right.next();
                }
            }
        }

        result.extend(left);
        result.extend(right);

        self.tags = result;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_collection_intersection_works() {
        let tags1 = StringTagCollection::from_strs(&["food", "fruit", "red"]);
        let tags2 = StringTagCollection::from_strs(&["food", "vegetable", "green"]);
        let tags3 = StringTagCollection::from_strs(&["electronics", "gadget"]);

        assert!(tags1.intersects(&tags2));
        assert!(!tags1.intersects(&tags3));
        assert!(!tags2.intersects(&tags3));

        let intersection = tags1.intersection(&tags2);
        assert_eq!(intersection.len(), 1);
        assert!(intersection.contains("food"));
    }

    #[test]
    fn string_collection_contains_works() {
        let tags = StringTagCollection::from_strs(&["food", "fruit", "red"]);

        assert!(tags.contains("food"));
        assert!(tags.contains("fruit"));
        assert!(tags.contains("red"));
        assert!(!tags.contains("vegetable"));
    }

    #[test]
    fn string_collection_add_remove_works() {
        let mut tags = StringTagCollection::from_strs(&["food", "fruit"]);

        assert_eq!(tags.len(), 2);
        assert!(!tags.contains("red"));
        assert!(!tags.is_empty());

        tags.add("red");
        assert_eq!(tags.len(), 3);
        assert!(tags.contains("red"));
        assert!(!tags.is_empty());

        tags.remove("fruit");
        assert_eq!(tags.len(), 2);
        assert!(!tags.contains("fruit"));
        assert!(!tags.is_empty());
    }

    #[test]
    fn string_collection_is_empty_works() {
        let empty = StringTagCollection::empty();
        assert!(empty.is_empty());

        let empty_from_strs = StringTagCollection::from_strs(&[]);
        assert!(empty_from_strs.is_empty());

        let tags = StringTagCollection::from_strs(&["food"]);
        assert!(!tags.is_empty());
    }

    #[test]
    fn string_collection_deduplicates_tags() {
        let tags = StringTagCollection::from_strs(&["food", "fruit", "food", "red", "fruit"]);

        assert_eq!(tags.len(), 3);
        assert!(tags.contains("food"));
        assert!(tags.contains("fruit"));
        assert!(tags.contains("red"));
    }

    #[test]
    fn string_collection_maintains_sorted_order() {
        let tags = StringTagCollection::from_strs(&["zebra", "apple", "banana"]);

        // The internal vector should be sorted
        assert_eq!(tags.tags, ["apple", "banana", "zebra"].into());
    }

    #[test]
    fn string_collection_bitwise_and_intersection() {
        let tags1 = StringTagCollection::from_strs(&["a", "b", "c"]);
        let tags2 = StringTagCollection::from_strs(&["b", "c", "d"]);

        let result = tags1 & tags2;
        assert_eq!(result.len(), 2);
        assert!(result.contains("b"));
        assert!(result.contains("c"));
        assert!(!result.contains("a"));
        assert!(!result.contains("d"));
    }

    #[test]
    fn string_collection_bitwise_or_union() {
        let tags1 = StringTagCollection::from_strs(&["a", "b"]);
        let tags2 = StringTagCollection::from_strs(&["c", "d"]);

        let result = tags1 | tags2;
        assert_eq!(result.len(), 4);
        assert!(result.contains("a"));
        assert!(result.contains("b"));
        assert!(result.contains("c"));
        assert!(result.contains("d"));
    }

    #[test]
    fn string_collection_bitwise_or_with_overlap() {
        let tags1 = StringTagCollection::from_strs(&["a", "b", "c"]);
        let tags2 = StringTagCollection::from_strs(&["b", "c", "d"]);

        let result = tags1 | tags2;
        assert_eq!(result.len(), 4);
        assert!(result.contains("a"));
        assert!(result.contains("b"));
        assert!(result.contains("c"));
        assert!(result.contains("d"));
    }

    #[test]
    fn string_collection_bitwise_or_prefers_right_when_smaller() {
        let tags1 = StringTagCollection::from_strs(&["b", "d"]);
        let tags2 = StringTagCollection::from_strs(&["a", "c"]);

        let result = tags1 | tags2;
        assert_eq!(result.tags, ["a", "b", "c", "d"].into());
    }

    #[test]
    fn string_collection_bitwise_xor_symmetric_difference() {
        let tags1 = StringTagCollection::from_strs(&["a", "b", "c"]);
        let tags2 = StringTagCollection::from_strs(&["b", "c", "d"]);

        let result = tags1 ^ tags2;
        assert_eq!(result.len(), 2);
        assert!(result.contains("a"));
        assert!(result.contains("d"));
        assert!(!result.contains("b"));
        assert!(!result.contains("c"));
    }

    #[test]
    fn string_collection_bitwise_xor_prefers_right_when_smaller() {
        let tags1 = StringTagCollection::from_strs(&["b"]);
        let tags2 = StringTagCollection::from_strs(&["a"]);

        let result = tags1 ^ tags2;
        assert_eq!(result.tags, ["a", "b"].into());
    }

    #[test]
    fn string_collection_bitwise_assign_operations() {
        let tags1 = StringTagCollection::from_strs(&["a", "b"]);
        let tags2 = StringTagCollection::from_strs(&["b", "c"]);

        // Test &=
        let mut and = tags1.clone();
        and &= tags2.clone();
        assert_eq!(and.len(), 1);
        assert!(and.contains("b"));

        // Test |=
        let mut or = tags1.clone();
        or |= tags2.clone();
        assert_eq!(or.len(), 3);
        assert!(or.contains("a"));
        assert!(or.contains("b"));
        assert!(or.contains("c"));

        // Test ^=
        let mut xor = tags1.clone();
        xor ^= tags2.clone();
        assert_eq!(xor.len(), 2);
        assert!(xor.contains("a"));
        assert!(xor.contains("c"));
        assert!(!xor.contains("b"));
    }

    #[test]
    fn string_collection_bitwise_assign_advances_right_when_smaller() {
        let mut and = StringTagCollection::from_strs(&["c"]);
        let and_rhs = StringTagCollection::from_strs(&["a", "c"]);
        and &= and_rhs;
        assert_eq!(and.tags, ["c"].into());

        let mut or = StringTagCollection::from_strs(&["c"]);
        let or_rhs = StringTagCollection::from_strs(&["a", "c"]);
        or |= or_rhs;
        assert_eq!(or.tags, ["a", "c"].into());

        let mut xor = StringTagCollection::from_strs(&["c"]);
        let xor_rhs = StringTagCollection::from_strs(&["a"]);
        xor ^= xor_rhs;
        assert_eq!(xor.tags, ["a", "c"].into());
    }

    #[test]
    fn string_collection_assign_operations_preserve_efficiency() {
        // This test verifies that assignment operations work correctly
        // and don't break the sorted order invariant
        let mut tags1 = StringTagCollection::from_strs(&["apple", "banana", "cherry"]);
        let tags2 = StringTagCollection::from_strs(&["banana", "date", "elderberry"]);

        // Test &= preserves sorted order
        tags1 &= tags2.clone();
        assert_eq!(tags1.tags, ["banana"].into());
        assert_eq!(tags1.len(), 1);

        // Reset and test |=
        let mut tags1 = StringTagCollection::from_strs(&["apple", "banana"]);
        tags1 |= tags2.clone();
        assert_eq!(tags1.tags, ["apple", "banana", "date", "elderberry"].into());
        assert_eq!(tags1.len(), 4);

        // Reset and test ^=
        let mut tags1 = StringTagCollection::from_strs(&["apple", "banana", "cherry"]);
        tags1 ^= tags2;
        assert_eq!(tags1.tags, ["apple", "cherry", "date", "elderberry"].into());
        assert_eq!(tags1.len(), 4);
    }
}
