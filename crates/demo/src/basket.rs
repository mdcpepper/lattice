use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[cfg(target_arch = "wasm32")]
use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use humanize_duration::{Truncate, prelude::DurationExt};
use leptos::prelude::*;
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

fn start_icon_confirmation(confirmed_icons: RwSignal<HashSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.insert(icon_key.to_string());
    });
}

fn clear_icon_confirmation(confirmed_icons: RwSignal<HashSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.remove(icon_key);
    });
}

fn is_icon_confirmed(confirmed_icons: RwSignal<HashSet<String>>, icon_key: &str) -> bool {
    confirmed_icons.with(|states| states.contains(icon_key))
}

fn remove_line_item(items: &mut Vec<String>, basket_index: usize, fixture_key: &str) {
    if items
        .get(basket_index)
        .is_some_and(|item_key| item_key == fixture_key)
    {
        items.remove(basket_index);
        return;
    }

    if let Some(position) = items.iter().position(|item_key| item_key == fixture_key) {
        items.remove(position);
    }
}

/// Solver inputs required to build basket/receipt view state.
#[derive(Debug)]
pub struct BasketSolverData {
    /// Catalog keyed by product key.
    pub product_meta_map: SlotMap<ProductKey, Product<'static>>,

    /// Fixture key -> product key lookup.
    pub product_key_by_fixture_key: HashMap<String, ProductKey>,

    /// Promotion graph built from fixtures.
    pub graph: PromotionGraph<'static>,

    /// Promotion key -> display name.
    pub promotion_names: SecondaryMap<PromotionKey, String>,

    /// Promotion metadata keyed by promotion key.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        expect(
            dead_code,
            reason = "This field is read only in wasm32 for Typst download support."
        )
    )]
    pub promotion_meta_map: SlotMap<PromotionKey, PromotionMeta>,

    /// Currency used by this demo fixture set.
    pub currency: &'static Currency,
}

/// Render model for an item line in the basket.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BasketLineItem {
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
struct PromotionSavings {
    /// Promotion display name.
    name: String,

    /// Savings amount for this promotion.
    savings: String,
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
                            bundle_id: app.bundle_id,
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
        solve_duration: format!("{}", solve_elapsed.human(Truncate::Nano)),
    })
}

fn collect_promotion_savings(
    receipt: &Receipt<'_>,
    solver_data: &BasketSolverData,
) -> Result<Vec<PromotionSavings>, String> {
    let mut savings_by_promotion: HashMap<String, i64> = HashMap::new();

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

            savings_by_promotion
                .entry(promotion_name)
                .and_modify(|total| *total += app_savings_minor)
                .or_insert(app_savings_minor);
        }
    }

    let mut grouped: Vec<(String, i64)> = savings_by_promotion.into_iter().collect();
    grouped.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    Ok(grouped
        .into_iter()
        .map(|(name, savings_minor)| PromotionSavings {
            name,
            savings: format!(
                "-{}",
                format_money(&Money::from_minor(savings_minor, solver_data.currency))
            ),
        })
        .collect())
}

