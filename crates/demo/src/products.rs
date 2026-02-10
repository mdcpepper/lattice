use std::{collections::HashMap, sync::Arc};

use leptos::prelude::*;
use rusty_money::iso::Currency;
use slotmap::SlotMap;

use lattice::{
    fixtures::products::{ProductsFixture, parse_price},
    products::{Product, ProductKey},
};

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

/// Products panel component.
#[component]
pub fn ProductsPanel(
    products: Arc<Vec<ProductListItem>>,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    estimates: RwSignal<HashMap<String, ProductEstimate>>,
    show_spinner: RwSignal<bool>,
) -> impl IntoView {
    let products = Arc::unwrap_or_clone(products);

    view! {
        <section class="products-panel">
            <ProductsHeading show_spinner=show_spinner />
            <ul class="products-list">
                {move || {
                    let estimate_map = estimates.get();

                    products
                    .iter()
                    .map(|product| {
                        let item_name = product.name.clone();
                        let product_name = item_name.clone();
                        let announce_name = item_name.clone();
                        let price = product.price.clone();
                        let estimate = estimate_map.get(&product.fixture_key).copied();

                        let impact_price = estimate.map_or_else(
                            || price.clone(),
                            |value| format_price(value.marginal_minor, product.currency_code),
                        );
                        let is_favorable_impact =
                            estimate.is_some_and(|value| value.marginal_minor <= 0);

                        let show_shelf_price =
                            estimate.is_some_and(|value| value.marginal_minor != product.price_minor);

                        let shelf_price = show_shelf_price.then(|| price.clone());

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
                                        class="icon-button icon-button-primary icon-button-product"
                                        on:click=move |_| {
                                            cart_items.update(|items| items.push(fixture_key.clone()));
                                            action_message
                                                .set(Some(format!("Added {announce_name} to basket.")));
                                        }
                                    >
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
                                            class="lucide lucide-plus-icon lucide-plus"
                                        >
                                            <path d="M5 12h14"></path>
                                            <path d="M12 5v14"></path>
                                        </svg>
                                    </button>
                                </div>
                            </li>
                        }
                    })
                    .collect_view()
                }}
            </ul>
        </section>
    }
}
