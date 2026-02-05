//! Receipt

use comfy_table::{Attribute, Cell, Color, Table};
use decimal_percentage::Percentage;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use rustc_hash::FxHashMap;
use rusty_money::{Money, MoneyError, iso::Currency};
use slotmap::SlotMap;
use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    basket::Basket,
    pricing::TotalPriceError,
    products::{Product, ProductKey},
    promotions::{PromotionKey, PromotionMeta, applications::PromotionApplication},
    solvers::SolverResult,
};

/// Errors that can occur when building a receipt.
#[derive(Debug, Error)]
pub enum ReceiptError {
    /// Error calculating total price from basket items.
    #[error(transparent)]
    TotalPrice(#[from] TotalPriceError),

    /// Wrapper for money errors.
    #[error(transparent)]
    Money(#[from] MoneyError),

    /// Error finding a product in the product catalog.
    #[error("Missing product")]
    MissingProduct(ProductKey),

    /// IO error
    #[error("IO error")]
    IO,
}

/// Final receipt for a processed basket.
#[derive(Debug, Clone)]
pub struct Receipt<'a> {
    /// Indexes of items in the basket that were purchased at full price, not in any promotion
    full_price_items: SmallVec<[usize; 10]>,

    /// Promotion application details keyed by basket item index.
    ///
    /// We enforce "each item can be in at most 1 promotion", which makes this a convenient
    /// structure for rendering and lookups.
    promotion_applications: FxHashMap<usize, PromotionApplication<'a>>,

    /// Total cost before any promotion applications
    subtotal: Money<'a, Currency>,

    /// Total amount paid for all items after any promotion applications
    total: Money<'a, Currency>,

    /// Currency used for all monetary values
    currency: &'static Currency,
}

impl<'a> Receipt<'a> {
    /// Create a new receipt with the given details.
    pub fn new(
        full_price_items: SmallVec<[usize; 10]>,
        promotion_applications: FxHashMap<usize, PromotionApplication<'a>>,
        subtotal: Money<'a, Currency>,
        total: Money<'a, Currency>,
        currency: &'static Currency,
    ) -> Self {
        Self {
            full_price_items,
            promotion_applications,
            subtotal,
            total,
            currency,
        }
    }

    /// Total cost before any promotion applications
    pub fn subtotal(&self) -> Money<'a, Currency> {
        self.subtotal
    }

