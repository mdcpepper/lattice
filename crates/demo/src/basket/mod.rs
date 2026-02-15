use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

#[cfg(target_arch = "wasm32")]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use humanize_duration::{Truncate, prelude::DurationExt};
use leptos::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use rusty_money::{Money, iso::Currency};
use slotmap::{SecondaryMap, SlotMap};

#[cfg(target_arch = "wasm32")]
use lattice::solvers::ilp::renderers::typst::MultiLayerRenderer;

use lattice::{
    basket::Basket,
    graph::PromotionGraph,
    items::{Item, groups::ItemGroup},
    products::{Product, ProductKey},
    promotions::{PromotionKey, PromotionMeta},
    receipt::Receipt,
};

use crate::{
    announce,
    promotions::{PromotionPill, bundle_pill_style},
};

pub(super) mod dock;
pub(super) mod line_item;
pub(super) mod summary;

use dock::{BasketMobileDock, install_mobile_dock_observer};
use line_item::BasketLine;
use summary::BasketSummary;

/// Solver inputs required to build basket/receipt view state.
#[derive(Debug)]
pub struct BasketSolverData {
    /// Catalog keyed by product key.
    pub product_meta_map: SlotMap<ProductKey, Product<'static>>,

    /// Fixture key -> product key lookup.
    pub product_key_by_fixture_key: HashMap<String, ProductKey>,

    /// Promotion graph built from fixtures.
    pub graph: PromotionGraph<'static>,

    /// Promotion key to display name.
    pub promotion_names: SecondaryMap<PromotionKey, String>,

    /// Promotion metadata keyed by promotion key.
    pub promotion_meta_map: SlotMap<PromotionKey, PromotionMeta>,

    /// Currency used by this demo fixture set.
    pub currency: &'static Currency,
}

/// Render model for an item line in the basket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BasketLineItem {
    /// Item index in the basket/cart.
    basket_index: usize,

    /// Product fixture key (used for add actions on this line).
    fixture_key: String,

    /// Product name.
    name: String,

    /// Base (pre-promotion) price.
    base_price: String,

    /// Final (post-promotion) price.
    final_price: String,

    /// Applied promotion pills.
    promotions: Vec<PromotionPill>,
}

/// Render model for the solved basket.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BasketViewModel {
    /// Solved line items.
    lines: Vec<BasketLineItem>,

    /// Basket subtotal from receipt.
    subtotal: String,

    /// Basket total from receipt.
    total: String,

    /// Savings from receipt.
    savings: String,

    /// Savings grouped by promotion.
    savings_breakdown: Vec<PromotionSavings>,

    /// Time taken by graph solver.
    solve_duration: String,
}

/// Render model for savings contribution by promotion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromotionSavings {
    /// Promotion display name.
    name: String,

    /// Savings amount for this promotion.
    savings: String,

    /// Number of discounted item applications for this promotion.
    item_applications: usize,

    /// Number of distinct bundles for this promotion.
    bundle_applications: usize,
}

fn build_basket(
    solver_data: &BasketSolverData,
    cart_fixture_keys: &[String],
) -> Result<Basket<'static>, String> {
    let mut basket_items: Vec<Item<'static>> = Vec::new();

    for fixture_key in cart_fixture_keys {
        let product_key = solver_data
            .product_key_by_fixture_key
            .get(fixture_key)
            .copied()
            .ok_or_else(|| format!("Product key not found in fixture: {fixture_key}"))?;

        let product = solver_data
            .product_meta_map
            .get(product_key)
            .ok_or_else(|| format!("Product metadata missing for fixture key: {fixture_key}"))?;

        basket_items.push(Item::with_tags(
            product_key,
            Money::from_minor(product.price.to_minor_units(), product.price.currency()),
            product.tags.clone(),
        ));
    }

    Basket::with_items(basket_items, solver_data.currency)
        .map_err(|error| format!("Failed to build basket: {error}"))
}

