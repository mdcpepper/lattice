use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use leptos::prelude::*;
use rusty_money::iso::Currency;
use slotmap::SlotMap;

use lattice::{
    fixtures::products::{ProductsFixture, parse_price},
    products::{Product, ProductKey},
};

fn start_icon_confirmation(confirmed_icons: RwSignal<BTreeSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.insert(icon_key.to_string());
    });
}

fn clear_icon_confirmation(confirmed_icons: RwSignal<BTreeSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.remove(icon_key);
    });
}

/// UI model for a product row.
#[derive(Debug, Clone)]
pub struct ProductListItem {
    /// Stable fixture key.
    pub fixture_key: String,

    /// Display name.
    pub name: String,

    /// Display price.
    pub price: String,

    /// Shelf price in minor units.
    pub price_minor: i64,

    /// Currency code for this product.
    pub currency_code: &'static str,
}

/// Estimated marginal basket effect of adding a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductEstimate {
    /// Delta in basket total after adding one unit.
    pub marginal_minor: i64,

    /// Savings relative to shelf price (`shelf - marginal`).
    pub savings_minor: i64,
}

/// Loaded product fixture data needed by the app.
#[derive(Debug)]
pub struct LoadedProducts {
    /// Products shown in the UI.
    pub products: Vec<ProductListItem>,

    /// Catalog keyed by product key.
    pub product_meta_map: SlotMap<ProductKey, Product<'static>>,

    /// Fixture key -> product key lookup.
    pub product_key_by_fixture_key: HashMap<String, ProductKey>,

    /// Currency used by this fixture set.
    pub currency: &'static Currency,
}

/// Load products fixture content into UI and solver-ready structures.
///
/// # Errors
///
/// Returns an error when fixture parsing fails, prices are invalid, currencies
/// are inconsistent across products, or no products are present.
pub fn load_products(yaml: &str) -> Result<LoadedProducts, String> {
    let products_fixture: ProductsFixture = serde_norway::from_str(yaml)
        .map_err(|error| format!("Failed to parse products fixture: {error}"))?;

    let mut products: Vec<ProductListItem> = Vec::new();
    let mut product_meta_map: SlotMap<ProductKey, Product<'static>> = SlotMap::with_key();
    let mut product_key_by_fixture_key: HashMap<String, ProductKey> = HashMap::new();
    let mut currency: Option<&'static Currency> = None;

    for (fixture_key, product_fixture) in products_fixture.products {
        let (minor_units, parsed_currency) = parse_price(&product_fixture.price)
            .map_err(|error| format!("Invalid price for product '{fixture_key}': {error}"))?;

        if let Some(existing_currency) = currency
            && existing_currency != parsed_currency
        {
            return Err(format!(
                "Currency mismatch in products fixture: expected {}, found {}",
                existing_currency.iso_alpha_code, parsed_currency.iso_alpha_code
            ));
        }

        currency = Some(parsed_currency);

        let parsed_product: Product<'static> =
            Product::try_from(product_fixture).map_err(|error| error.to_string())?;

        let display_name = parsed_product.name.clone();
        let product_key = product_meta_map.insert(parsed_product);

        products.push(ProductListItem {
            fixture_key: fixture_key.clone(),
            name: display_name,
            price: format_price(minor_units, parsed_currency.iso_alpha_code),
            price_minor: minor_units,
            currency_code: parsed_currency.iso_alpha_code,
        });

        product_key_by_fixture_key.insert(fixture_key, product_key);
    }

    products.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(LoadedProducts {
        products,
        product_meta_map,
        product_key_by_fixture_key,
        currency: currency.ok_or_else(|| "No products found in fixture".to_string())?,
    })
}

/// Format a minor-unit amount into a currency string.
pub fn format_price(minor_units: i64, currency_code: &str) -> String {
    let abs_minor = minor_units.unsigned_abs();
    let major_units = abs_minor / 100;
    let fractional = abs_minor % 100;
    let sign = if minor_units < 0 { "-" } else { "" };
    let symbol = match currency_code {
        "GBP" => "£",
        "USD" => "$",
        "EUR" => "€",
        _ => "",
    };

    if symbol.is_empty() {
        format!("{sign}{major_units}.{fractional:02} {currency_code}")
    } else {
        format!("{sign}{symbol}{major_units}.{fractional:02}")
    }
}

