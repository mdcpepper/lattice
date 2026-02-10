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

use crate::promotions::{PromotionPill, bundle_pill_style};

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

    /// Time taken by graph solver.
    solve_duration: String,
}

fn solve_basket(
    solver_data: &BasketSolverData,
    cart_fixture_keys: &[String],
) -> Result<BasketViewModel, String> {
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

    let basket = Basket::with_items(basket_items, solver_data.currency)
        .map_err(|error| format!("Failed to build basket: {error}"))?;

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

    Ok(BasketViewModel {
        lines,
        subtotal: format_money(&receipt.subtotal()),
        total: format_money(&receipt.total()),
        savings: format!("-{}", format_money(&savings)),
        solve_duration: format!("{}", solve_elapsed.human(Truncate::Nano)),
    })
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
fn BasketSummary(subtotal: String, savings: String, total: String) -> impl IntoView {
    view! {
        <div class="mt-4 space-y-1 border-t border-slate-200 pt-3 text-sm">
            <p class="flex items-center justify-between">
                <span class="text-slate-600">"Subtotal"</span>
                <span>{subtotal}</span>
            </p>
            <p class="flex items-center justify-between">
                <span class="text-slate-600">"Savings"</span>
                <span>{savings}</span>
            </p>
            <p class="flex items-center justify-between font-semibold">
                <span>"Total"</span>
                <span>{total}</span>
            </p>
        </div>
    }
}

#[component]
fn BasketLine(line: BasketLineItem, cart_items: RwSignal<Vec<String>>) -> impl IntoView {
    let basket_index = line.basket_index;
    let fixture_key = line.fixture_key.clone();
    let has_discount = line.base_price != line.final_price;
    let promotion_pills = line.promotions;

    view! {
        <li>
            <div class="flex items-start justify-between gap-2">
                <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-3">
                        <p class="min-w-0 flex-1 truncate text-sm font-medium">{line.name}</p>
                        <div class="shrink-0 text-right">
                            {if has_discount {
                                view! {
                                    <span class="mr-2 text-xs text-slate-500 line-through">{line.base_price}</span>
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }}
                            <span class="text-sm text-slate-700">{line.final_price}</span>
                        </div>
                    </div>
                    {if promotion_pills.is_empty() {
                        ().into_any()
                    } else {
                        view! {
                            <div class="mt-1.5 flex flex-wrap gap-1.5">
                                {promotion_pills
                                    .into_iter()
                                    .map(|pill| {
                                        let pill_text = pill.label.clone();

                                        view! {
                                            <span
                                                class="inline-flex items-center rounded-full border px-2 py-0.5 text-[11px] font-medium"
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

                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        aria-label="Remove item from basket"
                        class="rounded-md border border-slate-300 px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100"
                        on:click=move |_| {
                            cart_items.update(|items| {
                                if basket_index < items.len() {
                                    items.remove(basket_index);
                                }
                            });
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
                        aria-label="Add another item to basket"
                        class="rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white hover:bg-slate-700"
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
                </div>
            </div>
        </li>
    }
}

#[component]
fn BasketBody(basket: BasketViewModel, cart_items: RwSignal<Vec<String>>) -> impl IntoView {
    let summary = view! {
        <BasketSummary
            subtotal=basket.subtotal.clone()
            savings=basket.savings.clone()
            total=basket.total.clone()
        />
    };

    if basket.lines.is_empty() {
        view! {
            <div>
                <p class="text-sm text-slate-600">"Your basket is empty."</p>
                {summary}
            </div>
        }
        .into_any()
    } else {
        view! {
            <div>
                <ul class="space-y-3">
                    {basket
                        .lines
                        .into_iter()
                        .map(|line| view! { <BasketLine line=line cart_items=cart_items/> })
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
) -> impl IntoView {
    view! {
        <aside>
            <div class="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
                <h2 class="mb-4 text-lg font-semibold">{move || format!("Basket ({})", cart_items.get().len())}</h2>
                {let solver_data = solver_data;
                    move || match solve_basket(&solver_data, &cart_items.get()) {
                    Ok(basket) => {
                        solve_time_text.set(basket.solve_duration.clone());
                        view! { <BasketBody basket=basket cart_items=cart_items/> }.into_any()
                    }
                    Err(error_message) => view! {
                        {solve_time_text.set(String::new());}
                        <p class="text-sm text-red-700">{error_message}</p>
                    }
                        .into_any(),
                }}
            </div>
            {move || {
                let value = solve_time_text.get();
                if value.is_empty() {
                    ().into_any()
                } else {
                    view! {
                        <p class="mr-2 mt-1 text-right text-xs text-slate-400">{value}</p>
                    }
                        .into_any()
                }
            }}
        </aside>
    }
}