/// Solve only the basket total (minor units) for marginal-price estimation.
///
/// # Errors
///
/// Returns an error if basket construction or graph solving fails.
#[cfg(target_arch = "wasm32")]
pub fn solve_total_minor(
    solver_data: &BasketSolverData,
    cart_fixture_keys: &[String],
) -> Result<i64, String> {
    let basket = build_basket(solver_data, cart_fixture_keys)?;
    let item_group = ItemGroup::from(&basket);

    let solved = solver_data
        .graph
        .evaluate(&item_group)
        .map_err(|error| format!("Failed to solve promotion graph: {error}"))?;

    Ok(solved.total.to_minor_units())
}

fn solve_basket(
    solver_data: &BasketSolverData,
    cart_fixture_keys: &[String],
) -> Result<BasketViewModel, String> {
    let basket = build_basket(solver_data, cart_fixture_keys)?;

    let item_group = ItemGroup::from(&basket);

    #[cfg(target_arch = "wasm32")]
    let solve_started_at = monotonic_now();

    #[cfg(not(target_arch = "wasm32"))]
    let solve_started_at = Instant::now();

    let solved = solver_data
        .graph
        .evaluate(&item_group)
        .map_err(|error| format!("Failed to solve promotion graph: {error}"))?;

    #[cfg(target_arch = "wasm32")]
    let solve_elapsed = elapsed_since(solve_started_at);

    #[cfg(not(target_arch = "wasm32"))]
    let solve_elapsed = solve_started_at.elapsed();

    let receipt = Receipt::from_layered_result(&basket, solved)
        .map_err(|error| format!("Failed to build receipt: {error}"))?;

    let mut lines: Vec<BasketLineItem> = Vec::new();

    for (basket_index, item) in basket.iter().enumerate() {
        let product = solver_data
            .product_meta_map
            .get(item.product())
            .ok_or_else(|| "Missing product metadata while rendering basket".to_string())?;

        let applications = receipt.promotion_application_for_item(basket_index);

        let final_price = applications
            .and_then(|apps| apps.last().map(|app| app.final_price))
            .map_or_else(|| format_money(item.price()), |price| format_money(&price));

        let promotions = applications.map_or_else(Vec::new, |apps| {
            apps.iter()
                .filter_map(|app| {
                    solver_data
                        .promotion_names
                        .get(app.promotion_key)
                        .map(|label| PromotionPill {
                            label: label.clone(),
                            bundle_id: app.bundle_id + 1,
                            style: bundle_pill_style(app.bundle_id),
                        })
                })
                .collect()
        });

        let fixture_key = cart_fixture_keys
            .get(basket_index)
            .cloned()
            .ok_or_else(|| "Cart item index mismatch while rendering basket".to_string())?;

        lines.push(BasketLineItem {
            basket_index,
            fixture_key,
            name: product.name.clone(),
            base_price: format_money(item.price()),
            final_price,
            promotions,
        });
    }

    let savings = receipt
        .savings()
        .map_err(|error| format!("Failed to compute receipt savings: {error}"))?;

    let savings_breakdown = collect_promotion_savings(&receipt, solver_data)?;

    Ok(BasketViewModel {
        lines,
        subtotal: format_money(&receipt.subtotal()),
        total: format_money(&receipt.total()),
        savings: format!("-{}", format_money(&savings)),
        savings_breakdown,
        solve_duration: format_solve_duration(solve_elapsed),
    })
}

#[derive(Debug)]
struct PromotionAggregate {
    name: String,
    savings_minor: i64,
    item_applications: usize,
    bundle_ids: FxHashSet<usize>,
}

