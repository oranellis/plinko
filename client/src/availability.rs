use leptos::prelude::*;

use crate::nav::Availability as Of;

#[component]
pub fn Availability(of: Of) -> impl IntoView {
    let _ = of;
    view! { <p>"Availability"</p> }
}
