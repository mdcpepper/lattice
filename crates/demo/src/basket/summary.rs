use leptos::prelude::*;

use super::PromotionSavings;

pub(super) fn format_application_summary(
    item_applications: usize,
    bundle_applications: usize,
) -> String {
    let item_label = if item_applications == 1 {
        "item"
    } else {
        "items"
    };

    format!("× {bundle_applications} ({item_applications} {item_label})")
}

#[component]
pub(super) fn BasketSummary(
    subtotal: String,
    savings: String,
    savings_breakdown: Vec<PromotionSavings>,
    total: String,
) -> impl IntoView {
    view! {
        <div
            class="basket-summary basket-mobile-dock-hide-target"
            data-dock-hide-key="summary"
        >
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
                                                <span>
                                                    {entry.name} " "
                                                    {format_application_summary(
                                                        entry.item_applications,
                                                        entry.bundle_applications,
                                                    )}
                                                </span>
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