    /// Total amount paid for all items
    pub fn total(&self) -> Money<'a, Currency> {
        self.total
    }

    /// Calculate the savings made by applying promotions.
    ///
    /// # Errors
    ///
    /// Returns a [`MoneyError`] if the subtraction operation fails.
    pub fn savings(&self) -> Result<Money<'a, Currency>, MoneyError> {
        self.subtotal.sub(self.total)
    }

    /// Calculates the savings made by applying the promotions as a percentage
    ///
    /// # Errors
    ///
    /// Returns a [`MoneyError`] if the subtraction operation fails.
    pub fn savings_percent(&self) -> Result<Percentage, MoneyError> {
        let savings = self.savings()?;
        let subtotal = self.subtotal();

        // Percent savings is relative to the original (pre-discount) subtotal.
        // Avoid integer division truncation by doing the ratio in decimal space.
        let savings_minor = savings.to_minor_units();
        let subtotal_minor = subtotal.to_minor_units();

        if subtotal_minor == 0 {
            return Ok(Percentage::from(0.0));
        }

        let savings_dec = Decimal::from_i64(savings_minor).unwrap_or(Decimal::ZERO);
        let subtotal_dec = Decimal::from_i64(subtotal_minor).unwrap_or(Decimal::ZERO);

        Ok(Percentage::from(savings_dec / subtotal_dec))
    }

    /// Build a receipt from a basket and solver result.
    ///
    /// # Errors
    ///
    /// Returns a [`ReceiptError`] if the basket subtotal cannot be calculated.
    pub fn from_solver_result(
        basket: &'a Basket<'a>,
        result: SolverResult<'a>,
    ) -> Result<Self, ReceiptError> {
        let mut promotion_applications = FxHashMap::default();
        for app in result.promotion_applications {
            // Solver invariants say this can't happen, but map insertion makes it explicit.
            debug_assert!(
                !promotion_applications.contains_key(&app.item_idx),
                "duplicate promotion application for item_idx={}",
                app.item_idx
            );
            promotion_applications.insert(app.item_idx, app);
        }

        Ok(Receipt {
            full_price_items: result.unaffected_items,
            promotion_applications,
            subtotal: basket.subtotal()?,
            total: result.total,
            currency: basket.currency(),
        })
    }

    /// Indexes of items purchased at full price (not in any promotion).
    pub fn full_price_items(&self) -> &[usize] {
        &self.full_price_items
    }

    /// Promotion application details keyed by basket item index.
    pub fn promotion_applications(&self) -> &FxHashMap<usize, PromotionApplication<'a>> {
        &self.promotion_applications
    }

    /// Lookup the promotion application for a given basket item index.
    pub fn promotion_application_for_item(
        &self,
        item_idx: usize,
    ) -> Option<&PromotionApplication<'a>> {
        self.promotion_applications.get(&item_idx)
    }

    /// Currency used for all monetary values.
    pub fn currency(&self) -> &'static Currency {
        self.currency
    }

    /// Prints the receipt to the console.
    ///
    /// # Errors
    ///
    /// Returns an error if the receipt cannot be printed.
    pub fn write_to(
        &self,
        mut out: impl std::io::Write,
        basket: &Basket<'_>,
        product_meta: &SlotMap<ProductKey, Product<'_>>,
        promotion_meta: &SlotMap<PromotionKey, PromotionMeta>,
    ) -> Result<(), ReceiptError> {
        let mut table = Table::new();

        table
            .load_preset(comfy_table::presets::UTF8_HORIZONTAL_ONLY)
            .set_header(vec![
                Cell::new("").add_attribute(Attribute::Bold),
                Cell::new("Item").add_attribute(Attribute::Bold),
                Cell::new("Tags").add_attribute(Attribute::Bold),
                Cell::new("Base Price").add_attribute(Attribute::Bold),
                Cell::new("Discounted Price").add_attribute(Attribute::Bold),
                Cell::new("Savings").add_attribute(Attribute::Bold),
                Cell::new("Promotion").add_attribute(Attribute::Bold),
            ]);

        for (item_idx, item) in basket.iter().enumerate() {
            let product_name = &product_meta
                .get(item.product())
                .ok_or(ReceiptError::MissingProduct(item.product()))?
                .name;

            let product_tags = &product_meta
                .get(item.product())
                .ok_or(ReceiptError::MissingProduct(item.product()))?
                .tags
                .to_strs()
                .join("\n");

            let (base_price, final_price, savings, promo_name, bundle_id) =
                match self.promotion_applications.get(&item_idx) {
                    Some(app) => {
                        let promo_name = promotion_meta
                            .get(app.promotion_key)
                            .map_or("<unknown>", |meta| meta.name.as_str())
                            .to_string();

                        let savings_percent_points =
                            percent_points_from_fractional_percentage(app.savings_percent()?);

                        (
                            app.original_price,
                            app.final_price,
                            format!(
                                "-{} ({savings_percent_points}%)",
                                app.savings().map_err(ReceiptError::Money)?,
                            ),
                            promo_name,
                            format!("#{:<3}", app.bundle_id + 1),
                        )
                    }
                    None => (
                        *item.price(),
                        *item.price(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ),
                };

            let (final_price_display, savings_display) =
                if price_is_unchanged(base_price, final_price) {
                    (Cell::new(""), Cell::new(""))
                } else {
                    (
                        Cell::new(format!("{final_price}")).fg(Color::Green),
                        text_cell(&savings),
                    )
                };

            table.add_row(vec![
                Cell::new(format!("#{:<3}", item_idx + 1)),
                Cell::new(product_name.clone()),
                Cell::new(product_tags).fg(Color::DarkGrey),
                Cell::new(format!("{base_price}")),
                final_price_display,
                savings_display,
                text_cell(&format!("{bundle_id} {promo_name}")),
            ]);
        }

        writeln!(out, "\n{table}\n").map_err(|_err| ReceiptError::IO)?;
        writeln!(out, "Subtotal: {}", self.subtotal()).map_err(|_err| ReceiptError::IO)?;
        writeln!(out, "Total:    {}", self.total()).map_err(|_err| ReceiptError::IO)?;

        let savings_percent = self.savings_percent()?;
        let savings_percent_points = percent_points_from_fractional_percentage(savings_percent);

        writeln!(
            out,
            "Savings:  {} ({}%)",
            self.savings()?,
            format_args!("{savings_percent_points:.2}")
        )
        .map_err(|_err| ReceiptError::IO)?;

        Ok(())
    }
}

