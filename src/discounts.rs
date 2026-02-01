//! Discounts

use std::fmt;

use percentage::PercentageDecimal;
use rust_decimal::{
    Decimal, RoundingStrategy,
    prelude::{FromPrimitive, ToPrimitive},
};
use rusty_money::{Money, MoneyError, iso::Currency};

use thiserror::Error;

use crate::{
    items::{Item, cheapest_item},
    pricing::{TotalPriceError, total_price},
};

/// Errors specific to discount calculations.
#[derive(Debug, Error)]
pub enum DiscountError {
    /// No items provided, so currency cannot be determined.
    #[error("no items provided; cannot determine currency for discount")]
    NoItems,

    /// Percentage calculation could not be safely converted.
    #[error("percentage conversion overflowed or was not finite")]
    PercentConversion,

    /// Wrapped money arithmetic or currency mismatch error.
    #[error(transparent)]
    Money(#[from] MoneyError),

    /// Errors bubbled up from total price calculation.
    #[error(transparent)]
    TotalPrice(#[from] TotalPriceError),
}

/// Represents a single, valid discount scenario.
pub enum Discount<'a> {
    /// Apply a percentage discount to the total price of all items.
    ///
    /// Discount every item by this percentage of the total.
    PercentageDiscountAllItems(PercentageDecimal),

    /// Apply a percentage discount to the price of the cheapest item.
    ///
    /// Discount only the cheapest item by this percentage of its price.
    PercentageDiscountCheapestItem(PercentageDecimal),

    /// Override the total price of all items with a fixed price.
    PriceOverrideAllItems(Money<'a, Currency>),

    /// Override just the price of the cheapest item with a fixed price.
    PriceOverrideCheapestItem(Money<'a, Currency>),
}

impl fmt::Debug for Discount<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Discount::PercentageDiscountAllItems(p) => f
                .debug_tuple("PercentageDiscountAllItems")
                .field(&p.value())
                .finish(),
            Discount::PercentageDiscountCheapestItem(p) => f
                .debug_tuple("PercentageDiscountCheapestItem")
                .field(&p.value())
                .finish(),
            Discount::PriceOverrideAllItems(m) => {
                f.debug_tuple("PriceOverrideAllItems").field(m).finish()
            }
            Discount::PriceOverrideCheapestItem(m) => {
                f.debug_tuple("PriceOverrideCheapestItem").field(m).finish()
            }
        }
    }
}

/// Calculates the discounted price for a set of items.
///
/// # Errors
///
/// Returns an error if:
/// - `items` is empty and the discount needs an item currency (`DiscountError::NoItems`).
/// - a percentage calculation cannot be safely represented in minor units
///   (`DiscountError::PercentConversion`).
/// - underlying money arithmetic fails (for example, due to currency mismatch)
///   (`DiscountError::Money`).
pub fn calculate_discount<'a>(
    discount: &Discount<'a>,
    items: &'a [Item<'a>],
) -> Result<Money<'a, Currency>, DiscountError> {
    match discount {
        Discount::PriceOverrideAllItems(price) => {
            ensure_not_empty(items)?;
            Ok(*price)
        }
        Discount::PriceOverrideCheapestItem(price) => {
            let (total, cheapest) = totals_with_cheapest(items)?;

            Ok(total.sub(*cheapest.price())?.add(*price)?)
        }
        Discount::PercentageDiscountAllItems(percent) => {
            let (total, cheapest) = totals_with_cheapest(items)?;
            let discount_money = discount_on(cheapest.price(), percent)?;

            Ok(total.sub(discount_money)?)
        }
        Discount::PercentageDiscountCheapestItem(percent) => {
            let (total, cheapest) = totals_with_cheapest(items)?;
            let discount_money = discount_on(cheapest.price(), percent)?;

            Ok(total
                .sub(*cheapest.price())?
                .add(cheapest.price().sub(discount_money)?)?)
        }
    }
}

/// Return `NoItems` if the slice is empty.
fn ensure_not_empty(items: &[Item<'_>]) -> Result<(), DiscountError> {
    if items.is_empty() {
        Err(DiscountError::NoItems)
    } else {
        Ok(())
    }
}

/// Fetch the cheapest item or surface `NoItems`.
fn require_cheapest<'a>(items: &'a [Item<'a>]) -> Result<&'a Item<'a>, DiscountError> {
    cheapest_item(items).ok_or(DiscountError::NoItems)
}

/// Compute total once and return it alongside the cheapest item.
fn totals_with_cheapest<'a>(
    items: &'a [Item<'a>],
) -> Result<(Money<'a, Currency>, &'a Item<'a>), DiscountError> {
    let cheapest = require_cheapest(items)?;
    let total = total_price(items)?;

    Ok((total, cheapest))
}

/// Calculate the discount amount on a price for a percentage.
fn discount_on<'a>(
    price: &Money<'a, Currency>,
    percent: &PercentageDecimal,
) -> Result<Money<'a, Currency>, DiscountError> {
    let discount_minor = percent_of_minor(percent, price.to_minor_units())?;

    Ok(Money::from_minor(discount_minor, price.currency()))
}

/// Calculate the discount amount in minor units based on a percentage and a minor unit amount.
fn percent_of_minor(percent: &PercentageDecimal, minor: i64) -> Result<i64, DiscountError> {
    let Some(percent) = Decimal::from_f64_retain(percent.value()) else {
        return Err(DiscountError::PercentConversion);
    };

    let Some(minor) = Decimal::from_i64(minor) else {
        unreachable!("always returns `Some` for every `i64`")
    };

    let Some(applied) = percent.checked_mul(minor) else {
        return Err(DiscountError::PercentConversion);
    };

    let rounded = applied.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
    let Some(rounded) = rounded.to_i64() else {
        return Err(DiscountError::PercentConversion);
    };

    Ok(rounded)
}

