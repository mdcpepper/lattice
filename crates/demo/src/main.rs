//! Leptos Lattice Demo Application

use std::sync::Arc;

use leptos::prelude::*;

mod basket;
mod estimates;
mod products;
mod promotions;

const PRODUCTS_FIXTURE_YAML: &str = include_str!("../../../fixtures/products/demo.yml");
const PROMOTIONS_FIXTURE_YAML: &str = include_str!("../../../fixtures/promotions/demo.yml");

/// Parsed application fixtures/state used by the UI.
#[derive(Debug)]
struct AppData {
    /// Products shown on the left panel.
    products: Arc<Vec<products::ProductListItem>>,

    /// Basket/solver data used by the basket panel.
    basket_solver_data: Arc<basket::BasketSolverData>,
}

impl AppData {
    fn load() -> Result<Self, String> {
        let loaded_products = products::load_products(PRODUCTS_FIXTURE_YAML)?;
        let loaded_promotions = promotions::load_promotions(PROMOTIONS_FIXTURE_YAML)?;

        Ok(Self {
            products: Arc::new(loaded_products.products),
            basket_solver_data: Arc::new(basket::BasketSolverData {
                product_meta_map: loaded_products.product_meta_map,
                product_key_by_fixture_key: loaded_products.product_key_by_fixture_key,
                graph: loaded_promotions.graph,
                promotion_names: loaded_promotions.promotion_names,
                currency: loaded_products.currency,
            }),
        })
    }
}

/// Main demo app shell.
#[component]
fn App() -> impl IntoView {
    match AppData::load() {
        Ok(app_data) => render_app(app_data),
        Err(error_message) => render_load_error(error_message),
    }
}

fn render_app(app_data: AppData) -> AnyView {
    let app_data = Arc::new(app_data);
    let cart_items = RwSignal::new(Vec::<String>::new());
    let solve_time_text = RwSignal::new(String::new());
    let live_message = RwSignal::new((0_u64, String::new()));
    let action_message = RwSignal::new(None::<String>);
    let estimate_ui = estimates::install(cart_items);

    view! {
        <main class="app-shell">
            <p class="sr-only" role="status" aria-live="polite" aria-atomic="true">
                {move || live_message.get().1}
            </p>
            <div class="app-header app-frame">
                <h1 class="app-title">"Lattice Demo"</h1>
            </div>
            <div class="app-layout app-frame">
                <products::ProductsPanel
                    products=Arc::clone(&app_data.products)
                    cart_items=cart_items
                    action_message=action_message
                    estimates=estimate_ui.estimates
                    show_spinner=estimate_ui.show_spinner
                />
                <basket::BasketPanel
                    solver_data=Arc::clone(&app_data.basket_solver_data)
                    cart_items=cart_items
                    solve_time_text=solve_time_text
                    live_message=live_message
                    action_message=action_message
                />
            </div>
        </main>
    }
    .into_any()
}

fn render_load_error(error_message: String) -> AnyView {
    view! {
        <main class="app-shell">
            <div class="app-header app-frame">
                <h1 class="app-title">"Lattice Demo"</h1>
            </div>
            <div class="app-error app-frame">
                <p class="error-text">{error_message}</p>
            </div>
        </main>
    }
    .into_any()
}

/// Main server function
fn main() {
    console_error_panic_hook::set_once();

    #[cfg(target_arch = "wasm32")]
    if web_sys::window().is_none() {
        return;
    }

    leptos::mount::mount_to_body(App);
}

fn announce(live_message: RwSignal<(u64, String)>, message: String) {
    live_message.update(|(id, text)| {
        *id = id.saturating_add(1);
        *text = message;
    });
}
