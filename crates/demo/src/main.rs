//! Leptos Lattice Demo Application

use std::sync::Arc;

use leptos::prelude::*;

mod basket;
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
        Ok(app_data) => {
            let app_data = Arc::new(app_data);
            let cart_items = RwSignal::new(Vec::<String>::new());
            let solve_time_text = RwSignal::new(String::new());

            view! {
                <main class="min-h-screen bg-slate-50 px-4 py-6 text-slate-900">
                    <div class="mx-auto mb-6 max-w-5xl">
                        <h1 class="text-2xl font-semibold tracking-tight">"Lattice Demo"</h1>
                    </div>
                    <div class="mx-auto grid max-w-5xl grid-cols-1 gap-6 md:grid-cols-2">
                        <products::ProductsPanel
                            products=Arc::clone(&app_data.products)
                            cart_items=cart_items
                        />
                        <basket::BasketPanel
                            solver_data=Arc::clone(&app_data.basket_solver_data)
                            cart_items=cart_items
                            solve_time_text=solve_time_text
                        />
                    </div>
                </main>
            }
            .into_any()
        }
        Err(error_message) => view! {
            <main class="min-h-screen bg-slate-50 px-4 py-6 text-slate-900">
                <div class="mx-auto mb-6 max-w-5xl">
                    <h1 class="text-2xl font-semibold tracking-tight">"Lattice Demo"</h1>
                </div>
                <div class="mx-auto max-w-3xl rounded-lg border border-red-200 bg-red-50 p-4">
                    <p class="text-sm text-red-700">{error_message}</p>
                </div>
            </main>
        }
        .into_any(),
    }
}

/// Main server function
fn main() {
    console_error_panic_hook::set_once();

    leptos::mount::mount_to_body(App);
}
