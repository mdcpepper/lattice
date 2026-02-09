//! Receipt

use std::{fmt::Write, io};

use decimal_percentage::Percentage;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use rustc_hash::FxHashMap;
use rusty_money::{Money, MoneyError, iso::Currency};
use slotmap::SlotMap;
use smallvec::{SmallVec, smallvec};
use tabled::{
    builder::Builder,
    grid::config::HorizontalLine,
    settings::{
        Alignment, Color, Style, Theme,
        object::{Columns, Rows},
    },
};
use thiserror::Error;

use crate::{
    basket::Basket,
    graph::result::LayeredSolverResult,
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
    /// Each item may have multiple layered applications (one per promotion layer that
    /// touched it). For flat solver results, this will contain a single-element `SmallVec`.
    promotion_applications: FxHashMap<usize, SmallVec<[PromotionApplication<'a>; 3]>>,

    /// Total cost before any promotion applications
    subtotal: Money<'a, Currency>,

    /// Total amount paid for all items after any promotion applications
    total: Money<'a, Currency>,

    /// Currency used for all monetary values
    currency: &'static Currency,
}

impl<'a> Receipt<'a> {
    /// Create a new receipt with the given details.
    #[must_use]
    pub fn new(
        full_price_items: SmallVec<[usize; 10]>,
        promotion_applications: FxHashMap<usize, SmallVec<[PromotionApplication<'a>; 3]>>,
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
    #[must_use]
    pub fn subtotal(&self) -> Money<'a, Currency> {
        self.subtotal
    }

    /// Total amount paid for all items
    #[must_use]
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

            promotion_applications.insert(app.item_idx, smallvec![app]);
        }

        Ok(Receipt {
            full_price_items: result.unaffected_items,
            promotion_applications,
            subtotal: basket.subtotal()?,
            total: result.total,
            currency: basket.currency(),
        })
    }

    /// Build a receipt from a basket and layered solver result.
    ///
    /// # Errors
    ///
    /// Returns a [`ReceiptError`] if the basket subtotal cannot be calculated.
    pub fn from_layered_result(
        basket: &Basket<'_>,
        result: LayeredSolverResult<'a>,
    ) -> Result<Self, ReceiptError> {
        let subtotal_minor = basket.subtotal()?.to_minor_units();
        let currency = basket.currency();

        Ok(Receipt {
            full_price_items: result.full_price_items,
            promotion_applications: result.item_applications,
            subtotal: Money::from_minor(subtotal_minor, currency),
            total: result.total,
            currency,
        })
    }

    /// Indexes of items purchased at full price (not in any promotion).
    #[must_use]
    pub fn full_price_items(&self) -> &[usize] {
        &self.full_price_items
    }

    /// Promotion application details keyed by basket item index.
    #[must_use]
    pub fn promotion_applications(
        &self,
    ) -> &FxHashMap<usize, SmallVec<[PromotionApplication<'a>; 3]>> {
        &self.promotion_applications
    }

    /// Lookup the promotion applications for a given basket item index.
    ///
    /// Returns a slice of applications (one per promotion layer that touched this item).
    pub fn promotion_application_for_item(
        &self,
        item_idx: usize,
    ) -> Option<&[PromotionApplication<'a>]> {
        self.promotion_applications
            .get(&item_idx)
            .map(SmallVec::as_slice)
    }

    /// Currency used for all monetary values.
    #[must_use]
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
        mut out: impl io::Write,
        basket: &Basket<'_>,
        product_meta: &SlotMap<ProductKey, Product<'_>>,
        promotion_meta: &SlotMap<PromotionKey, PromotionMeta>,
    ) -> Result<(), ReceiptError> {
        let mut builder = Builder::default();

        push_receipt_header(&mut builder);

        let mut item_boundary_rows: SmallVec<[usize; 16]> = smallvec![];
        let mut color_ops: SmallVec<[(usize, usize, Color); 32]> = smallvec![];

        append_item_rows(
            self,
            basket,
            product_meta,
            promotion_meta,
            &mut builder,
            &mut item_boundary_rows,
            &mut color_ops,
        )?;

        write_receipt_table(&mut out, builder, &item_boundary_rows, color_ops)?;

        write_receipt_summary(&mut out, self)?;

        Ok(())
    }
}

fn push_receipt_header(builder: &mut Builder) {
    builder.push_record([
        "",
        "Item",
        "Tags",
        "Base Price",
        "Discounted Price",
        "Savings",
        "Promotion",
    ]);
}