fn text_cell(s: &str) -> Cell {
    if s == "-" {
        Cell::new("")
    } else {
        Cell::new(s)
    }
}

/// Converts a fractional percentage to percent points for display.
fn percent_points_from_fractional_percentage(p: Percentage) -> Decimal {
    // `Percentage` is a fraction (e.g. 0.25), so multiply by 100 to print percent points.
    ((p * Decimal::ONE) * Decimal::from_i64(100).unwrap_or(Decimal::ZERO)).round_dp(2)
}

/// Returns true if the final price is the same as the base price.
fn price_is_unchanged<'a>(
    base_price: Money<'a, Currency>,
    final_price: Money<'a, Currency>,
) -> bool {
    final_price == base_price
}

#[cfg(test)]
mod tests {
    use num_traits::FromPrimitive;
    use rustc_hash::FxHashMap;
    use rusty_money::{
        Money,
        iso::{self, GBP, USD},
    };
    use slotmap::SlotMap;
    use smallvec::smallvec;
    use testresult::TestResult;

    use crate::{
        items::Item,
        products::{Product, ProductKey},
        promotions::{PromotionKey, PromotionMeta},
        tags::string::StringTagCollection,
    };

    use super::*;

    #[test]
    fn accessors_return_values_from_constructor() {
        let promotion_apps = FxHashMap::default();
        let receipt = Receipt::new(
            smallvec![0, 2],
            promotion_apps,
            Money::from_minor(300, GBP),
            Money::from_minor(250, GBP),
            GBP,
        );

        assert_eq!(receipt.subtotal(), Money::from_minor(300, GBP));
        assert_eq!(receipt.total(), Money::from_minor(250, GBP));
    }

