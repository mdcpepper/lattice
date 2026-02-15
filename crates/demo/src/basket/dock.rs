#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use rustc_hash::FxHashMap;

#[component]
pub(super) fn BasketMobileDock(
    item_count: usize,
    basket_total: Option<String>,
    savings: Option<String>,
    hidden: RwSignal<bool>,
) -> impl IntoView {
    let total_text = basket_total.unwrap_or_else(|| "N/A".to_string());

    let (meta_label, meta_value) = if let Some(value) = savings {
        if item_count == 0 {
            ("Your basket is empty.".to_string(), None)
        } else {
            ("Savings".to_string(), Some(value))
        }
    } else {
        ("Basket unavailable".to_string(), None)
    };

    let meta_content = if let Some(value) = meta_value {
        view! {
            <>
                <span class="basket-mobile-dock-meta-label sr-only">{meta_label}</span>
                <span class="basket-mobile-dock-meta-value">{value}</span>
            </>
        }
        .into_any()
    } else {
        view! { <span class="basket-mobile-dock-meta-label">{meta_label}</span> }.into_any()
    };

    view! {
        <a
            class="basket-mobile-dock"
            class:basket-mobile-dock-hidden=move || hidden.get()
            href="#basket-panel"
            aria-label="Jump to basket panel"
        >
            <div class="basket-mobile-dock-label basket-title-label">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="16"
                    height="16"
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
            </div>
            <p class="basket-mobile-dock-total">{total_text}</p>
            <p class="basket-mobile-dock-meta">{meta_content}</p>
        </a>
    }
}

#[cfg(target_arch = "wasm32")]
type DockIntersectionObserver = (
    web_sys::IntersectionObserver,
    wasm_bindgen::closure::Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
);

#[cfg(target_arch = "wasm32")]
pub(super) fn install_mobile_dock_observer(
    cart_items: RwSignal<Vec<String>>,
    dock_hidden: RwSignal<bool>,
) {
    use wasm_bindgen::{JsCast, closure::Closure};

    let dock_observer = Rc::new(RefCell::new(None::<DockIntersectionObserver>));

    Effect::new({
        let dock_observer = Rc::clone(&dock_observer);

        move |_| {
            let _ = cart_items.get();

            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };

            let Ok(targets) = document.query_selector_all(".basket-mobile-dock-hide-target") else {
                return;
            };

            if targets.length() == 0 {
                dock_hidden.set(false);

                if let Some((observer, _callback)) = dock_observer.borrow_mut().take() {
                    observer.disconnect();
                }

                return;
            }

            if let Some((observer, _callback)) = dock_observer.borrow_mut().take() {
                observer.disconnect();
            }

            let mut visibility_by_target = FxHashMap::<String, bool>::default();

            for index in 0..targets.length() {
                let Some(node) = targets.item(index) else {
                    continue;
                };

                let Ok(element) = node.dyn_into::<web_sys::Element>() else {
                    continue;
                };

                let key = element
                    .get_attribute("data-dock-hide-key")
                    .unwrap_or_else(|| index.to_string());

                visibility_by_target.entry(key).or_insert(false);
            }

            let observer_callback = Closure::<
                dyn FnMut(js_sys::Array, web_sys::IntersectionObserver),
            >::new(
                move |entries: js_sys::Array, _observer: web_sys::IntersectionObserver| {
                    for index in 0..entries.length() {
                        let raw_entry = entries.get(index);

                        let Ok(entry) = raw_entry.dyn_into::<web_sys::IntersectionObserverEntry>()
                        else {
                            continue;
                        };

                        let target = entry.target();

                        let key = target
                            .get_attribute("data-dock-hide-key")
                            .unwrap_or_else(|| index.to_string());

                        visibility_by_target.insert(key, entry.is_intersecting());
                    }

                    dock_hidden.set(visibility_by_target.values().any(|state| *state));
                },
            );

            let options = web_sys::IntersectionObserverInit::new();
            options.set_threshold(&wasm_bindgen::JsValue::from_f64(0.05));

            let Ok(observer) = web_sys::IntersectionObserver::new_with_options(
                observer_callback.as_ref().unchecked_ref(),
                &options,
            ) else {
                return;
            };

            for index in 0..targets.length() {
                let Some(node) = targets.item(index) else {
                    continue;
                };

                let Ok(element) = node.dyn_into::<web_sys::Element>() else {
                    continue;
                };

                observer.observe(&element);
            }

            dock_observer.replace(Some((observer, observer_callback)));
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn install_mobile_dock_observer(
    _cart_items: RwSignal<Vec<String>>,
    _dock_hidden: RwSignal<bool>,
) {
}