fn append_item_rows(
    receipt: &Receipt<'_>,
    basket: &Basket<'_>,
    product_meta: &SlotMap<ProductKey, Product<'_>>,
    promotion_meta: &SlotMap<PromotionKey, PromotionMeta>,
    builder: &mut Builder,
    item_boundary_rows: &mut SmallVec<[usize; 16]>,
    color_ops: &mut SmallVec<[(usize, usize, Color); 32]>,
) -> Result<(), ReceiptError> {
    let mut row_writer = RowWriter::new(builder, color_ops, promotion_meta);

    for (item_idx, item) in basket.iter().enumerate() {
        let (product_name, product_tags) = product_display(item.product(), product_meta)?;

        item_boundary_rows.push(row_writer.current_row);

        match receipt.promotion_applications.get(&item_idx) {
            Some(apps) if apps.len() == 1 => row_writer.append_single_application_row(
                item_idx,
                &product_name,
                &product_tags,
                apps,
            )?,
            Some(apps) if apps.len() > 1 => row_writer.append_multi_layer_rows(
                item_idx,
                &product_name,
                &product_tags,
                item.price(),
                apps,
            )?,
            _ => row_writer.append_full_price_row(
                item_idx,
                &product_name,
                &product_tags,
                item.price(),
            ),
        }
    }

    Ok(())
}

fn product_display(
    product_key: ProductKey,
    product_meta: &SlotMap<ProductKey, Product<'_>>,
) -> Result<(String, String), ReceiptError> {
    let product = product_meta
        .get(product_key)
        .ok_or(ReceiptError::MissingProduct(product_key))?;

    Ok((product.name.clone(), product.tags.to_strs().join("\n")))
}

struct RowWriter<'a> {
    builder: &'a mut Builder,
    color_ops: &'a mut SmallVec<[(usize, usize, Color); 32]>,
    current_row: usize,
    promotion_meta: &'a SlotMap<PromotionKey, PromotionMeta>,
}