fn collect_promotion_savings(
    receipt: &Receipt<'_>,
    solver_data: &BasketSolverData,
) -> Result<Vec<PromotionSavings>, String> {
    let mut by_promotion: FxHashMap<PromotionKey, PromotionAggregate> = FxHashMap::default();

    for applications in receipt.promotion_applications().values() {
        for app in applications {
            let app_savings_minor = app
                .savings()
                .map_err(|error| format!("Failed to compute promotion savings: {error}"))?
                .to_minor_units();

            if app_savings_minor <= 0 {
                continue;
            }

            let promotion_name = solver_data
                .promotion_names
                .get(app.promotion_key)
                .cloned()
                .unwrap_or_else(|| "Unknown promotion".to_string());

            let aggregate =
                by_promotion
                    .entry(app.promotion_key)
                    .or_insert_with(|| PromotionAggregate {
                        name: promotion_name,
                        savings_minor: 0,
                        item_applications: 0,
                        bundle_ids: FxHashSet::default(),
                    });

            aggregate.savings_minor += app_savings_minor;
            aggregate.item_applications += 1;
            aggregate.bundle_ids.insert(app.bundle_id);
        }
    }

    let mut grouped: Vec<PromotionAggregate> = by_promotion.into_values().collect();

    grouped.sort_by(|left, right| {
        right
            .savings_minor
            .cmp(&left.savings_minor)
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(grouped
        .into_iter()
        .map(|aggregate| PromotionSavings {
            item_applications: aggregate.item_applications,
            bundle_applications: aggregate.bundle_ids.len(),
            savings: format!(
                "-{}",
                format_money(&Money::from_minor(
                    aggregate.savings_minor,
                    solver_data.currency
                ))
            ),
            name: aggregate.name,
        })
        .collect())
}

fn format_money(money: &Money<'_, Currency>) -> String {
    format!("{money}")
}

fn format_solve_duration(duration: Duration) -> String {
    if duration < Duration::from_millis(1) {
        return "< 1ms".to_string();
    }

    format!("{}", duration.human(Truncate::Nano))
}

#[cfg(target_arch = "wasm32")]
fn monotonic_now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or(0.0)
}

#[cfg(target_arch = "wasm32")]
fn elapsed_since(start_ms: f64) -> Duration {
    let elapsed_ms = (monotonic_now() - start_ms).max(0.0);

    Duration::from_secs_f64(elapsed_ms / 1_000.0)
}

#[cfg(target_arch = "wasm32")]
fn render_basket_typst(
    solver_data: &BasketSolverData,
    cart_fixture_keys: &[String],
) -> Result<String, String> {
    let basket = build_basket(solver_data, cart_fixture_keys)?;
    let item_group = ItemGroup::from(&basket);

    let mut renderer = MultiLayerRenderer::new_with_metadata(
        PathBuf::from("basket.typ"),
        &item_group,
        &solver_data.product_meta_map,
        &solver_data.promotion_meta_map,
    );

    solver_data
        .graph
        .evaluate_with_observer(&item_group, Some(&mut renderer))
        .map_err(|error| format!("Failed to capture ILP formulation: {error}"))?;

    Ok(renderer.render())
}

#[cfg(target_arch = "wasm32")]
fn js_value_message(error: wasm_bindgen::JsValue, fallback: &str) -> String {
    error.as_string().unwrap_or_else(|| fallback.to_string())
}

#[cfg(target_arch = "wasm32")]
fn download_text_file(filename: &str, content: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/plain;charset=utf-8");

    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));

    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)
        .map_err(|error| js_value_message(error, "Failed to create download blob"))?;

    let object_url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| js_value_message(error, "Failed to create download URL"))?;

    let window = web_sys::window().ok_or_else(|| "Window is unavailable".to_string())?;

    let document = window
        .document()
        .ok_or_else(|| "Document is unavailable".to_string())?;

    let body = document
        .body()
        .ok_or_else(|| "Document body is unavailable".to_string())?;

    let anchor = document
        .create_element("a")
        .map_err(|error| js_value_message(error, "Failed to create download link"))?;

    anchor
        .set_attribute("href", &object_url)
        .map_err(|error| js_value_message(error, "Failed to set download URL"))?;

    anchor
        .set_attribute("download", filename)
        .map_err(|error| js_value_message(error, "Failed to set filename"))?;

    body.append_child(&anchor)
        .map_err(|error| js_value_message(error, "Failed to add temporary download link"))?;

    let anchor_element: web_sys::HtmlElement = anchor
        .dyn_into()
        .map_err(|_error| "Failed to prepare download link".to_string())?;

    anchor_element.click();

    let _ = body.remove_child(&anchor_element);

    web_sys::Url::revoke_object_url(&object_url)
        .map_err(|error| js_value_message(error, "Failed to release download URL"))?;

    Ok(())
}

