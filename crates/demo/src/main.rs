//! Leptos Lattice Demo Application

use std::sync::Arc;

use leptos::prelude::*;

mod basket;
mod estimates;
mod products;
mod promotions;

const PRODUCTS_FIXTURE_YAML: &str = include_str!("../../../fixtures/products/demo.yml");
const PROMOTIONS_FIXTURE_YAML: &str = include_str!("../../../fixtures/promotions/demo.yml");
const REPOSITORY_URL: &str = "https://github.com/mdcpepper/lattice";

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
                promotion_meta_map: loaded_promotions.promotion_meta_map,
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
            <RepoCornerLink />
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
            <RepoCornerLink />
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

#[component]
fn RepoCornerLink() -> impl IntoView {
    view! {
        <a
            class="repo-corner-link"
            href=REPOSITORY_URL
            target="_blank"
            rel="noopener noreferrer"
            aria-label="Open the Lattice repository on GitHub"
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
                class="repo-corner-icon lucide lucide-github-icon lucide-github"
                aria-hidden="true"
            >
                <path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"></path>
                <path d="M9 18c-4.51 2-5-2-7-2"></path>
            </svg>
        </a>
    }
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
