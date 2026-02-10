use std::collections::BTreeMap;

#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

use leptos::{prelude::*, task};

#[cfg(target_arch = "wasm32")]
use leptos_workers::worker;

#[cfg(target_arch = "wasm32")]
use crate::basket;

use crate::products::ProductEstimate;

#[cfg(target_arch = "wasm32")]
const PRODUCTS_FIXTURE_YAML: &str = include_str!("../../../fixtures/products/demo.yml");

#[cfg(target_arch = "wasm32")]
const PROMOTIONS_FIXTURE_YAML: &str = include_str!("../../../fixtures/promotions/demo.yml");

const ESTIMATE_DEBOUNCE_MS: i32 = 50;

#[cfg(target_arch = "wasm32")]
const SPINNER_DELAY_MS: i32 = 100;

#[cfg(target_arch = "wasm32")]
const WORKER_ERROR_PREFIX: &str = "ERR:\t";

/// UI-facing reactive state for product impact estimates.
#[derive(Debug, Clone, Copy)]
pub struct EstimateUiSignals {
    /// Map of product fixture key to current estimate.
    pub estimates: RwSignal<BTreeMap<String, ProductEstimate>>,

    /// Whether the product panel should show its estimating spinner.
    pub show_spinner: RwSignal<bool>,
}