#[component]
fn BasketBody(
    basket: BasketViewModel,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    let summary = view! {
        <BasketSummary
            subtotal=basket.subtotal.clone()
            savings=basket.savings.clone()
            savings_breakdown=basket.savings_breakdown.clone()
            total=basket.total.clone()
        />
    };

    if basket.lines.is_empty() {
        view! {
            <div>
                <p class="basket-empty">"Your basket is empty."</p>
                {summary}
            </div>
        }
        .into_any()
    } else {
        view! {
            <div>
                <ul class="basket-lines">
                    {basket
                        .lines
                        .into_iter()
                        .map(|line| {
                            view! {
                                <BasketLine
                                    line=line
                                    cart_items=cart_items
                                    action_message=action_message
                                    add_icon_confirmations=add_icon_confirmations
                                />
                            }
                        })
                        .collect_view()}
                </ul>
                {summary}
            </div>
        }
        .into_any()
    }
}

#[component]
fn BasketHeading(item_count: usize, basket_total: Option<String>) -> impl IntoView {
    view! {
        <h2
            class="panel-title panel-title-spaced basket-mobile-dock-hide-target"
            data-dock-hide-key="heading"
        >
            <div class="panel-title-row">
                <span class="panel-title-leading basket-title-label">
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
                        class="basket-title-icon lucide lucide-shopping-basket-icon lucide-shopping-basket"
                        aria-hidden="true"
                    >
                        <path d="m15 11-1 9"></path>
                        <path d="m19 11-4-7"></path>
                        <path d="M2 11h20"></path>
                        <path d="m3.5 11 1.6 7.4a2 2 0 0 0 2 1.6h9.8a2 2 0 0 0 2-1.6l1.7-7.4"></path>
                        <path d="M4.5 15.5h15"></path>
                        <path d="m5 11 4-7"></path>
                        <path d="m9 11 1 9"></path>
                    </svg>
                    <span>{format!("Basket ({item_count})")}</span>
                </span>
                {basket_total.map_or_else(
                    || ().into_any(),
                    |total| view! { <span class="panel-title-trailing">{total}</span> }.into_any(),
                )}
            </div>
        </h2>
    }
}