impl RowWriter<'_> {
    fn new<'a>(
        builder: &'a mut Builder,
        color_ops: &'a mut SmallVec<[(usize, usize, Color); 32]>,
        promotion_meta: &'a SlotMap<PromotionKey, PromotionMeta>,
    ) -> RowWriter<'a> {
        RowWriter {
            builder,
            color_ops,
            current_row: 1, // header is row 0
            promotion_meta,
        }
    }

    fn append_single_application_row(
        &mut self,
        item_idx: usize,
        product_name: &str,
        product_tags: &str,
        apps: &[PromotionApplication<'_>],
    ) -> Result<(), ReceiptError> {
        let Some(app) = apps.first() else {
            return Ok(());
        };

        let cells = promotion_cells(app, self.promotion_meta, true)?;

        self.builder.push_record([
            format!("#{:<3}", item_idx + 1),
            product_name.to_string(),
            product_tags.to_string(),
            cells.base_price,
            cells.final_price.clone(),
            cells.savings,
            cells.promotion,
        ]);

        self.color_ops
            .push((self.current_row, 2, color_dark_grey()));

        self.color_ops
            .push((self.current_row, 3, color_dark_grey()));

        if !cells.final_price.is_empty() {
            self.color_ops
                .push((self.current_row, 4, cells.price_color));
        }

        self.current_row += 1;

        Ok(())
    }

    fn append_multi_layer_rows(
        &mut self,
        item_idx: usize,
        product_name: &str,
        product_tags: &str,
        item_price: &Money<'_, Currency>,
        apps: &[PromotionApplication<'_>],
    ) -> Result<(), ReceiptError> {
        self.builder.push_record([
            format!("#{:<3}", item_idx + 1),
            product_name.to_string(),
            product_tags.to_string(),
            format!("{item_price}"),
            String::new(),
            String::new(),
            String::new(),
        ]);

        self.color_ops
            .push((self.current_row, 2, color_dark_grey()));

        self.color_ops
            .push((self.current_row, 3, color_dark_grey()));

        self.current_row += 1;

        let last_idx = apps.len().saturating_sub(1);

        for (idx, app) in apps.iter().enumerate() {
            let cells = promotion_cells(app, self.promotion_meta, idx == last_idx)?;

            self.builder.push_record([
                String::new(),
                String::new(),
                String::new(),
                cells.base_price,
                cells.final_price.clone(),
                cells.savings,
                cells.promotion,
            ]);

            self.color_ops
                .push((self.current_row, 3, color_dark_grey()));

            if !cells.final_price.is_empty() {
                self.color_ops
                    .push((self.current_row, 4, cells.price_color));
            }

            self.current_row += 1;
        }

        Ok(())
    }

    fn append_full_price_row(
        &mut self,
        item_idx: usize,
        product_name: &str,
        product_tags: &str,
        item_price: &Money<'_, Currency>,
    ) {
        self.builder.push_record([
            format!("#{:<3}", item_idx + 1),
            product_name.to_string(),
            product_tags.to_string(),
            format!("{item_price}"),
            String::new(),
            String::new(),
            String::new(),
        ]);

        self.color_ops
            .push((self.current_row, 2, color_dark_grey()));

        self.current_row += 1;
    }
}

fn write_receipt_table(
    out: &mut impl io::Write,
    builder: Builder,
    item_boundary_rows: &[usize],
    color_ops: SmallVec<[(usize, usize, Color); 32]>,
) -> Result<(), ReceiptError> {
    let mut table = builder.build();
    let mut theme = Theme::from(Style::modern_rounded());
    let separator = HorizontalLine::new(Some('─'), Some('┼'), Some('├'), Some('┤'));

    theme.remove_horizontal_lines();
    theme.insert_horizontal_line(1, separator);

    for &row in item_boundary_rows {
        if row > 1 {
            theme.insert_horizontal_line(row, separator);
        }
    }

    table.with(theme);
    table.modify(Rows::first(), Color::BOLD);
    table.modify(Columns::new(3..6), Alignment::right());

    for (row, col, color) in color_ops {
        table.modify((row, col), color);
    }

    let table_str = colorize_borders(&table.to_string());

    writeln!(out, "\n{table_str}").map_err(|_err| ReceiptError::IO)
}

fn write_receipt_summary(
    out: &mut impl io::Write,
    receipt: &Receipt<'_>,
) -> Result<(), ReceiptError> {
    let savings = receipt.savings()?;
    let savings_percent = receipt.savings_percent()?;
    let savings_percent_points = percent_points_from_fractional_percentage(savings_percent);

    let subtotal_label = " Subtotal:";
    let total_label = " \x1b[1mTotal:\x1b[0m";
    let savings_label = " Savings:";

    let subtotal_val = format!("{}  ", receipt.subtotal());
    let total_val = format!("{}  ", receipt.total());
    let savings_val = format!("({savings_percent_points:.2}%) {savings}  ");

    let label_width = visible_width(subtotal_label)
        .max(visible_width(total_label))
        .max(visible_width(savings_label));

    let value_width = subtotal_val
        .len()
        .max(total_val.len())
        .max(savings_val.len());

    write_summary_line(out, subtotal_label, &subtotal_val, label_width, value_width)?;

    write_summary_line(
        out,
        total_label,
        &format!("\x1b[1m{total_val}\x1b[0m"),
        label_width,
        value_width,
    )?;

    write_summary_line(out, savings_label, &savings_val, label_width, value_width)?;

    writeln!(out).map_err(|_err| ReceiptError::IO)
}

/// Cell contents for a single promotion application row.
struct PromotionCells {
    base_price: String,
    final_price: String,
    savings: String,
    promotion: String,
    price_color: Color,
}

/// Build the cell contents for one promotion application row.
///
/// When `is_final` is true the discounted price gets bright green.
/// Otherwise it gets dark green (an intermediate price feeding into the next layer).
fn promotion_cells(
    app: &PromotionApplication<'_>,
    promotion_meta: &SlotMap<PromotionKey, PromotionMeta>,
    is_final: bool,
) -> Result<PromotionCells, ReceiptError> {
    let promo_name = promotion_meta
        .get(app.promotion_key)
        .map_or("<unknown>", |meta| meta.name.as_str())
        .to_string();

    let savings_percent_points = percent_points_from_fractional_percentage(app.savings_percent()?);

    let savings_str = format!(
        "({savings_percent_points}%) -{}",
        app.savings().map_err(ReceiptError::Money)?,
    );

    let bundle_id = format!("#{:<3}", app.bundle_id + 1);

    let price_color = if is_final {
        Color::FG_GREEN
    } else {
        color_dark_green()
    };

    let (final_price_display, savings_display) =
        if price_is_unchanged(app.original_price, app.final_price) {
            (String::new(), String::new())
        } else {
            (format!("{}", app.final_price), savings_str)
        };

    Ok(PromotionCells {
        base_price: format!("{}", app.original_price),
        final_price: final_price_display,
        savings: savings_display,
        promotion: format!("{bundle_id} {promo_name}"),
        price_color,
    })
}

/// Converts a fractional percentage to percent points for display.
fn percent_points_from_fractional_percentage(percentage: Percentage) -> Decimal {
    // `Percentage` is a fraction (e.g. 0.25), so multiply by 100 to print percent points.
    ((percentage * Decimal::ONE) * Decimal::from_i64(100).unwrap_or(Decimal::ZERO)).round_dp(2)
}

/// Returns true if the final price is the same as the base price.
fn price_is_unchanged<'a>(
    base_price: Money<'a, Currency>,
    final_price: Money<'a, Currency>,
) -> bool {
    final_price == base_price
}