#[derive(Debug, Clone, Copy)]
struct EstimateSignals {
    estimates: RwSignal<BTreeMap<String, ProductEstimate>>,
    estimating: RwSignal<bool>,
    show_spinner: RwSignal<bool>,
    generation: RwSignal<u64>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
struct WorkerProductMeta {
    fixture_key: String,
    price_minor: i64,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
struct WorkerData {
    solver_data: basket::BasketSolverData,
    products: Vec<WorkerProductMeta>,
}

#[cfg(target_arch = "wasm32")]
static WORKER_DATA: OnceLock<Result<WorkerData, String>> = OnceLock::new();

/// Install estimation effects tied to cart changes and return UI-facing signals.
pub fn install(cart_items: RwSignal<Vec<String>>) -> EstimateUiSignals {
    let signals = EstimateSignals {
        estimates: RwSignal::new(BTreeMap::new()),
        estimating: RwSignal::new(false),
        show_spinner: RwSignal::new(false),
        generation: RwSignal::new(0_u64),
    };

    Effect::new(move |_| {
        let cart_snapshot = cart_items.get();

        signals.generation.update(|generation| {
            *generation = generation.saturating_add(1);
        });

        signals.show_spinner.set(false);

        let run_id = signals.generation.get_untracked();

        task::spawn_local(async move {
            wait_for_timeout(ESTIMATE_DEBOUNCE_MS).await;

            if !is_current(run_id, signals) {
                return;
            }

            begin_estimation(run_id, signals);

            if let Some(worker_payload) = run_worker_estimate(&cart_snapshot).await {
                if !is_current(run_id, signals) {
                    return;
                }

                signals.estimates.set(parse_worker_result(&worker_payload));
            }

            if is_current(run_id, signals) {
                finish_estimation(signals);
            }
        });
    });

    EstimateUiSignals {
        estimates: signals.estimates,
        show_spinner: signals.show_spinner,
    }
}

fn is_current(run_id: u64, signals: EstimateSignals) -> bool {
    signals.generation.get_untracked() == run_id
}

fn begin_estimation(run_id: u64, signals: EstimateSignals) {
    signals.estimating.set(true);
    spawn_spinner_reveal(run_id, signals);
}

#[cfg(target_arch = "wasm32")]
fn spawn_spinner_reveal(run_id: u64, signals: EstimateSignals) {
    task::spawn_local(async move {
        wait_for_timeout(SPINNER_DELAY_MS).await;

        if is_current(run_id, signals) && signals.estimating.get_untracked() {
            signals.show_spinner.set(true);
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_spinner_reveal(run_id: u64, signals: EstimateSignals) {
    if is_current(run_id, signals) && signals.estimating.get_untracked() {
        signals.show_spinner.set(true);
    }
}

fn finish_estimation(signals: EstimateSignals) {
    signals.estimating.set(false);
    signals.show_spinner.set(false);
}

fn parse_worker_result(result_text: &str) -> BTreeMap<String, ProductEstimate> {
    let mut map = BTreeMap::<String, ProductEstimate>::new();

    for line in result_text.lines() {
        let mut parts = line.splitn(3, '\t');

        let Some(fixture_key) = parts.next() else {
            continue;
        };

        let Some(marginal_raw) = parts.next() else {
            continue;
        };

        let Some(savings_raw) = parts.next() else {
            continue;
        };

        let Ok(marginal_minor) = marginal_raw.parse::<i64>() else {
            continue;
        };

        let Ok(savings_minor) = savings_raw.parse::<i64>() else {
            continue;
        };

        map.insert(
            fixture_key.to_string(),
            ProductEstimate {
                marginal_minor,
                savings_minor,
            },
        );
    }

    map
}

#[cfg(target_arch = "wasm32")]
fn worker_data() -> Result<&'static WorkerData, String> {
    WORKER_DATA
        .get_or_init(load_worker_data)
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(target_arch = "wasm32")]
fn load_worker_data() -> Result<WorkerData, String> {
    let loaded_products = crate::products::load_products(PRODUCTS_FIXTURE_YAML)?;
    let loaded_promotions = crate::promotions::load_promotions(PROMOTIONS_FIXTURE_YAML)?;

    let worker_products = loaded_products
        .products
        .iter()
        .map(|product| WorkerProductMeta {
            fixture_key: product.fixture_key.clone(),
            price_minor: product.price_minor,
        })
        .collect();

    Ok(WorkerData {
        solver_data: basket::BasketSolverData {
            product_meta_map: loaded_products.product_meta_map,
            product_key_by_fixture_key: loaded_products.product_key_by_fixture_key,
            graph: loaded_promotions.graph,
            promotion_names: loaded_promotions.promotion_names,
            promotion_meta_map: loaded_promotions.promotion_meta_map,
            currency: loaded_products.currency,
        },
        products: worker_products,
    })
}

#[cfg(target_arch = "wasm32")]
fn decode_cart_keys(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        Vec::new()
    } else {
        raw.split('\n').map(str::to_owned).collect()
    }
}

#[cfg(target_arch = "wasm32")]
/// Worker entrypoint that computes marginal basket-impact estimates for all products.
#[worker(EstimateCartWorker)]
async fn estimate_cart_worker(cart_keys: String) -> String {
    let Ok(worker_data) = worker_data() else {
        return format!("{WORKER_ERROR_PREFIX}Failed to load worker fixtures");
    };

    let cart_snapshot = decode_cart_keys(&cart_keys);

    let Ok(base_total_minor) = basket::solve_total_minor(&worker_data.solver_data, &cart_snapshot)
    else {
        return format!("{WORKER_ERROR_PREFIX}Failed to solve base basket");
    };

    let mut lines: Vec<String> = Vec::with_capacity(worker_data.products.len());

    for product in &worker_data.products {
        let mut projected_cart = cart_snapshot.clone();
        projected_cart.push(product.fixture_key.clone());

        let Ok(projected_total_minor) =
            basket::solve_total_minor(&worker_data.solver_data, &projected_cart)
        else {
            continue;
        };

        let marginal_minor = projected_total_minor - base_total_minor;
        let savings_minor = product.price_minor - marginal_minor;

        lines.push(format!(
            "{}\t{}\t{}",
            product.fixture_key, marginal_minor, savings_minor
        ));
    }

    lines.join("\n")
}

#[cfg(target_arch = "wasm32")]
async fn run_worker_estimate(cart_snapshot: &[String]) -> Option<String> {
    let response = estimate_cart_worker(cart_snapshot.join("\n")).await.ok()?;

    (!response.starts_with(WORKER_ERROR_PREFIX)).then_some(response)
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_worker_estimate(_cart_snapshot: &[String]) -> Option<String> {
    task::tick().await;

    None
}

#[cfg(target_arch = "wasm32")]
async fn wait_for_timeout(delay_ms: i32) {
    use js_sys::{Function, Promise};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::JsFuture;

    let mut executor = move |resolve: Function, _reject: Function| {
        let Some(window) = web_sys::window() else {
            let _ = resolve.call0(&JsValue::NULL);
            return;
        };

        let callback = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });

        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            delay_ms,
        );
    };

    let promise = Promise::new(&mut executor);
    let _ = JsFuture::from(promise).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn wait_for_timeout(_delay_ms: i32) {
    task::tick().await;
}

#[cfg(test)]
mod tests {
    use leptos::prelude::*;

    use super::*;

    #[test]
    fn test_parse_worker_result_empty_string() {
        let result = parse_worker_result("");

        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worker_result_single_line() {
        let input = "product1\t100\t50";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("product1").copied(),
            Some(ProductEstimate {
                marginal_minor: 100,
                savings_minor: 50
            })
        );
    }

    #[test]
    fn test_parse_worker_result_multiple_lines() {
        let input = "product1\t100\t50\nproduct2\t200\t75\nproduct3\t150\t25";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 3);

        assert_eq!(
            result.get("product1").copied(),
            Some(ProductEstimate {
                marginal_minor: 100,
                savings_minor: 50
            })
        );
        assert_eq!(
            result.get("product2").copied(),
            Some(ProductEstimate {
                marginal_minor: 200,
                savings_minor: 75
            })
        );
        assert_eq!(
            result.get("product3").copied(),
            Some(ProductEstimate {
                marginal_minor: 150,
                savings_minor: 25
            })
        );
    }

    #[test]
    fn test_parse_worker_result_negative_values() {
        let input = "product1\t-50\t150";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("product1").copied(),
            Some(ProductEstimate {
                marginal_minor: -50,
                savings_minor: 150
            })
        );
    }

