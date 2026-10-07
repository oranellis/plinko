use leptos::prelude::*;
use plinko::types::short_id::ShortId;

#[component]
pub fn PlanSettings(plan: ShortId) -> impl IntoView {
    let _ = plan;
    view! { <p>"Settings"</p> }
}