/// Wraps runs of UTF-8 box-drawing characters in ANSI dark-grey escape codes.
///
/// Box-drawing characters occupy the Unicode range U+2500..U+257F. This function
/// scans each character, grouping consecutive border characters and emitting a
/// single grey escape sequence around each run, leaving cell content untouched.
fn colorize_borders(table: &str) -> String {
    let mut out = String::with_capacity(table.len() + 256);
    let mut in_run = false;

    for ch in table.chars() {
        let box_char = ('\u{2500}'..='\u{257F}').contains(&ch);

        if box_char && !in_run {
            _ = out.write_str("\x1b[90m");
            in_run = true;
        } else if !box_char && in_run {
            _ = out.write_str("\x1b[0m");
            in_run = false;
        }

        out.push(ch);
    }

    if in_run {
        _ = out.write_str("\x1b[0m");
    }

    out
}

/// Returns the visible (non-ANSI) width of a string.
fn visible_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut in_escape = false;

    for ch in s.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            width += 1;
        }
    }

    width
}

/// Writes a summary line with a right-aligned label and a fixed-width value column.
fn write_summary_line(
    out: &mut impl io::Write,
    label: &str,
    value: &str,
    label_col_width: usize,
    value_col_width: usize,
) -> Result<(), ReceiptError> {
    let label_vis = visible_width(label);
    let value_vis = visible_width(value);

    // 2 chars of spacing between label and value column.
    let label_pad = label_col_width.saturating_sub(label_vis);
    let value_pad = value_col_width.saturating_sub(value_vis);

    writeln!(
        out,
        "{:>label_pad$}{label}  {value_pad}{value}",
        "",
        value_pad = " ".repeat(value_pad)
    )
    .map_err(|_err| ReceiptError::IO)
}

/// ANSI dark grey foreground.
fn color_dark_grey() -> Color {
    Color::new("\x1b[90m", "\x1b[0m")
}