    #[test]
    fn test_parse_worker_result_zero_values() {
        let input = "product1\t0\t0";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("product1").copied(),
            Some(ProductEstimate {
                marginal_minor: 0,
                savings_minor: 0
            })
        );
    }

    #[test]
    fn test_parse_worker_result_malformed_line_missing_field() {
        let input = "product1\t100";

        let result = parse_worker_result(input);

        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worker_result_malformed_line_invalid_number() {
        let input = "product1\tabc\t50";

        let result = parse_worker_result(input);

        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worker_result_mixed_valid_invalid() {
        let input = "product1\t100\t50\nproduct2\tabc\t75\nproduct3\t150\t25";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("product1"));
        assert!(!result.contains_key("product2"));
        assert!(result.contains_key("product3"));
    }

    #[test]
    fn test_parse_worker_result_empty_lines() {
        let input = "product1\t100\t50\n\nproduct2\t200\t75";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_worker_result_trailing_newline() {
        let input = "product1\t100\t50\n";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_worker_result_product_key_with_special_chars() {
        let input = "product-1_test\t100\t50";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("product-1_test"));
    }

    #[test]
    fn test_parse_worker_result_large_numbers() {
        let input = "product1\t999999999\t888888888";

        let result = parse_worker_result(input);

        assert_eq!(result.len(), 1);

        assert_eq!(
            result.get("product1").copied(),
            Some(ProductEstimate {
                marginal_minor: 999_999_999,
                savings_minor: 888_888_888
            })
        );
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_decode_cart_keys_empty() {
        let result = decode_cart_keys("");

        assert!(result.is_empty());
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_decode_cart_keys_single_item() {
        let result = decode_cart_keys("item1");

        assert_eq!(result, vec!["item1".to_string()]);
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_decode_cart_keys_multiple_items() {
        let result = decode_cart_keys("item1\nitem2\nitem3");

        assert_eq!(
            result,
            vec![
                "item1".to_string(),
                "item2".to_string(),
                "item3".to_string()
            ]
        );
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_decode_cart_keys_trailing_newline() {
        let result = decode_cart_keys("item1\nitem2\n");

        assert_eq!(
            result,
            vec!["item1".to_string(), "item2".to_string(), String::new()]
        );
    }

    // Test signal-based helper functions
    #[test]
    fn test_is_current_returns_true_for_matching_id() {
        let signals = EstimateSignals {
            estimates: RwSignal::new(BTreeMap::new()),
            estimating: RwSignal::new(false),
            show_spinner: RwSignal::new(false),
            generation: RwSignal::new(42),
        };

        assert!(is_current(42, signals));
    }

    #[test]
    fn test_is_current_returns_false_for_different_id() {
        let signals = EstimateSignals {
            estimates: RwSignal::new(BTreeMap::new()),
            estimating: RwSignal::new(false),
            show_spinner: RwSignal::new(false),
            generation: RwSignal::new(42),
        };

        assert!(!is_current(41, signals));
    }

    #[test]
    fn test_begin_estimation_sets_estimating() {
        let signals = EstimateSignals {
            estimates: RwSignal::new(BTreeMap::new()),
            estimating: RwSignal::new(false),
            show_spinner: RwSignal::new(false),
            generation: RwSignal::new(1),
        };

        begin_estimation(1, signals);

        assert!(signals.estimating.get_untracked());
    }

    #[test]
    fn test_finish_estimation_clears_state() {
        let signals = EstimateSignals {
            estimates: RwSignal::new(BTreeMap::new()),
            estimating: RwSignal::new(true),
            show_spinner: RwSignal::new(true),
            generation: RwSignal::new(1),
        };

        finish_estimation(signals);

        assert!(!signals.estimating.get_untracked());
        assert!(!signals.show_spinner.get_untracked());
    }
}
