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

/// Products panel component.
#[component]
pub fn ProductsPanel(
    products: Arc<Vec<ProductListItem>>,
    cart_items: RwSignal<Vec<String>>,
) -> impl IntoView {
    let products = Arc::unwrap_or_clone(products);

    view! {
        <section class="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
            <h2 class="mb-4 text-lg font-semibold">"Products"</h2>
            <ul class="space-y-3">
                {products
                    .iter()
                    .map(|product| {
                        let product_name = product.name.clone();
                        let price = product.price.clone();
                        let fixture_key = product.fixture_key.clone();

                        view! {
                            <li class="flex items-center justify-between gap-3 rounded-md border border-slate-200 px-3 py-2">
                                <div class="min-w-0">
                                    <p class="truncate text-sm font-medium">{product_name}</p>
                                    <p class="text-sm text-slate-600">{price}</p>
                                </div>
                                <button
                                    type="button"
                                    aria-label="Add product to basket"
                                    class="shrink-0 rounded-md bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700"
                                    on:click=move |_| {
                                        cart_items.update(|items| items.push(fixture_key.clone()));
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
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </section>
    }
}
