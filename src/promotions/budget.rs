//! Promotion Budget Constraints

use rusty_money::{Money, iso::Currency};

/// Budget constraints for a promotion
#[derive(Debug, Clone, Copy, Default)]
pub struct PromotionBudget<'a> {
    /// Maximum number of applications (items or bundles depending on promotion type)
    pub application_limit: Option<u32>,

    /// Maximum total discount value (original - discounted)
    pub monetary_limit: Option<Money<'a, Currency>>,
}

impl<'a> PromotionBudget<'a> {
    /// Create a budget with no constraints
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            application_limit: None,
            monetary_limit: None,
        }
    }

    /// Create a budget with application limit only
    #[must_use]
    pub const fn with_application_limit(limit: u32) -> Self {
        Self {
            application_limit: Some(limit),
            monetary_limit: None,
        }
    }

    /// Create a budget with monetary limit only
    #[must_use]
    pub const fn with_monetary_limit(limit: Money<'a, Currency>) -> Self {
        Self {
            application_limit: None,
            monetary_limit: Some(limit),
        }
    }

    /// Create a budget with both limits
    #[must_use]
    pub const fn with_both_limits(instance: u32, monetary: Money<'a, Currency>) -> Self {
        Self {
            application_limit: Some(instance),
            monetary_limit: Some(monetary),
        }
    }

    /// Check if this budget has any constraints
    #[must_use]
    pub const fn has_constraints(&self) -> bool {
        self.application_limit.is_some() || self.monetary_limit.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_money::iso;

    #[test]
    fn test_unlimited_budget() {
        let budget = PromotionBudget::unlimited();

        assert!(!budget.has_constraints());
        assert!(budget.application_limit.is_none());
        assert!(budget.monetary_limit.is_none());
    }

    #[test]
    fn test_application_limit_only() {
        let budget = PromotionBudget::with_application_limit(5);

        assert!(budget.has_constraints());
        assert_eq!(budget.application_limit, Some(5));
        assert!(budget.monetary_limit.is_none());
    }

    #[test]
    fn test_monetary_limit_only() {
        let limit = Money::from_minor(1000, iso::GBP);
        let budget = PromotionBudget::with_monetary_limit(limit);

        assert!(budget.has_constraints());
        assert!(budget.application_limit.is_none());
        assert_eq!(budget.monetary_limit, Some(limit));
    }

    #[test]
    fn test_both_limits() {
        let limit = Money::from_minor(1000, iso::GBP);
        let budget = PromotionBudget::with_both_limits(5, limit);

        assert!(budget.has_constraints());
        assert_eq!(budget.application_limit, Some(5));
        assert_eq!(budget.monetary_limit, Some(limit));
    }
}