#[component]
fn PriceSummary(impact_price: String, shelf_price: Option<String>) -> impl IntoView {
    view! {
        <div class="product-price-summary">
            {shelf_price.map_or_else(
                || ().into_any(),
                |value| {
                    view! {
                        <span class="product-shelf-price">
                            <span class="sr-only">"Was "</span>
                            <del>{value}</del>
                        </span>
                    }
                    .into_any()
                },
            )}
            <span class="product-impact-price">{impact_price}</span>
        </div>
    }
}

#[component]
fn ProductsHeading(show_spinner: RwSignal<bool>) -> impl IntoView {
    view! {
        <div class="panel-header">
            <h2 class="panel-title panel-title-offset">"Products"</h2>
            {move || {
                if show_spinner.get() {
                    view! {
                        <span class="panel-spinner" aria-live="polite">
                            <svg
                                xmlns="http://www.w3.org/2000/svg"
                                width="24"
                                height="24"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                class="lucide lucide-loader-circle-icon lucide-loader-circle"
                                aria-hidden="true"
                            >
                                <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
                            </svg>
                            <span class="sr-only">"Updating product prices"</span>
                        </span>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

#[component]
fn SavingsLine(text: Option<String>) -> impl IntoView {
    let (classes, value) = match text {
        Some(value) => ("product-savings-line", value),
        None => (
            "product-savings-line product-savings-line-hidden",
            String::new(),
        ),
    };

    view! {
        <p class=classes>{value}</p>
    }
}

#[component]
fn ProductRow(
    product: ProductListItem,
    estimate: Option<ProductEstimate>,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    let item_name = product.name.clone();
    let product_name = item_name.clone();
    let announce_name = item_name.clone();
    let price = product.price.clone();

    let impact_price = estimate.map_or_else(
        || price.clone(),
        |value| format_price(value.marginal_minor, product.currency_code),
    );

    let is_favorable_impact = estimate.is_some_and(|value| value.marginal_minor <= 0);

    let show_shelf_price =
        estimate.is_some_and(|value| value.marginal_minor != product.price_minor);
    let shelf_price = show_shelf_price.then_some(price.clone());

    let savings_text = estimate.and_then(|value| {
        (value.savings_minor > 0).then(|| {
            format!(
                "Save {}",
                format_price(value.savings_minor, product.currency_code)
            )
        })
    });

    let add_button_label = estimate.map_or_else(
        || format!("Add {item_name} ({price}) to basket"),
        |value| {
            format!(
                "Add {item_name}. Basket impact {}.",
                format_price(value.marginal_minor, product.currency_code)
            )
        },
    );

    let fixture_key = product.fixture_key.clone();
    let icon_key_for_class = product.fixture_key.clone();
    let icon_key_for_click = product.fixture_key.clone();
    let icon_key_for_animation_end = product.fixture_key.clone();

    let row_class = if is_favorable_impact {
        "product-row product-row-favorable"
    } else {
        "product-row"
    };

    view! {
        <li class=row_class>
            <div>
                <p class="product-name">{product_name}</p>
                <SavingsLine text=savings_text />
            </div>
            <div>
                <PriceSummary impact_price=impact_price shelf_price=shelf_price />
                <button
                    type="button"
                    aria-label=add_button_label
                    class=move || {
                        if add_icon_confirmations.with(|states| states.contains(&icon_key_for_class)) {
                            "icon-button icon-button-primary icon-button-product icon-button-confirmed"
                        } else {
                            "icon-button icon-button-primary icon-button-product"
                        }
                    }
                    on:click=move |_| {
                        start_icon_confirmation(add_icon_confirmations, &icon_key_for_click);
                        cart_items.update(|items| items.push(fixture_key.clone()));
                        action_message.set(Some(format!("Added {announce_name} to basket.")));
                    }
                >
                    <span class="icon-button-icon-stack" aria-hidden="true">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            class="icon-button-icon icon-button-icon-original lucide lucide-plus-icon lucide-plus"
                        >
                            <path d="M5 12h14"></path>
                            <path d="M12 5v14"></path>
                        </svg>
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            class="icon-button-icon icon-button-icon-check lucide lucide-check-icon lucide-check"
                            on:animationend=move |_| {
                                clear_icon_confirmation(add_icon_confirmations, &icon_key_for_animation_end);
                            }
                        >
                            <path d="M20 6 9 17l-5-5"></path>
                        </svg>
                    </span>
                </button>
            </div>
        </li>
    }
}

#[component]
fn ProductRows(
    products: Vec<ProductListItem>,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    estimates: RwSignal<BTreeMap<String, ProductEstimate>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    view! {
        {move || {
            let estimate_map = estimates.get();

            products
                .iter()
                .map(|product| {
                    view! {
                        <ProductRow
                            product=product.clone()
                            estimate=estimate_map.get(&product.fixture_key).copied()
                            cart_items=cart_items
                            action_message=action_message
                            add_icon_confirmations=add_icon_confirmations
                        />
                    }
                })
                .collect_view()
        }}
    }
}

/// Products panel component.
#[component]
pub fn ProductsPanel(
    /// Product rows rendered in the panel.
    products: Arc<Vec<ProductListItem>>,
    /// Shared cart fixture keys.
    cart_items: RwSignal<Vec<String>>,
    /// Ephemeral action message shown to the user.
    action_message: RwSignal<Option<String>>,
    /// Latest per-product basket-impact estimates.
    estimates: RwSignal<BTreeMap<String, ProductEstimate>>,
    /// Whether estimate recalculation is currently in progress.
    show_spinner: RwSignal<bool>,
) -> impl IntoView {
    let products = Arc::unwrap_or_clone(products);
    let add_icon_confirmations = RwSignal::new(BTreeSet::<String>::new());

    view! {
        <section class="products-panel">
            <ProductsHeading show_spinner=show_spinner />
            <ul class="products-list">
                <ProductRows
                    products=products
                    cart_items=cart_items
                    action_message=action_message
                    estimates=estimates
                    add_icon_confirmations=add_icon_confirmations
                />
            </ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::*;
    use testresult::TestResult;

    use super::*;

    // Test icon confirmation helper functions
    #[test]
    fn test_start_icon_confirmation_adds_key() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        start_icon_confirmation(confirmed_icons, "test-key");

        let result = confirmed_icons.get_untracked();

        assert!(result.contains("test-key"));
    }

    #[test]
    fn test_start_icon_confirmation_multiple_keys() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        start_icon_confirmation(confirmed_icons, "key1");
        start_icon_confirmation(confirmed_icons, "key2");

        let result = confirmed_icons.get_untracked();

        assert!(result.contains("key1"));
        assert!(result.contains("key2"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_clear_icon_confirmation_removes_key() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        start_icon_confirmation(confirmed_icons, "test-key");

        clear_icon_confirmation(confirmed_icons, "test-key");

        let result = confirmed_icons.get_untracked();

        assert!(!result.contains("test-key"));
    }

    #[test]
    fn test_clear_icon_confirmation_nonexistent_key() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        clear_icon_confirmation(confirmed_icons, "nonexistent");

        let result = confirmed_icons.get_untracked();

        assert!(result.is_empty());
    }

    // Test format_price function
    #[test]
    fn test_format_price_gbp_positive() {
        let result = format_price(1250, "GBP");

        assert_eq!(result, "£12.50");
    }

    #[test]
    fn test_format_price_usd_positive() {
        let result = format_price(999, "USD");

        assert_eq!(result, "$9.99");
    }

    #[test]
    fn test_format_price_eur_positive() {
        let result = format_price(5000, "EUR");

        assert_eq!(result, "€50.00");
    }

    #[test]
    fn test_format_price_zero() {
        let result = format_price(0, "GBP");

        assert_eq!(result, "£0.00");
    }

    #[test]
    fn test_format_price_negative_gbp() {
        let result = format_price(-1250, "GBP");

        assert_eq!(result, "-£12.50");
    }

    #[test]
    fn test_format_price_negative_usd() {
        let result = format_price(-999, "USD");

        assert_eq!(result, "-$9.99");
    }

    #[test]
    fn test_format_price_negative_eur() {
        let result = format_price(-5000, "EUR");

        assert_eq!(result, "-€50.00");
    }

    #[test]
    fn test_format_price_single_digit_cents() {
        let result = format_price(105, "GBP");

        assert_eq!(result, "£1.05");
    }

    #[test]
    fn test_format_price_zero_cents() {
        let result = format_price(1000, "USD");

        assert_eq!(result, "$10.00");
    }

    #[test]
    fn test_format_price_large_amount() {
        let result = format_price(123_456, "GBP");

        assert_eq!(result, "£1234.56");
    }

    #[test]
    fn test_format_price_unknown_currency() {
        let result = format_price(1250, "JPY");

        assert_eq!(result, "12.50 JPY");
    }

    #[test]
    fn test_format_price_unknown_currency_zero() {
        let result = format_price(0, "CHF");

        assert_eq!(result, "0.00 CHF");
    }

    #[test]
    fn test_format_price_unknown_currency_negative() {
        let result = format_price(-1250, "AUD");

        assert_eq!(result, "-12.50 AUD");
    }

    #[test]
    fn test_format_price_one_cent() {
        let result = format_price(1, "GBP");

        assert_eq!(result, "£0.01");
    }

    #[test]
    fn test_format_price_ninety_nine_cents() {
        let result = format_price(99, "USD");

        assert_eq!(result, "$0.99");
    }

    #[test]
    fn test_format_price_exactly_one_dollar() {
        let result = format_price(100, "USD");

        assert_eq!(result, "$1.00");
    }

    #[test]
    fn test_format_price_very_large_amount() {
        let result = format_price(999_999_999, "EUR");

        assert_eq!(result, "€9999999.99");
    }

    // Test load_products function with valid YAML
    #[test]
    fn test_load_products_empty_yaml() {
        let yaml = r"
products: {}
";

        let result = load_products(yaml);

        assert!(result.is_err());
        assert!(result.is_err_and(|error| error.contains("No products found in fixture")));
    }

    #[test]
    fn test_load_products_invalid_yaml() {
        let yaml = "invalid: yaml: structure: [[[";

        let result = load_products(yaml);

        assert!(result.is_err());
    }

    #[test]
    fn test_load_products_single_product() -> TestResult {
        let yaml = r#"
products:
  product1:
    name: "Test Product"
    price: "10.00 GBP"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.products.len(), 1);
        assert_eq!(loaded.products[0].name, "Test Product");
        assert_eq!(loaded.products[0].price_minor, 1000);

        Ok(())
    }

    #[test]
    fn test_load_products_multiple_products_sorted() -> TestResult {
        let yaml = r#"
products:
  product1:
    name: "Zebra"
    price: "10.00 GBP"
    tags: []
  product2:
    name: "Apple"
    price: "5.00 GBP"
    tags: []
  product3:
    name: "Mango"
    price: "7.50 GBP"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.products.len(), 3);

        // Products should be sorted alphabetically by name
        assert_eq!(loaded.products[0].name, "Apple");
        assert_eq!(loaded.products[1].name, "Mango");
        assert_eq!(loaded.products[2].name, "Zebra");

        Ok(())
    }

    #[test]
    fn test_load_products_currency_mismatch() {
        let yaml = r#"
products:
  product1:
    name: "Product 1"
    price: "10.00 GBP"
    tags: []
  product2:
    name: "Product 2"
    price: "5.00 USD"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_err());
        assert!(result.is_err_and(|error| error.contains("Currency mismatch")));
    }

    #[test]
    fn test_load_products_invalid_price() {
        let yaml = r#"
products:
  product1:
    name: "Test Product"
    price: "invalid"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_err());
        assert!(result.is_err_and(|error| error.contains("Invalid price")));
    }

    #[test]
    fn test_load_products_with_tags() -> TestResult {
        let yaml = r#"
products:
  product1:
    name: "Test Product"
    price: "10.00 GBP"
    tags: ["tag1", "tag2"]
"#;

        let result = load_products(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.products.len(), 1);

        Ok(())
    }

    #[test]
    fn test_load_products_fixture_key_mapping() -> TestResult {
        let yaml = r#"
products:
  my-product-key:
    name: "Test Product"
    price: "10.00 GBP"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert!(
            loaded
                .product_key_by_fixture_key
                .contains_key("my-product-key")
        );
        assert_eq!(loaded.products[0].fixture_key, "my-product-key");

        Ok(())
    }

    #[test]
    fn test_load_products_currency_extraction() -> TestResult {
        let yaml = r#"
products:
  product1:
    name: "Test Product"
    price: "15.50 EUR"
    tags: []
"#;

        let result = load_products(yaml);

        assert!(result.is_ok());

        let loaded = result?;

        assert_eq!(loaded.currency.iso_alpha_code, "EUR");

        Ok(())
    }
}