#[cfg(test)]
mod tests {
    use percentage::{Percentage, PercentageDecimal};
    use rusty_money::iso::GBP;
    use testresult::TestResult;

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(Money::from_minor(100, GBP)),
            Item::new(Money::from_minor(200, GBP)),
            Item::new(Money::from_minor(300, GBP)),
        ]
    }

    #[test]
    fn calculate_price_override_all_items() -> TestResult {
        let items = test_items();
        let discount = Discount::PriceOverrideAllItems(Money::from_minor(50, GBP));
        let discounted_price = calculate_discount(&discount, &items)?;

        assert_eq!(discounted_price, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn calculate_price_override_cheapest_item() -> TestResult {
        let items = test_items();
        let discount = Discount::PriceOverrideCheapestItem(Money::from_minor(50, GBP));
        let discounted_price = calculate_discount(&discount, &items)?;

        assert_eq!(discounted_price, Money::from_minor(550, GBP));

        Ok(())
    }

    #[test]
    fn calculate_percentage_all_items() -> TestResult {
        let items = test_items();
        let discount = Discount::PercentageDiscountAllItems(Percentage::from_decimal(0.25));
        let discounted_price = calculate_discount(&discount, &items)?;

        assert_eq!(discounted_price, Money::from_minor(575, GBP));

        Ok(())
    }

    #[test]
    fn calculate_percentage_cheapest_item() -> TestResult {
        let items = test_items();
        let discount = Discount::PercentageDiscountCheapestItem(Percentage::from_decimal(0.5));
        let discounted_price = calculate_discount(&discount, &items)?;

        assert_eq!(discounted_price, Money::from_minor(550, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discount_returns_no_items_error() {
        let items: [Item<'static>; 0] = [];

        let price_override_all = Discount::PriceOverrideAllItems(Money::from_minor(50, GBP));

        let price_override_cheapest =
            Discount::PriceOverrideCheapestItem(Money::from_minor(50, GBP));

        let percent_all = Discount::PercentageDiscountAllItems(Percentage::from_decimal(0.25));

        let percent_cheapest =
            Discount::PercentageDiscountCheapestItem(Percentage::from_decimal(0.25));

        assert!(matches!(
            calculate_discount(&price_override_all, &items),
            Err(DiscountError::NoItems)
        ));
        assert!(matches!(
            calculate_discount(&price_override_cheapest, &items),
            Err(DiscountError::NoItems)
        ));
        assert!(matches!(
            calculate_discount(&percent_all, &items),
            Err(DiscountError::NoItems)
        ));
        assert!(matches!(
            calculate_discount(&percent_cheapest, &items),
            Err(DiscountError::NoItems)
        ));
    }

    #[test]
    fn discount_debug_includes_variant_names() {
        let price = Money::from_minor(50, GBP);

        let all = format!(
            "{:?}",
            Discount::PercentageDiscountAllItems(Percentage::from_decimal(0.25))
        );

        let cheapest = format!(
            "{:?}",
            Discount::PercentageDiscountCheapestItem(Percentage::from_decimal(0.25))
        );

        let override_all = format!("{:?}", Discount::PriceOverrideAllItems(price));
        let override_cheapest = format!("{:?}", Discount::PriceOverrideCheapestItem(price));

        assert!(all.contains("PercentageDiscountAllItems"));
        assert!(cheapest.contains("PercentageDiscountCheapestItem"));
        assert!(override_all.contains("PriceOverrideAllItems"));
        assert!(override_cheapest.contains("PriceOverrideCheapestItem"));
    }

    #[test]
    fn percent_of_minor_nan_returns_error() {
        let percent = Percentage::from_decimal(f64::NAN);
        let result = percent_of_minor(&percent, 100);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn percent_of_minor_overflow_returns_error() {
        // SAFETY: PercentageDecimal is a single-field wrapper around f64.
        //         We construct an out-of-range percentage purely to
        //         exercise overflow handling in tests.
        let percent = unsafe { std::mem::transmute::<f64, PercentageDecimal>(2.0) };

        let result = percent_of_minor(&percent, i64::MAX);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn percent_of_minor_checked_mul_overflow_returns_error() {
        // SAFETY: PercentageDecimal is a single-field wrapper around f64.
        //         1e20 is representable as a Decimal, but multiplying by a
        //         very large minor value should overflow the Decimal range.
        let percent = unsafe { std::mem::transmute::<f64, PercentageDecimal>(1e20) };

        let result = percent_of_minor(&percent, i64::MAX);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn percent_of_minor_underflow_returns_error() {
        // SAFETY: PercentageDecimal is a single-field wrapper around f64.
        //         We construct an out-of-range percentage purely to
        //         exercise underflow handling in tests.
        let percent = unsafe { std::mem::transmute::<f64, PercentageDecimal>(2.0) };

        let result = percent_of_minor(&percent, i64::MIN);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn discount_on_returns_expected_amount() -> TestResult {
        let price = Money::from_minor(200, GBP);
        let percent = Percentage::from_decimal(0.25);

        let discount = discount_on(&price, &percent)?;

        assert_eq!(discount, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn totals_with_cheapest_returns_total_and_item() -> TestResult {
        let items = test_items();

        let (total, cheapest) = totals_with_cheapest(&items)?;

        assert_eq!(total, Money::from_minor(600, GBP));
        assert_eq!(cheapest.price(), &Money::from_minor(100, GBP));

        Ok(())
    }
}