#[component]
fn BasketPanelMeta(
    solve_time_text: RwSignal<String>,
    solver_data: Arc<BasketSolverData>,
    cart_items: RwSignal<Vec<String>>,
    live_message: RwSignal<(u64, String)>,
) -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (&solver_data, cart_items, live_message);

    move || {
        let value = solve_time_text.get();

        let solve_meta = if value.is_empty() {
            ().into_any()
        } else {
            #[cfg(target_arch = "wasm32")]
            let on_download = {
                let solver_data = Arc::clone(&solver_data);

                move |_| {
                    let cart_snapshot = cart_items.get_untracked();

                    let result = render_basket_typst(&solver_data, &cart_snapshot)
                        .and_then(|typst| download_text_file("basket.typ", &typst));

                    match result {
                        Ok(()) => announce(live_message, "Downloaded basket.typ.".to_string()),
                        Err(error) => announce(live_message, format!("Download failed: {error}")),
                    }
                }
            };

            let download_link = {
                #[cfg(target_arch = "wasm32")]
                {
                    view! {
                        <button
                            type="button"
                            class="panel-meta-download"
                            on:click=on_download
                            title="Download ILP formulation as basket.typ"
                            aria-label="Download basket.typ"
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
                                class="panel-meta-download-icon lucide lucide-file-down-icon lucide-file-down"
                                aria-hidden="true"
                            >
                                <path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"></path>
                                <path d="M14 2v5a1 1 0 0 0 1 1h5"></path>
                                <path d="M12 18v-6"></path>
                                <path d="m9 15 3 3 3-3"></path>
                            </svg>
                            <span><span class="sr-only">"download" </span>".typ"</span>
                        </button>
                    }
                    .into_any()
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ().into_any()
                }
            };

            view! {
                <p class="panel-meta">
                    <span>{value}</span>
                    {download_link}
                </p>
            }
            .into_any()
        };

        view! {
            <div class="panel-meta-block">
                {solve_meta}
                <dl class="panel-meta-notes">
                    <dt>"10% Off Coca Cola"</dt>
                    <dd>"A supplier-funded discount that applies first and can stack with other retailer promotions."</dd>

                    <dt>"£3.80 Meal Deal"</dt>
                    <dd>"Applies to 1 "<kbd>"main"</kbd>" + 1 "<kbd>"drink"</kbd>" + 1 "<kbd>"snack"</kbd>" as a fixed bundle price."</dd>

                    <dt>"Buy One Get One Free Drinks"</dt>
                    <dd>"Makes every second drink free."</dd>

                    <dt>"3-for-2 Vitamins"</dt>
                    <dd>"Makes every third vitamin free (items tagged "<kbd>"vitamins"</kbd>")."</dd>

                    <dt>"Sports Nutrition Tiered Saver"</dt>
                    <dd>
                        "Applies to items tagged "<kbd>"sports-nutrition"</kbd>": spend £10 to under £50 for 10% off each item, or £50 to under £100 for 20% off each item."
                    </dd>

                    <dt>"5% Staff discount"</dt>
                    <dd>
                        "Applies only to items that are not part of any base promotions above, and does not apply to items tagged "
                        <kbd>"staff-discount-exempt"</kbd>" (for example, newspaper, supplements, and vitamins)."
                    </dd>
                </dl>
            </div>
        }
        .into_any()
    }
}

fn render_basket_panel_content(
    solver_data: &Arc<BasketSolverData>,
    cart_items: RwSignal<Vec<String>>,
    solve_time_text: RwSignal<String>,
    live_message: RwSignal<(u64, String)>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
    dock_hidden: RwSignal<bool>,
) -> AnyView {
    let cart_snapshot = cart_items.get();
    let item_count = cart_snapshot.len();

    match solve_basket(solver_data, &cart_snapshot) {
        Ok(basket) => {
            if item_count == 0 {
                solve_time_text.set(String::new());
            } else {
                solve_time_text.set(basket.solve_duration.clone());
            }

            if let Some(action) = action_message.get_untracked() {
                announce(live_message, format!("{action}, total {}.", basket.total));
                action_message.set(None);
            }

            let basket_total = basket.total.clone();
            let basket_savings = basket.savings.clone();

            view! {
                <BasketMobileDock
                    item_count=item_count
                    basket_total=Some(basket_total.clone())
                    savings=Some(basket_savings)
                    hidden=dock_hidden
                />
                <div class="basket-panel-main">
                    <BasketHeading item_count=item_count basket_total=Some(basket_total) />
                    <div class="panel-card">
                        <BasketBody
                            basket=basket
                            cart_items=cart_items
                            action_message=action_message
                            add_icon_confirmations=add_icon_confirmations
                        />
                    </div>
                </div>
            }
            .into_any()
        }
        Err(error_message) => {
            solve_time_text.set(String::new());
            view! {
                <BasketMobileDock
                    item_count=item_count
                    basket_total=None
                    savings=None
                    hidden=dock_hidden
                />
                <div class="basket-panel-main">
                    <BasketHeading item_count=item_count basket_total=None />
                    <div class="panel-card">
                        <p class="error-text">{error_message}</p>
                    </div>
                </div>
            }
            .into_any()
        }
    }
}