/// ANSI dark green (intermediate layer price).
fn color_dark_green() -> Color {
    Color::new("\x1b[32m", "\x1b[0m")
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

        let apps = receipt
            .promotion_application_for_item(0)
            .ok_or("Expected promotion application")?;

        let app = apps.first().ok_or("Expected at least one application")?;

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
            smallvec![PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 1,
                bundle_id: 0,
                original_price: Money::from_minor(200, GBP),
                final_price: Money::from_minor(150, GBP),
            }],
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
            ..Default::default()
        });

        let items = [
            Item::new(apple_key, apple_price),
            Item::new(banana_key, banana_price),
        ];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();
        promotion_apps.insert(
            0,
            smallvec![PromotionApplication {
                promotion_key: promo_key,
                item_idx: 0,
                bundle_id: 0,
                original_price: apple_price,
                final_price: Money::from_minor(80, GBP),
            }],
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
            smallvec![PromotionApplication {
                promotion_key: PromotionKey::default(),
                item_idx: 0,
                bundle_id: 0,
                original_price: drink_price,
                final_price: drink_price,
            }],
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
            ..Default::default()
        });

        let items = [Item::new(apple_key, apple_price)];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();

        promotion_apps.insert(
            0,
            smallvec![PromotionApplication {
                promotion_key: promo_key,
                item_idx: 0,
                bundle_id: 0,
                original_price: apple_price,
                final_price: Money::from_minor(50, GBP),
            }],
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

    #[test]
    fn write_to_renders_item_savings_with_bundle_id() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let mut promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let wrap_price = Money::from_minor(400, GBP);
        let drink_price = Money::from_minor(150, GBP);

        let wrap_key = product_meta.insert(Product {
            name: "Chicken Wrap".to_string(),
            tags: StringTagCollection::from_strs(&["main", "hot"]),
            price: wrap_price,
        });

        let drink_key = product_meta.insert(Product {
            name: "Water".to_string(),
            tags: StringTagCollection::from_strs(&["drink", "cold"]),
            price: drink_price,
        });

        let promo_key = promotion_meta.insert(PromotionMeta {
            name: "Meal Deal".to_string(),
            ..Default::default()
        });

        let items = [
            Item::with_tags(
                wrap_key,
                wrap_price,
                StringTagCollection::from_strs(&["main", "hot"]),
            ),
            Item::with_tags(
                drink_key,
                drink_price,
                StringTagCollection::from_strs(&["drink", "cold"]),
            ),
        ];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();

        promotion_apps.insert(
            0,
            smallvec![PromotionApplication {
                promotion_key: promo_key,
                item_idx: 0,
                bundle_id: 5,
                original_price: wrap_price,
                final_price: Money::from_minor(300, GBP),
            }],
        );

        promotion_apps.insert(
            1,
            smallvec![PromotionApplication {
                promotion_key: promo_key,
                item_idx: 1,
                bundle_id: 5,
                original_price: drink_price,
                final_price: Money::from_minor(100, GBP),
            }],
        );

        let receipt = Receipt::new(
            smallvec![],
            promotion_apps,
            Money::from_minor(550, GBP),
            Money::from_minor(400, GBP),
            GBP,
        );

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;
        assert!(output.contains("Chicken Wrap"));
        assert!(output.contains("Water"));
        assert!(output.contains("Meal Deal"));
        assert!(output.contains("#6")); // bundle_id 5 displayed as 6 (5+1)
        assert!(output.contains("Subtotal:"));
        assert!(output.contains("Total:"));
        assert!(output.contains("Savings:"));
        // Tags should appear
        assert!(output.contains("main"));
        assert!(output.contains("drink"));

        Ok(())
    }

    #[test]
    fn write_to_with_zero_savings_percentage() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let item_price = Money::from_minor(100, GBP);
        let item_key = product_meta.insert(Product {
            name: "Item".to_string(),
            tags: StringTagCollection::from_strs(&["test"]),
            price: item_price,
        });

        let items = [Item::new(item_key, item_price)];
        let basket = Basket::with_items(items, GBP)?;

        let receipt = Receipt::new(
            smallvec![0],
            FxHashMap::default(),
            Money::from_minor(100, GBP),
            Money::from_minor(100, GBP),
            GBP,
        );

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;
        assert!(output.contains("Savings:"));
        assert!(output.contains("(0.00%"));

        Ok(())
    }

    #[test]
    fn from_layered_result_builds_receipt() -> TestResult {
        use crate::graph::result::LayeredSolverResult;

        let items = [
            Item::new(ProductKey::default(), Money::from_minor(400, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        let basket = Basket::with_items(items, GBP)?;

        let mut item_applications = FxHashMap::default();
        item_applications.insert(
            0,
            smallvec![
                PromotionApplication {
                    promotion_key: PromotionKey::default(),
                    item_idx: 0,
                    bundle_id: 0,
                    original_price: Money::from_minor(400, GBP),
                    final_price: Money::from_minor(300, GBP),
                },
                PromotionApplication {
                    promotion_key: PromotionKey::default(),
                    item_idx: 0,
                    bundle_id: 1,
                    original_price: Money::from_minor(300, GBP),
                    final_price: Money::from_minor(270, GBP),
                },
            ],
        );

        let layered_result = LayeredSolverResult {
            total: Money::from_minor(470, GBP),
            item_applications,
            full_price_items: smallvec![1],
        };

        let receipt = Receipt::from_layered_result(&basket, layered_result)?;

        assert_eq!(receipt.subtotal(), Money::from_minor(600, GBP));
        assert_eq!(receipt.total(), Money::from_minor(470, GBP));
        assert_eq!(receipt.full_price_items(), &[1]);
        assert_eq!(receipt.promotion_applications().len(), 1);

        let apps = receipt
            .promotion_application_for_item(0)
            .ok_or("Expected applications for item 0")?;
        assert_eq!(apps.len(), 2);

        Ok(())
    }

    #[test]
    fn write_to_renders_multi_layer_detail_rows() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();
        let mut promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let wrap_price = Money::from_minor(400, GBP);

        let wrap_key = product_meta.insert(Product {
            name: "Chicken Wrap".to_string(),
            tags: StringTagCollection::from_strs(&["main", "hot"]),
            price: wrap_price,
        });

        let food_sale_key = promotion_meta.insert(PromotionMeta {
            name: "Food Sale".to_string(),
            ..Default::default()
        });

        let loyalty_key = promotion_meta.insert(PromotionMeta {
            name: "Loyalty".to_string(),
            ..Default::default()
        });

        let items = [Item::with_tags(
            wrap_key,
            wrap_price,
            StringTagCollection::from_strs(&["main", "hot"]),
        )];
        let basket = Basket::with_items(items, GBP)?;

        let mut promotion_apps = FxHashMap::default();
        promotion_apps.insert(
            0,
            smallvec![
                PromotionApplication {
                    promotion_key: food_sale_key,
                    item_idx: 0,
                    bundle_id: 0,
                    original_price: Money::from_minor(400, GBP),
                    final_price: Money::from_minor(300, GBP),
                },
                PromotionApplication {
                    promotion_key: loyalty_key,
                    item_idx: 0,
                    bundle_id: 2,
                    original_price: Money::from_minor(300, GBP),
                    final_price: Money::from_minor(270, GBP),
                },
            ],
        );

        let receipt = Receipt::new(
            smallvec![],
            promotion_apps,
            Money::from_minor(400, GBP),
            Money::from_minor(270, GBP),
            GBP,
        );

        let mut out = Vec::new();
        receipt.write_to(&mut out, &basket, &product_meta, &promotion_meta)?;

        let output = String::from_utf8(out)?;

        // Item name appears in header row
        assert!(output.contains("Chicken Wrap"));
        // Both promotion names appear in detail rows
        assert!(output.contains("Food Sale"));
        assert!(output.contains("Loyalty"));
        // Intermediate prices are visible
        assert!(output.contains("£3.00"));
        assert!(output.contains("£2.70"));

        Ok(())
    }

    #[test]
    fn promotion_application_for_item_returns_slice() -> TestResult {
        let mut promotion_apps = FxHashMap::default();
        promotion_apps.insert(
            0,
            smallvec![
                PromotionApplication {
                    promotion_key: PromotionKey::default(),
                    item_idx: 0,
                    bundle_id: 0,
                    original_price: Money::from_minor(100, GBP),
                    final_price: Money::from_minor(80, GBP),
                },
                PromotionApplication {
                    promotion_key: PromotionKey::default(),
                    item_idx: 0,
                    bundle_id: 1,
                    original_price: Money::from_minor(80, GBP),
                    final_price: Money::from_minor(72, GBP),
                },
            ],
        );

        let receipt = Receipt::new(
            smallvec![],
            promotion_apps,
            Money::from_minor(100, GBP),
            Money::from_minor(72, GBP),
            GBP,
        );

        // Multi-layer item returns slice of length 2
        let apps = receipt
            .promotion_application_for_item(0)
            .ok_or("Expected applications")?;
        assert_eq!(apps.len(), 2);

        // Missing item returns None
        assert!(receipt.promotion_application_for_item(99).is_none());

        Ok(())
    }

    #[test]
    fn write_to_errors_when_product_metadata_missing() -> TestResult {
        let mut product_meta = SlotMap::<ProductKey, Product<'_>>::with_key();

        let promotion_meta = SlotMap::<PromotionKey, PromotionMeta>::with_key();

        let product_key = product_meta.insert(Product {
            name: "Snack".to_string(),
            tags: StringTagCollection::from_strs(&["snack"]),
            price: Money::from_minor(100, GBP),
        });

        let items = [Item::new(product_key, Money::from_minor(100, GBP))];

        let basket = Basket::with_items(items, GBP)?;

        let receipt = Receipt::new(
            smallvec![0],
            FxHashMap::default(),
            Money::from_minor(100, GBP),
            Money::from_minor(100, GBP),
            GBP,
        );

        let mut out = Vec::new();

        product_meta.remove(product_key);

        let err = receipt
            .write_to(&mut out, &basket, &product_meta, &promotion_meta)
            .expect_err("expected missing product error");

        assert!(matches!(err, ReceiptError::MissingProduct(_)));

        Ok(())
    }
}
