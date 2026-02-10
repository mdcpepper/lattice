use std::{collections::HashMap, sync::Arc};

#[cfg(target_arch = "wasm32")]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use humanize_duration::{Truncate, prelude::DurationExt};
use leptos::prelude::*;
use rusty_money::{Money, iso::Currency};
use slotmap::{SecondaryMap, SlotMap};

use lattice::{
    basket::Basket,
    graph::PromotionGraph,
    items::{Item, groups::ItemGroup},
    products::{Product, ProductKey},
    promotions::PromotionKey,
    receipt::Receipt,
};

use crate::{
    announce,
    promotions::{PromotionPill, bundle_pill_style},
};

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
) -> impl IntoView {
    let basket_index = line.basket_index;
    let fixture_key = line.fixture_key.clone();
    let item_name_for_add = line.name.clone();
    let item_name_for_remove = line.name.clone();

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
                                if basket_index < items.len() {
                                    items.remove(basket_index);
                                }
                            });

                            action_message
                                .set(Some(format!("Removed {item_name_for_remove} from basket.")));
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
                        class="icon-button icon-button-primary icon-button-compact"
                        on:click=move |_| {
                            cart_items.update(|items| items.push(fixture_key.clone()));
                            action_message
                                .set(Some(format!("Added {item_name_for_add} to basket.")));
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
            </div>
        </li>
    }
}

#[component]
fn BasketBody(
    basket: BasketViewModel,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
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
                        .map(|line| view! { <BasketLine line=line cart_items=cart_items action_message=action_message/> })
                        .collect_view()}
                </ul>
                {summary}
            </div>
        }
        .into_any()
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
    view! {
        <aside class="basket-panel">
            {let solver_data = solver_data;
                move || {
                    let cart_snapshot = cart_items.get();
                    let item_count = cart_snapshot.len();

                    match solve_basket(&solver_data, &cart_snapshot) {
                        Ok(basket) => {
                            solve_time_text.set(basket.solve_duration.clone());

                            if let Some(action) = action_message.get_untracked() {
                                announce(live_message, format!("{action}, total {}.", basket.total));
                                action_message.set(None);
                            }

                            let basket_total = basket.total.clone();

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
                                        <span class="panel-title-trailing">{basket_total}</span>
                                    </div>
                                </h2>
                                <div class="panel-card">
                                    <BasketBody basket=basket cart_items=cart_items action_message=action_message/>
                                </div>
                            }
                                .into_any()
                        }
                        Err(error_message) => view! {
                            {solve_time_text.set(String::new());}
                            <h2 class="panel-title panel-title-spaced">
                                <div class="panel-title-row">
                                    <span class="basket-title-label">
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
                                </div>
                            </h2>
                            <div class="panel-card">
                                <p class="error-text">{error_message}</p>
                            </div>
                        }
                            .into_any(),
                    }
                }}
            {move || {
                let value = solve_time_text.get();
                if value.is_empty() {
                    ().into_any()
                } else {
                    view! {
                        <p class="panel-meta">{value}</p>
                    }
                        .into_any()
                }
            }}
        </aside>
    }
}
