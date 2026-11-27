//! The help window from the top right of every page: what each page does with the keyboard, a
//! page of it each, listed down the left.

// modules {{{
use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    icons::{Icon, IconKind},
    ui::Modal,
};
// }}}

// constants {{{
/// Keys, and what they do
type Keys = &'static [(&'static [&'static str], &'static str)];
/// A mode with its keys, a heading with its modes, and a page with its headings
type Mode = (&'static str, Keys);
type Heading = (&'static str, &'static [Mode]);
type Page = (&'static str, &'static [Heading]);

/// The pages of the help: each has headings, and under those what the keys do in each mode
const PAGES: &[Page] = &[(
    "Plan view",
    &[(
        "Gantt",
        &[
            (
                "Anywhere on the chart",
                &[
                    (
                        &["a"],
                        "Add an item: then t task, m milestone, g group, b buffer, d delay",
                    ),
                    (&["/"], "Search the plan"),
                    (&["t"], "Jump to today"),
                    (
                        &["shift"],
                        "Hold to show the handles of the item the pointer is near",
                    ),
                    (&["?"], "This help"),
                ],
            ),
            (
                "With an item selected",
                &[
                    (&["e"], "Edit it"),
                    (
                        &["s"],
                        "List its statuses (a task), or pin or unpin its start (a buffer)",
                    ),
                    (&["d"], "Pick what it waits for, with clicks or the search"),
                    (&["f"], "Pick what waits for it"),
                    (
                        &["g"],
                        "Pick its group: click a group, or nothing for the top level",
                    ),
                    (&["c"], "Edit it and take hold of the day it is held to"),
                    (&["m"], "Move its days, then s its start, e its end"),
                    (&["delete"], "Delete it, after asking, as backspace does"),
                    (&["escape"], "End a pick"),
                ],
            ),
            (
                "Creating or editing",
                &[
                    (&["n"], "Type in its name, at the end"),
                    (&["tab"], "Leave the name for these keys"),
                    (&["l"], "Type its days"),
                    (&["m"], "Move its days, then s its start, e its end"),
                    (&["w"], "Put the keys on its workers (a task)"),
                    (
                        &["d", "f", "g"],
                        "Pick what it waits for, what waits for it, or its group",
                    ),
                    (
                        &["c"],
                        "Take hold of its day, holding it first; again goes round start, end and none",
                    ),
                    (
                        &["s"],
                        "List its statuses (a task), or pin or unpin its start (a buffer)",
                    ),
                    (&["enter"], "Save it"),
                    (
                        &["escape"],
                        "End a pick, then leave it, asking first when there are changes",
                    ),
                    (&["delete"], "Delete it, after asking, as backspace does"),
                ],
            ),
            (
                "With its day taken hold of",
                &[
                    (
                        &["←", "→"],
                        "Move the day a day either way, as h and l do, or move the pointer",
                    ),
                    (&["enter"], "Put it there, as a click on a day does"),
                    (&["e", "f", "d"], "Hold it no earlier, exactly, or no later (a deadline)"),
                    (&["c"], "Go round holding its start, its end, or nothing"),
                    (&["escape"], "Leave it where it was"),
                ],
            ),
            (
                "Moving its days",
                &[
                    (&["←", "→"], "A day either way, as h and l do"),
                    (
                        &["s", "e"],
                        "Its start, or its end, which for one not started is how long it lasts",
                    ),
                    (&["m"], "All of its days, for a task that has started"),
                    (&["enter"], "Stop, as escape does"),
                ],
            ),
            (
                "With the statuses listed",
                &[
                    (&["s"], "The next status, as the button shows"),
                    (
                        &["n", "i", "o", "c", "d"],
                        "Not started, in progress, on hold, complete, dropped",
                    ),
                    (&["escape"], "Close them"),
                ],
            ),
            (
                "On the workers",
                &[
                    (&["↑", "↓"], "Move along them, as k and j do"),
                    (&["←", "→"], "Fewer or more hours a day, as h and l do"),
                    (&["a"], "Add one, its list open to search"),
                    (&["c"], "Change who the marked one is, from its list"),
                    (&["p"], "Add a placeholder"),
                    (&["delete"], "Take the marked one away, as backspace does"),
                    (&["w"], "Leave them, as escape does"),
                ],
            ),
            (
                "In the search",
                &[
                    (&["↑", "↓"], "Move down the list"),
                    (
                        &["enter"],
                        "Select it and show it, or while picking, pick it",
                    ),
                    (&["escape"], "Close it"),
                ],
            ),
            (
                "In a question",
                &[
                    (&["enter"], "Yes: save, or delete"),
                    (
                        &["escape"],
                        "Unsaved changes: discard them. Deleting: keep it",
                    ),
                ],
            ),
        ],
    )],
)];
// }}}

// HelpButton {{{
/// The question mark at the top right, and the window it opens, which `?` opens too from
/// anywhere that is not being typed into
#[component]
pub fn HelpButton() -> impl IntoView {
    let open = RwSignal::new(false);
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            let target = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            let typing = target.is_some_and(|t| {
                t.closest("input, textarea, select")
                    .ok()
                    .flatten()
                    .is_some()
            });
            let window_open = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector(".modal").ok().flatten())
                .is_some();
            if ev.key() == "?" && !typing && !window_open && !ev.default_prevented() {
                ev.prevent_default();
                open.try_set(true);
            }
        });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
    view! {
        <button
            type="button"
            class="plain helpbutton"
            title="Help"
            aria-label="Help"
            on:click=move |_| open.set(true)
        >
            <Icon kind=IconKind::Help size=24 />
        </button>
        <Show when=move || open.get()>
            <HelpWindow on_close=Callback::new(move |()| open.set(false)) />
        </Show>
    }
}
// }}}

// HelpWindow {{{
#[component]
fn HelpWindow(on_close: Callback<()>) -> impl IntoView {
    let page = RwSignal::new(0usize);
    view! {
        <Modal title="Help" on_close=on_close wide=true>
            <div class="help">
                <nav class="helptabs">
                    {PAGES.iter().enumerate().map(|(i, (title, _))| view! {
                        <button type="button" class:active=move || page.get() == i on:click=move |_| page.set(i)>
                            {*title}
                        </button>
                    }).collect_view()}
                </nav>
                <div class="helppage">
                    {move || PAGES[page.get()].1.iter().map(|(heading, modes)| view! {
                        <h3>{*heading}</h3>
                        {modes.iter().map(|(mode, keys)| view! {
                            <h4>{*mode}</h4>
                            <table class="keys">
                                {keys.iter().map(|(pressed, does)| view! {
                                    <tr>
                                        <td>{pressed.iter().map(|k| view! { <kbd>{*k}</kbd> }).collect_view()}</td>
                                        <td>{*does}</td>
                                    </tr>
                                }).collect_view()}
                            </table>
                        }).collect_view()}
                    }).collect_view()}
                </div>
            </div>
        </Modal>
    }
}
// }}}
