//! Promotion Qualification Rules
//!
//! Nested boolean tag qualification rules used by promotions and slots.

use smallvec::{SmallVec, smallvec};

use crate::tags::{collection::TagCollection, string::StringTagCollection};

/// Qualification expression for item-tag matching.
#[derive(Debug, Clone)]
pub struct Qualification<T: TagCollection = StringTagCollection> {
    /// How `rules` are combined.
    pub op: BoolOp,

    /// Child rules. Empty means "match all items".
    pub rules: SmallVec<[QualificationRule<T>; 2]>,
}

/// Boolean operation used to combine qualification rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    /// All child rules must match.
    And,

    /// At least one child rule must match.
    Or,
}

/// Single qualification rule.
#[derive(Debug, Clone)]
pub enum QualificationRule<T: TagCollection = StringTagCollection> {
    /// Item must have all listed tags.
    HasAll {
        /// Tag set used by this rule.
        tags: T,
    },

    /// Item must have at least one listed tag.
    HasAny {
        /// Tag set used by this rule.
        tags: T,
    },

    /// Item must have none of the listed tags.
    HasNone {
        /// Tag set used by this rule.
        tags: T,
    },

    /// Nested qualification group.
    Group(Box<Qualification<T>>),
}

impl<T: TagCollection> Qualification<T> {
    /// Create a qualification from operator and rules.
    #[must_use]
    pub fn new(op: BoolOp, rules: SmallVec<[QualificationRule<T>; 2]>) -> Self {
        Self { op, rules }
    }

    /// Match all items.
    #[must_use]
    pub fn match_all() -> Self {
        Self {
            op: BoolOp::And,
            rules: SmallVec::new(),
        }
    }

    /// Create a qualification that just matches any item's tags.
    #[must_use]
    pub fn match_any(tags: T) -> Self {
        if tags.is_empty() {
            return Self::match_all();
        }

        Self {
            op: BoolOp::And,
            rules: smallvec![QualificationRule::HasAny { tags }],
        }
    }

    /// Evaluate the qualification against an item's tags.
    #[must_use]
    pub fn matches(&self, item_tags: &T) -> bool {
        if self.rules.is_empty() {
            return true;
        }

        match self.op {
            BoolOp::And => self.rules.iter().all(|rule| rule.matches(item_tags)),
            BoolOp::Or => self.rules.iter().any(|rule| rule.matches(item_tags)),
        }
    }
}

impl<T: TagCollection> Default for Qualification<T> {
    fn default() -> Self {
        Self::match_all()
    }
}

impl<T: TagCollection> QualificationRule<T> {
    #[must_use]
    fn matches(&self, item_tags: &T) -> bool {
        match self {
            Self::HasAll { tags } => {
                if tags.is_empty() {
                    return true;
                }

                item_tags.intersection(tags).len() == tags.len()
            }
            Self::HasAny { tags } => !tags.is_empty() && item_tags.intersects(tags),
            Self::HasNone { tags } => tags.is_empty() || !item_tags.intersects(tags),
            Self::Group(group) => group.matches(item_tags),
        }
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use crate::tags::string::StringTagCollection;

    use super::*;

    #[test]
    fn empty_qualification_matches_all() {
        let qualification = Qualification::<StringTagCollection>::default();
        let tags = StringTagCollection::from_strs(&["peak", "snack"]);

        assert!(qualification.matches(&tags));
    }

    #[test]
    fn supports_nested_boolean_groups() {
        let qualification = Qualification::new(
            BoolOp::And,
            smallvec![
                QualificationRule::HasAll {
                    tags: StringTagCollection::from_strs(&["peak", "snack"])
                },
                QualificationRule::Group(Box::new(Qualification::new(
                    BoolOp::Or,
                    smallvec![
                        QualificationRule::HasAny {
                            tags: StringTagCollection::from_strs(&["member", "staff"])
                        },
                        QualificationRule::HasNone {
                            tags: StringTagCollection::from_strs(&["excluded"])
                        }
                    ]
                )))
            ],
        );

        assert!(qualification.matches(&StringTagCollection::from_strs(&[
            "peak", "snack", "member"
        ])));
        assert!(qualification.matches(&StringTagCollection::from_strs(&["peak", "snack"])));
        assert!(!qualification.matches(&StringTagCollection::from_strs(&["peak", "member"])));
        assert!(!qualification.matches(&StringTagCollection::from_strs(&[
            "peak", "snack", "excluded"
        ])));
    }
}