fn format_money(money: &Money<'_, Currency>) -> String {
    format!("{money}")
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

/// Basket panel component.
#[component]
fn BasketSummary(
    subtotal: String,
    savings: String,
    savings_breakdown: Vec<PromotionSavings>,
    total: String,
) -> impl IntoView {
    view! {
        <div class="basket-summary">
            <p class="basket-summary-row">
                <span>"Subtotal"</span>
                <span>{subtotal}</span>
            </p>
            <details class="basket-savings">
                <summary class="basket-savings-summary">
                    <span>
                        <span>"Savings"</span>
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
                            aria-hidden="true"
                        >
                            <path d="m9 18 6-6-6-6"></path>
                        </svg>
                    </span>
                    <span>{savings}</span>
                </summary>

                <div class="basket-savings-body">
                    {if savings_breakdown.is_empty() {
                        view! { <p>"No promotion savings applied."</p> }.into_any()
                    } else {
                        view! {
                            <ul>
                                {savings_breakdown
                                    .into_iter()
                                    .map(|entry| {
                                        view! {
                                            <li>
                                                <span>{entry.name}</span>
                                                <span>{entry.savings}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                            .into_any()
                    }}
                </div>
            </details>

            <p class="basket-total-row">
                <span>"Total"</span>
                <span>{total}</span>
            </p>
        </div>
    }
}

#[component]
fn BasketLine(
    line: BasketLineItem,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<HashSet<String>>,
) -> impl IntoView {
    let basket_index = line.basket_index;

    let fixture_key = line.fixture_key.clone();
    let remove_fixture_key = line.fixture_key.clone();
    let line_key = format!("{basket_index}:{}", line.fixture_key);

    let item_name_for_add = line.name.clone();
    let item_name_for_remove = line.name.clone();

    let add_icon_key = format!("add:{line_key}");
    let add_icon_key_for_class = add_icon_key.clone();
    let add_icon_key_for_click = add_icon_key.clone();
    let add_icon_key_for_animation_end = add_icon_key.clone();

    let add_button_label = format!(
        "Add another {} ({}) to basket",
        item_name_for_add.clone(),
        line.final_price
    );

    let remove_button_label = format!(
        "Remove {} ({}) from basket",
        item_name_for_remove.clone(),
        line.final_price
    );

    let has_discount = line.base_price != line.final_price;
    let promotion_pills = line.promotions;

    view! {
        <li>
            <div class="basket-line-content">
                <div>
                    <div class="basket-line-header">
                        <p class="basket-line-name">{line.name}</p>
                        <div class="basket-line-price">
                            {if has_discount {
                                view! {
                                    <span class="basket-line-base-price">{line.base_price}</span>
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }}
                            <span class="basket-line-final-price">{line.final_price}</span>
                        </div>
                    </div>
                    {if promotion_pills.is_empty() {
                        ().into_any()
                    } else {
                        view! {
                            <div class="basket-line-pills">
                                {promotion_pills
                                    .into_iter()
                                    .map(|pill| {
                                        let pill_text = pill.label.clone();

                                        view! {
                                            <span
                                                class="basket-line-pill"
                                                style=pill.style
                                            >
                                                {pill_text}
                                            </span>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }}
                </div>

                <div>
                    <button
                        type="button"
                        aria-label=remove_button_label
                        class="icon-button icon-button-secondary icon-button-compact"
                        on:click=move |_| {
                            cart_items.update(|items| {
                                remove_line_item(items, basket_index, &remove_fixture_key);
                            });

                            action_message.set(Some(format!(
                                "Removed {item_name_for_remove} from basket."
                            )));
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
                            class="lucide lucide-minus-icon lucide-minus"
                        >
                            <path d="M5 12h14"></path>
                        </svg>
                    </button>
                    <button
                        type="button"
                        aria-label=add_button_label
                        class=move || {
                            if is_icon_confirmed(
                                add_icon_confirmations,
                                &add_icon_key_for_class,
                            ) {
                                "icon-button icon-button-primary icon-button-compact icon-button-confirmed"
                            } else {
                                "icon-button icon-button-primary icon-button-compact"
                            }
                        }
                        on:click=move |_| {
                            start_icon_confirmation(
                                add_icon_confirmations,
                                &add_icon_key_for_click,
                            );

                            cart_items.update(|items| items.push(fixture_key.clone()));

                            action_message
                                .set(Some(format!("Added {item_name_for_add} to basket.")));
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
                                    clear_icon_confirmation(
                                        add_icon_confirmations,
                                        &add_icon_key_for_animation_end,
                                    );
                                }
                            >
                                <path d="M20 6 9 17l-5-5"></path>
                            </svg>
                        </span>
                    </button>
                </div>
            </div>
        </li>
    }
}

#[component]
fn BasketBody(
    basket: BasketViewModel,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<HashSet<String>>,
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
        <h2 class="panel-title panel-title-spaced">
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

        if value.is_empty() {
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
        }
    }
}

fn render_basket_panel_content(
    solver_data: &Arc<BasketSolverData>,
    cart_items: RwSignal<Vec<String>>,
    solve_time_text: RwSignal<String>,
    live_message: RwSignal<(u64, String)>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<HashSet<String>>,
) -> AnyView {
    let cart_snapshot = cart_items.get();
    let item_count = cart_snapshot.len();

    match solve_basket(solver_data, &cart_snapshot) {
        Ok(basket) => {
            solve_time_text.set(basket.solve_duration.clone());

            if let Some(action) = action_message.get_untracked() {
                announce(live_message, format!("{action}, total {}.", basket.total));
                action_message.set(None);
            }

            let basket_total = basket.total.clone();

            view! {
                <BasketHeading item_count=item_count basket_total=Some(basket_total) />
                <div class="panel-card">
                    <BasketBody
                        basket=basket
                        cart_items=cart_items
                        action_message=action_message
                        add_icon_confirmations=add_icon_confirmations
                    />
                </div>
            }
            .into_any()
        }
        Err(error_message) => {
            solve_time_text.set(String::new());
            view! {
                <BasketHeading item_count=item_count basket_total=None />
                <div class="panel-card">
                    <p class="error-text">{error_message}</p>
                </div>
            }
            .into_any()
        }
    }
}

/// Basket panel component.
#[component]
pub fn BasketPanel(
    solver_data: Arc<BasketSolverData>,
    cart_items: RwSignal<Vec<String>>,
    solve_time_text: RwSignal<String>,
    live_message: RwSignal<(u64, String)>,
    action_message: RwSignal<Option<String>>,
) -> impl IntoView {
    let add_icon_confirmations = RwSignal::new(HashSet::<String>::new());
    let panel_solver_data = solver_data;
    let meta_solver_data = Arc::clone(&panel_solver_data);

    view! {
        <aside class="basket-panel">
            {move || {
                render_basket_panel_content(
                    &panel_solver_data,
                    cart_items,
                    solve_time_text,
                    live_message,
                    action_message,
                    add_icon_confirmations,
                )
            }}
            <BasketPanelMeta
                solve_time_text=solve_time_text
                solver_data=meta_solver_data
                cart_items=cart_items
                live_message=live_message
            />
        </aside>
    }
}