    #[test]
    fn savings_is_subtotal_minus_total() -> TestResult {
        let promotion_apps = FxHashMap::default();
        let receipt = Receipt::new(
            smallvec![0, 1],
            promotion_apps,
            Money::from_minor(300, GBP),
            Money::from_minor(250, GBP),
            GBP,
        );

        assert_eq!(receipt.savings()?, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn savings_errors_on_currency_mismatch() {
        let promotion_apps = FxHashMap::default();
        let receipt = Receipt::new(
            smallvec![0],
            promotion_apps,
            Money::from_minor(300, GBP),
            Money::from_minor(250, iso::USD),
            GBP,
        );

        assert_eq!(
            receipt.savings(),
            Err(MoneyError::CurrencyMismatch {
                expected: GBP.iso_alpha_code,
                actual: USD.iso_alpha_code,
            })
        );
    }

    #[test]
    fn savings_percent_is_zero_when_subtotal_is_zero() -> TestResult {
        let receipt = Receipt::new(
            smallvec![],
            FxHashMap::default(),
            Money::from_minor(0, GBP),
            Money::from_minor(0, GBP),
            GBP,
        );

        assert_eq!(receipt.savings_percent()?, Percentage::from(0.0));
        Ok(())
    }

    #[test]
    fn savings_percent_is_correct_for_nonzero_subtotal() -> TestResult {
        let receipt = Receipt::new(
            smallvec![],
            FxHashMap::default(),
            Money::from_minor(400, GBP),
            Money::from_minor(300, GBP),
            GBP,
        );

        let percent_points = percent_points_from_fractional_percentage(receipt.savings_percent()?);

        assert_eq!(
            percent_points,
            Decimal::from_i64(25).expect("Failed to convert to Decimal")
        );

        Ok(())
    }

    #[test]
    fn discounted_price_dash_logic_matches_price_equality() {
        let base = Money::from_minor(100, GBP);
        assert!(price_is_unchanged(base, base));
        assert!(!price_is_unchanged(base, Money::from_minor(99, GBP)));
    }

    #[test]
    fn percent_points_converts_fractional_percentage_to_percent_points() {
        let points = percent_points_from_fractional_percentage(Percentage::from(0.25));

        assert_eq!(
            points,
            Decimal::from_i64(25).expect("Failed to convert to Decimal")
        );
    }

    #[test]
    fn from_solver_result_builds_receipt_with_correct_fields() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
        ];

        let basket = Basket::with_items(items, GBP)?;

        let promotion_apps = smallvec![
            PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 0,
                bundle_id: 0,
                original_price: Money::from_minor(100, GBP),
                final_price: Money::from_minor(75, GBP),
            },
            PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 2,
                bundle_id: 1,
                original_price: Money::from_minor(300, GBP),
                final_price: Money::from_minor(225, GBP),
            },
        ];

        let solver_result = SolverResult {
            affected_items: smallvec![0, 2],
            unaffected_items: smallvec![1],
            total: Money::from_minor(500, GBP), // 75 + 200 + 225
            promotion_applications: promotion_apps,
        };

        let receipt = Receipt::from_solver_result(&basket, solver_result)?;

        assert_eq!(receipt.subtotal(), Money::from_minor(600, GBP));
        assert_eq!(receipt.total(), Money::from_minor(500, GBP));
        assert_eq!(receipt.full_price_items(), &[1]);
        assert_eq!(receipt.promotion_applications().len(), 2);
        assert_eq!(receipt.currency(), GBP);

        Ok(())
    }

    #[test]
    fn from_solver_result_handles_no_promotions() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        let basket = Basket::with_items(items, GBP)?;

        let solver_result = SolverResult {
            affected_items: smallvec![],
            unaffected_items: smallvec![0, 1],
            total: Money::from_minor(300, GBP),
            promotion_applications: smallvec![],
        };

        let receipt = Receipt::from_solver_result(&basket, solver_result)?;

        assert_eq!(receipt.subtotal(), Money::from_minor(300, GBP));
        assert_eq!(receipt.total(), Money::from_minor(300, GBP));
        assert_eq!(receipt.full_price_items(), &[0, 1]);
        assert!(receipt.promotion_applications().is_empty());
        assert_eq!(receipt.savings()?, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn from_solver_result_verifies_promotion_application_details() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let basket = Basket::with_items(items, GBP)?;

        let promotion_apps = smallvec![PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 42,
            original_price: Money::from_minor(100, GBP),
            final_price: Money::from_minor(50, GBP),
        }];

        let solver_result = SolverResult {
            affected_items: smallvec![0],
            unaffected_items: smallvec![],
            total: Money::from_minor(50, GBP),
            promotion_applications: promotion_apps,
        };

        let receipt = Receipt::from_solver_result(&basket, solver_result)?;

        let app = receipt
            .promotion_application_for_item(0)
            .ok_or("Expected promotion application")?;

        assert_eq!(app.item_idx, 0);
        assert_eq!(app.bundle_id, 42);
        assert_eq!(app.original_price, Money::from_minor(100, GBP));
        assert_eq!(app.final_price, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn new_accessor_methods_return_expected_values() {
        let mut promotion_apps = FxHashMap::default();

        promotion_apps.insert(
            1,
            PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 1,
                bundle_id: 0,
                original_price: Money::from_minor(200, GBP),
                final_price: Money::from_minor(150, GBP),
            },
        );

        let receipt = Receipt::new(
            smallvec![0, 2],
            promotion_apps,
            Money::from_minor(600, GBP),
            Money::from_minor(550, GBP),
            GBP,
        );

        assert_eq!(receipt.full_price_items(), &[0, 2]);
        assert_eq!(receipt.promotion_applications().len(), 1);
        assert_eq!(receipt.currency(), GBP);
    }

    #[test]
    fn write_to_renders_promotion_and_full_price_items() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let mut promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let apple_price = Money::from_minor(100, GBP);
        let banana_price = Money::from_minor(200, GBP);

        let apple_key = product_meta.insert(Product {
            name: "Apple".to_string(),
            tags: StringTagCollection::from_strs(&["fruit"]),
            price: apple_price,
        });

        let banana_key = product_meta.insert(Product {
            name: "Banana".to_string(),
            tags: StringTagCollection::from_strs(&["fruit"]),
            price: banana_price,
        });

        let promo_key = promotion_meta.insert(PromotionMeta {
            name: "Fruit Sale".to_string(),
        });

        let items = [
            Item::new(apple_key, apple_price),
            Item::new(banana_key, banana_price),
        ];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();
        promotion_apps.insert(
            0,
            PromotionApplication {
                promotion_key: promo_key,
                item_idx: 0,
                bundle_id: 0,
                original_price: apple_price,
                final_price: Money::from_minor(80, GBP),
            },
        );

        let receipt = Receipt::new(
            smallvec![1],
            promotion_apps,
            Money::from_minor(300, GBP),
            Money::from_minor(280, GBP),
            GBP,
        );

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;
        assert!(output.contains("Apple"));
        assert!(output.contains("Banana"));
        assert!(output.contains("Fruit Sale"));
        assert!(output.contains("Subtotal:"));
        assert!(output.contains("Total:"));

        Ok(())
    }

    #[test]
    fn write_to_errors_on_missing_product() -> TestResult {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let basket = Basket::with_items(items, GBP)?;

        let receipt = Receipt::new(
            smallvec![0],
            FxHashMap::default(),
            Money::from_minor(100, GBP),
            Money::from_minor(100, GBP),
            GBP,
        );

        let product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let result = receipt.write_to(Vec::new(), &basket, &product_meta, &promotion_meta);
        assert!(matches!(result, Err(ReceiptError::MissingProduct(_))));

        Ok(())
    }

    #[test]
    fn write_to_renders_unknown_promotion_name_and_full_price_items() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();

        let drink_price = Money::from_minor(100, GBP);
        let snack_price = Money::from_minor(120, GBP);

        let drink_key = product_meta.insert(Product {
            name: "Drink".to_string(),
            tags: StringTagCollection::from_strs(&["fruit"]),
            price: drink_price,
        });

        let snack_key = product_meta.insert(Product {
            name: "Snack".to_string(),
            tags: StringTagCollection::from_strs(&["fruit"]),
            price: snack_price,
        });

        let items = [
            Item::new(drink_key, drink_price),
            Item::new(snack_key, snack_price),
        ];

        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();

        promotion_apps.insert(
            0,
            PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 0,
                bundle_id: 0,
                original_price: drink_price,
                final_price: drink_price,
            },
        );

        let receipt = Receipt::new(
            smallvec![1],
            promotion_apps,
            Money::from_minor(220, GBP),
            Money::from_minor(220, GBP),
            GBP,
        );

        let promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;
        assert!(output.contains("<unknown>"));
        assert!(output.contains("Savings:"));

        Ok(())
    }

    #[test]
    fn write_to_renders_discounted_price_and_savings_percent() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let mut promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let apple_price = Money::from_minor(100, GBP);
        let apple_key = product_meta.insert(Product {
            name: "Apple".to_string(),
            tags: StringTagCollection::from_strs(&["fruit"]),
            price: apple_price,
        });

        let promo_key = promotion_meta.insert(PromotionMeta {
            name: "Half Off".to_string(),
        });

        let items = [Item::new(apple_key, apple_price)];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();

        promotion_apps.insert(
            0,
            PromotionApplication {
                promotion_key: promo_key,
                item_idx: 0,
                bundle_id: 0,
                original_price: apple_price,
                final_price: Money::from_minor(50, GBP),
            },
        );

        let receipt = Receipt::new(
            smallvec![],
            promotion_apps,
            Money::from_minor(100, GBP),
            Money::from_minor(50, GBP),
            GBP,
        );

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;
        assert!(output.contains("Apple"));
        assert!(output.contains("Half Off"));
        assert!(output.contains("Savings:"));
        assert!(output.contains("(50.00%)"));

        Ok(())
    }

    #[test]
    fn text_cell_hides_dash() {
        let cell = super::text_cell("-");
        assert_eq!(cell.content(), "");

        let cell = super::text_cell("Value");
        assert_eq!(cell.content(), "Value");
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "duplicate promotion application")]
    fn from_solver_result_panics_on_duplicate_promotion_applications() {
        let items = [Item::new(
            ProductKey::default(),
            Money::from_minor(100, GBP),
        )];

        let basket = Basket::with_items(items, GBP).expect("basket should build");

        let app = PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 0,
            original_price: Money::from_minor(100, GBP),
            final_price: Money::from_minor(50, GBP),
        };

        let solver_result = SolverResult {
            affected_items: smallvec![0],
            unaffected_items: smallvec![],
            total: Money::from_minor(50, GBP),
            promotion_applications: smallvec![app.clone(), app],
        };

        let _ = Receipt::from_solver_result(&basket, solver_result).expect("receipt should build");
    }
}
