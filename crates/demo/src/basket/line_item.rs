use std::collections::BTreeSet;

use leptos::prelude::*;

use crate::promotions::PromotionPill;

use super::BasketLineItem;

pub(super) fn start_icon_confirmation(confirmed_icons: RwSignal<BTreeSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.insert(icon_key.to_string());
    });
}

pub(super) fn clear_icon_confirmation(confirmed_icons: RwSignal<BTreeSet<String>>, icon_key: &str) {
    confirmed_icons.update(|states| {
        states.remove(icon_key);
    });
}

pub(super) fn is_icon_confirmed(
    confirmed_icons: RwSignal<BTreeSet<String>>,
    icon_key: &str,
) -> bool {
    confirmed_icons.with(|states| states.contains(icon_key))
}

pub(super) fn remove_line_item(items: &mut Vec<String>, basket_index: usize, fixture_key: &str) {
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

fn render_promotion_pills(promotion_pills: Vec<PromotionPill>) -> AnyView {
    if promotion_pills.is_empty() {
        return view! {
            <div class="basket-line-pills basket-line-pills-empty" aria-hidden="true"></div>
        }
        .into_any();
    }

    view! {
        <div class="basket-line-pills">
            {promotion_pills
                .into_iter()
                .map(|pill| {
                    let pill_text = format!("{} (#{})", pill.label, pill.bundle_id);

                    view! {
                        <span class="basket-line-pill" style=pill.style>
                            {pill_text}
                        </span>
                    }
                })
                .collect_view()}
        </div>
    }
    .into_any()
}

#[component]
fn RemoveLineButton(
    basket_index: usize,
    remove_fixture_key: String,
    item_name_for_remove: String,
    final_price: String,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
) -> impl IntoView {
    let remove_button_label = format!(
        "Remove {} ({}) from basket",
        item_name_for_remove.clone(),
        final_price
    );

    view! {
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
    }
}

#[component]
fn AddLineButton(
    fixture_key: String,
    item_name_for_add: String,
    final_price: String,
    add_icon_key: String,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    let add_icon_key_for_class = add_icon_key.clone();
    let add_icon_key_for_click = add_icon_key.clone();
    let add_icon_key_for_animation_end = add_icon_key;

    let add_button_label = format!(
        "Add another {} ({}) to basket",
        item_name_for_add.clone(),
        final_price
    );

    view! {
        <button
            type="button"
            aria-label=add_button_label
            class=move || {
                if is_icon_confirmed(add_icon_confirmations, &add_icon_key_for_class) {
                    "icon-button icon-button-primary icon-button-compact icon-button-confirmed"
                } else {
                    "icon-button icon-button-primary icon-button-compact"
                }
            }
            on:click=move |_| {
                start_icon_confirmation(add_icon_confirmations, &add_icon_key_for_click);

                cart_items.update(|items| items.push(fixture_key.clone()));

                action_message.set(Some(format!("Added {item_name_for_add} to basket.")));
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
    }
}

#[component]
pub(super) fn BasketLine(
    line: BasketLineItem,
    cart_items: RwSignal<Vec<String>>,
    action_message: RwSignal<Option<String>>,
    add_icon_confirmations: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    let basket_index = line.basket_index;
    let fixture_key = line.fixture_key.clone();
    let remove_fixture_key = fixture_key.clone();
    let line_key = format!("{basket_index}:{}", line.fixture_key);

    let item_name_for_add = line.name.clone();
    let item_name_for_remove = line.name.clone();

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
                                    <span class="basket-line-base-price">{line.base_price.clone()}</span>
                                }
                                .into_any()
                            } else {
                                ().into_any()
                            }}
                            <span class="basket-line-final-price">{line.final_price.clone()}</span>
                        </div>
                    </div>
                    {render_promotion_pills(promotion_pills)}
                </div>

                <div>
                    <RemoveLineButton
                        basket_index=basket_index
                        remove_fixture_key=remove_fixture_key
                        item_name_for_remove=item_name_for_remove
                        final_price=line.final_price.clone()
                        cart_items=cart_items
                        action_message=action_message
                    />
                    <AddLineButton
                        fixture_key=fixture_key
                        item_name_for_add=item_name_for_add
                        final_price=line.final_price
                        add_icon_key=format!("add:{line_key}")
                        cart_items=cart_items
                        action_message=action_message
                        add_icon_confirmations=add_icon_confirmations
                    />
                </div>
            </div>
        </li>
    }
}