/// Basket panel component.
#[component]
pub fn BasketPanel(
    /// Solver-ready data loaded from fixtures.
    solver_data: Arc<BasketSolverData>,
    /// Shared cart fixture keys.
    cart_items: RwSignal<Vec<String>>,
    /// Text showing the last solve duration.
    solve_time_text: RwSignal<String>,
    /// Live-region announcement signal.
    live_message: RwSignal<(u64, String)>,
    /// Ephemeral action message shown to the user.
    action_message: RwSignal<Option<String>>,
) -> impl IntoView {
    let add_icon_confirmations = RwSignal::new(BTreeSet::<String>::new());
    let dock_hidden = RwSignal::new(false);
    let panel_solver_data = solver_data;
    let meta_solver_data = Arc::clone(&panel_solver_data);

    install_mobile_dock_observer(cart_items, dock_hidden);

    view! {
        <aside id="basket-panel" class="basket-panel">
            <div class="basket-panel-content">
                {move || {
                    render_basket_panel_content(
                        &panel_solver_data,
                        cart_items,
                        solve_time_text,
                        live_message,
                        action_message,
                        add_icon_confirmations,
                        dock_hidden,
                    )
                }}
                <BasketPanelMeta
                    solve_time_text=solve_time_text
                    solver_data=meta_solver_data
                    cart_items=cart_items
                    live_message=live_message
                />
            </div>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use leptos::prelude::*;
    use rusty_money::{Money, iso};
    use slotmap::{SecondaryMap, SlotMap};

    use lattice::{
        basket::Basket, graph::PromotionGraph, items::groups::ItemGroup, products::Product,
        receipt::Receipt, tags::string::StringTagCollection,
    };
    use testresult::TestResult;

    use crate::basket::{
        line_item::{
            clear_icon_confirmation, is_icon_confirmed, remove_line_item, start_icon_confirmation,
        },
        summary::format_application_summary,
    };

    use super::*;

    // Test helper functions that manipulate icon confirmation state
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

    #[test]
    fn test_is_icon_confirmed_returns_true_when_present() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        start_icon_confirmation(confirmed_icons, "test-key");

        assert!(is_icon_confirmed(confirmed_icons, "test-key"));
    }

    #[test]
    fn test_is_icon_confirmed_returns_false_when_absent() {
        let confirmed_icons = RwSignal::new(BTreeSet::<String>::new());

        assert!(!is_icon_confirmed(confirmed_icons, "test-key"));
    }

    // Test remove_line_item function
    #[test]
    fn test_remove_line_item_removes_exact_match() {
        let mut items = vec![
            "item1".to_string(),
            "item2".to_string(),
            "item3".to_string(),
        ];

        remove_line_item(&mut items, 1, "item2");

        assert_eq!(items, vec!["item1".to_string(), "item3".to_string()]);
    }

    #[test]
    fn test_remove_line_item_removes_first_occurrence_when_index_mismatch() {
        let mut items = vec![
            "item1".to_string(),
            "item2".to_string(),
            "item2".to_string(),
        ];

        remove_line_item(&mut items, 0, "item2");

        assert_eq!(items, vec!["item1".to_string(), "item2".to_string()]);
    }

    #[test]
    fn test_remove_line_item_handles_out_of_bounds() {
        let mut items = vec!["item1".to_string(), "item2".to_string()];

        remove_line_item(&mut items, 10, "item1");

        assert_eq!(items, vec!["item2".to_string()]);
    }

    #[test]
    fn test_remove_line_item_nonexistent_key() {
        let mut items = vec!["item1".to_string(), "item2".to_string()];

        remove_line_item(&mut items, 0, "nonexistent");

        assert_eq!(items, vec!["item1".to_string(), "item2".to_string()]);
    }

    #[test]
    fn test_remove_line_item_empty_list() {
        let mut items: Vec<String> = vec![];

        remove_line_item(&mut items, 0, "item1");

        assert!(items.is_empty());
    }

    // Test format_money function
    #[test]
    fn test_format_money_gbp() {
        let money = Money::from_minor(1250, iso::GBP);

        let result = format_money(&money);

        assert_eq!(result, "£12.50");
    }

    #[test]
    fn test_format_money_usd() {
        let money = Money::from_minor(999, iso::USD);

        let result = format_money(&money);

        assert_eq!(result, "$9.99");
    }

    #[test]
    fn test_format_money_zero() {
        let money = Money::from_minor(0, iso::GBP);

        let result = format_money(&money);

        assert_eq!(result, "£0.00");
    }

    #[test]
    fn test_format_money_large_amount() {
        let money = Money::from_minor(123_456, iso::EUR);

        let result = format_money(&money);

        // EUR uses European number format (comma for decimal, period for thousands)
        assert_eq!(result, "€1.234,56");
    }

    #[test]
    fn test_format_solve_duration_sub_millisecond() {
        let result = format_solve_duration(Duration::from_nanos(999_999));

        assert_eq!(result, "< 1ms");
    }

    #[test]
    fn test_format_solve_duration_one_millisecond() {
        let result = format_solve_duration(Duration::from_millis(1));

        assert_eq!(result, "1ms");
    }

    #[test]
    fn test_format_application_summary_mixed_counts() {
        let result = format_application_summary(3, 1);

        assert_eq!(result, "× 1 (3 items)");
    }

    #[test]
    fn test_format_application_summary_singular_counts() {
        let result = format_application_summary(1, 1);

        assert_eq!(result, "× 1 (1 item)");
    }

    #[test]
    fn test_build_basket_empty_cart() -> TestResult {
        let solver_data = create_minimal_solver_data()?;
        let cart_fixture_keys: Vec<String> = vec![];

        let result = build_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_ok());

        let basket = result?;

        assert_eq!(basket.len(), 0);

        Ok(())
    }

    #[test]
    fn test_build_basket_unknown_fixture_key() -> TestResult {
        let solver_data = create_minimal_solver_data()?;
        let cart_fixture_keys = vec!["unknown-key".to_string()];

        let result = build_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_err());
        assert!(result.is_err_and(|error| error.contains("Product key not found in fixture")));

        Ok(())
    }

    #[test]
    fn test_build_basket_with_valid_products() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let cart_fixture_keys = vec!["product1".to_string()];

        let result = build_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_ok());

        let basket = result?;

        assert_eq!(basket.len(), 1);

        Ok(())
    }

    #[test]
    fn test_build_basket_multiple_items() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let cart_fixture_keys = vec![
            "product1".to_string(),
            "product1".to_string(),
            "product2".to_string(),
        ];

        let result = build_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_ok());

        let basket = result?;

        assert_eq!(basket.len(), 3);

        Ok(())
    }

    #[test]
    fn test_solve_basket_empty() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let cart_fixture_keys: Vec<String> = vec![];

        let result = solve_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_ok());
        let view_model = result?;

        assert_eq!(view_model.lines.len(), 0);
        assert!(!view_model.solve_duration.is_empty());

        Ok(())
    }

    #[test]
    fn test_solve_basket_single_item() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let cart_fixture_keys = vec!["product1".to_string()];

        let result = solve_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_ok());

        let view_model = result?;

        assert_eq!(view_model.lines.len(), 1);
        assert_eq!(view_model.lines[0].name, "Test Product 1");

        Ok(())
    }

    #[test]
    fn test_solve_basket_unknown_product() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let cart_fixture_keys = vec!["nonexistent".to_string()];

        let result = solve_basket(&solver_data, &cart_fixture_keys);

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_collect_promotion_savings_empty_receipt() -> TestResult {
        let solver_data = create_test_solver_data()?;
        let basket = Basket::new(iso::GBP);
        let items = ItemGroup::from(&basket);

        let solved = solver_data.graph.evaluate(&items)?;
        let receipt = Receipt::from_layered_result(&basket, solved)?;

        let result = collect_promotion_savings(&receipt, &solver_data);

        assert!(result.is_ok());

        let savings = result?;

        assert_eq!(savings.len(), 0);

        Ok(())
    }

    #[test]
    fn test_collect_promotion_savings_meal_deal_counts_items_and_bundles() -> TestResult {
        let solver_data = create_demo_solver_data()?;

        let cart_fixture_keys = vec![
            "sandwich".to_string(),
            "water".to_string(),
            "crisps".to_string(),
        ];

        let basket = build_basket(&solver_data, &cart_fixture_keys)?;
        let item_group = ItemGroup::from(&basket);

        let solved = solver_data.graph.evaluate(&item_group)?;
        let receipt = Receipt::from_layered_result(&basket, solved)?;

        let savings = collect_promotion_savings(&receipt, &solver_data)?;

        let meal_deal = savings
            .iter()
            .find(|entry| entry.name == "£3.80 Meal Deal")
            .ok_or_else(|| "Expected meal deal entry in savings breakdown".to_string())?;

        assert_eq!(meal_deal.item_applications, 3);
        assert_eq!(meal_deal.bundle_applications, 1);

        Ok(())
    }

    fn create_minimal_solver_data() -> TestResult<BasketSolverData> {
        let product_meta_map = SlotMap::with_key();
        let product_key_by_fixture_key = HashMap::new();
        let promotion_names = SecondaryMap::new();
        let promotion_meta_map = SlotMap::with_key();

        let graph = PromotionGraph::single_layer(Vec::new())?;

        Ok(BasketSolverData {
            product_meta_map,
            product_key_by_fixture_key,
            graph,
            promotion_names,
            promotion_meta_map,
            currency: iso::GBP,
        })
    }

    fn create_test_solver_data() -> TestResult<BasketSolverData> {
        let mut product_meta_map = SlotMap::with_key();
        let mut product_key_by_fixture_key = HashMap::new();

        let promotion_names = SecondaryMap::new();
        let promotion_meta_map = SlotMap::with_key();

        // Add test products
        let product1 = Product {
            name: "Test Product 1".to_string(),
            price: Money::from_minor(100, iso::GBP),
            tags: StringTagCollection::from_strs(&[]),
        };

        let product2 = Product {
            name: "Test Product 2".to_string(),
            price: Money::from_minor(200, iso::GBP),
            tags: StringTagCollection::from_strs(&[]),
        };

        let key1 = product_meta_map.insert(product1);
        let key2 = product_meta_map.insert(product2);

        product_key_by_fixture_key.insert("product1".to_string(), key1);
        product_key_by_fixture_key.insert("product2".to_string(), key2);

        let graph = PromotionGraph::single_layer(Vec::new())?;

        Ok(BasketSolverData {
            product_meta_map,
            product_key_by_fixture_key,
            graph,
            promotion_names,
            promotion_meta_map,
            currency: iso::GBP,
        })
    }

    fn create_demo_solver_data() -> TestResult<BasketSolverData> {
        let products_yaml = include_str!("../../../../fixtures/products/demo.yml");
        let promotions_yaml = include_str!("../../../../fixtures/promotions/demo.yml");

        let loaded_products =
            crate::products::load_products(products_yaml).map_err(testresult::TestError::from)?;
        let loaded_promotions = crate::promotions::load_promotions(promotions_yaml)
            .map_err(testresult::TestError::from)?;

        Ok(BasketSolverData {
            product_meta_map: loaded_products.product_meta_map,
            product_key_by_fixture_key: loaded_products.product_key_by_fixture_key,
            graph: loaded_promotions.graph,
            promotion_names: loaded_promotions.promotion_names,
            promotion_meta_map: loaded_promotions.promotion_meta_map,
            currency: loaded_products.currency,
        })
    }
}
