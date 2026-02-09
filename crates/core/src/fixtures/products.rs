//! Product Fixtures

use decimal_percentage::Percentage;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use rustc_hash::FxHashMap;
use rusty_money::{
    Money,
    iso::{Currency, EUR, GBP, USD},
};
use serde::Deserialize;

use crate::{fixtures::FixtureError, products::Product, tags::string::StringTagCollection};

/// Wrapper for products in YAML
#[derive(Debug, Deserialize)]
pub struct ProductsFixture {
    /// Map of product key -> product fixture
    pub products: FxHashMap<String, ProductFixture>,
}

/// Product Fixture
#[derive(Debug, Deserialize)]
pub struct ProductFixture {
    /// Product name
    pub name: String,

    /// Product tags
    pub tags: Vec<String>,

    /// Product price (e.g., "299 GBP")
    pub price: String,
}

impl TryFrom<ProductFixture> for Product<'_> {
    type Error = FixtureError;

    fn try_from(fixture: ProductFixture) -> Result<Self, Self::Error> {
        let (minor_units, currency) = parse_price(&fixture.price)?;
        let price = Money::from_minor(minor_units, currency);

        let tag_refs: Vec<&str> = fixture.tags.iter().map(String::as_str).collect();
        let tags = StringTagCollection::from_strs(&tag_refs);

        Ok(Product {
            name: fixture.name,
            tags,
            price,
        })
    }
}

/// Parse price string (e.g., "2.99 GBP") into minor units and currency
///
/// # Errors
///
/// Returns an error if the string is not in the format "AMOUNT CURRENCY",
/// if the amount cannot be parsed as a float, or if the currency code
/// is not recognized.
pub fn parse_price(s: &str) -> Result<(i64, &'static Currency), FixtureError> {
    let parts: Vec<&str> = s.split_whitespace().collect();

    if parts.len() != 2 {
        return Err(FixtureError::InvalidPrice(format!(
            "Expected format 'AMOUNT CURRENCY', got: {s}"
        )));
    }

    let amount = parts
        .first()
        .ok_or_else(|| FixtureError::InvalidPrice(s.to_string()))?
        .parse::<Decimal>()
        .map_err(|_err| FixtureError::InvalidPrice(s.to_string()))?;

    let minor_units = amount
        .checked_mul(Decimal::new(100, 0))
        .and_then(|value| value.round_dp(0).to_i64())
        .ok_or_else(|| FixtureError::InvalidPrice(s.to_string()))?;

    let currency_code = parts
        .get(1)
        .ok_or_else(|| FixtureError::InvalidPrice(s.to_string()))?;

    let currency = match *currency_code {
        "GBP" => GBP,
        "USD" => USD,
        "EUR" => EUR,
        other => return Err(FixtureError::UnknownCurrency(other.to_string())),
    };

    Ok((minor_units, currency))
}

/// Parse percentage string (e.g., "15%" or "0.15") into a `Percentage`
///
/// Accepts two formats:
/// - Percentage format: "15%" for 15%
/// - Decimal format: "0.15" for 15%
///
/// # Errors
///
/// Returns an error if the string cannot be parsed or if the value is invalid.
pub fn parse_percentage(s: &str) -> Result<Percentage, FixtureError> {
    let trimmed = s.trim();

    if let Some(percent_str) = trimmed.strip_suffix('%') {
        // Parse as percentage (e.g., "15%" -> 0.15)
        let value = percent_str
            .trim()
            .parse::<f64>()
            .map_err(|_err| FixtureError::InvalidPercentage(s.to_string()))?;

        // Convert from percentage to decimal (15 -> 0.15)
        Ok(Percentage::from(value / 100.0))
    } else {
        // Parse as decimal (e.g., "0.15" -> 0.15)
        let value = trimmed
            .parse::<f64>()
            .map_err(|_err| FixtureError::InvalidPercentage(s.to_string()))?;

        Ok(Percentage::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_price_rejects_invalid_format() {
        let result = parse_price("2.99GBP");

        assert!(matches!(result, Err(FixtureError::InvalidPrice(_))));
    }

    #[test]
    fn parse_price_rejects_unknown_currency() {
        let result = parse_price("2.99 ABC");

        assert!(matches!(result, Err(FixtureError::UnknownCurrency(code)) if code == "ABC"));
    }

    #[test]
    fn parse_price_accepts_usd_and_eur() -> Result<(), FixtureError> {
        let (usd_minor, usd) = parse_price("1.00 USD")?;
        let (eur_minor, eur) = parse_price("2.50 EUR")?;

        assert_eq!(usd_minor, 100);
        assert_eq!(usd, USD);
        assert_eq!(eur_minor, 250);
        assert_eq!(eur, EUR);

        Ok(())
    }

    #[test]
    fn parse_percentage_accepts_percentage_format() -> Result<(), FixtureError> {
        let percent = parse_percentage("15%")?;

        assert_eq!(percent, Percentage::from(0.15));

        Ok(())
    }

    #[test]
    fn parse_percentage_accepts_decimal_format() -> Result<(), FixtureError> {
        let percent = parse_percentage("0.15")?;

        assert_eq!(percent, Percentage::from(0.15));

        Ok(())
    }

    #[test]
    fn parse_percentage_accepts_100_percent() -> Result<(), FixtureError> {
        let percent = parse_percentage("100%")?;

        assert_eq!(percent, Percentage::from(1.0));

        Ok(())
    }

    #[test]
    fn parse_percentage_accepts_one_as_decimal() -> Result<(), FixtureError> {
        let percent = parse_percentage("1")?;

        assert_eq!(percent, Percentage::from(1.0));

        Ok(())
    }

    #[test]
    fn parse_percentage_rejects_invalid_format() {
        let result = parse_percentage("invalid");

        assert!(matches!(result, Err(FixtureError::InvalidPercentage(_))));
    }

    #[test]
    fn parse_percentage_handles_whitespace() -> Result<(), FixtureError> {
        let percent = parse_percentage("  15%  ")?;

        assert_eq!(percent, Percentage::from(0.15));

        Ok(())
    }
}
