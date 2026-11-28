//! Asks before leaving an edit with unsaved changes.

// modules {{{
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::{
    icons::{Icon, IconKind},
    ui::Modal,
};
// }}}

// constants {{{
/// Clicks inside these are part of the edit
const EDITING: &str = ".contextbar, .infopanel, .workerwin, .modal";
/// Controls that leave the edit alone, like the search or jumping to today
const KEEPS: &str = ".keeps, .helpbutton";
// }}}

#[derive(Clone, Copy)]
pub struct Guard {
    dirty: RwSignal<bool>,
    open: RwSignal<bool>,
    /// Run once the person has answered the question
    asking: RwSignal<Option<Callback<()>>>,
    /// The click that was held back, to replay once answered
    clicked: StoredValue<Option<web_sys::HtmlElement>, LocalStorage>,
}

// impl Guard {{{
impl Guard {
    pub fn new() -> Self {
        Self {
            dirty: RwSignal::new(false),
            open: RwSignal::new(false),
            asking: RwSignal::new(None),
            clicked: StoredValue::new_local(None),
        }
    }

    pub fn then(&self, then: Callback<()>) {
        if self.dirty.get_untracked() {
            self.asking.set(Some(then));
        } else {
            then.run(());
        }
    }

    pub fn opened(&self) {
        self.dirty.set(false);
        self.open.set(true);
    }

    /// Fields can still report a change as they are torn down, which must not count.
    pub fn closed(&self) {
        self.open.try_set(false);
        self.dirty.try_set(false);
    }

    /// The edit was changed from outside the pane
    pub fn dirtied(&self) {
        if self.open.get_untracked() {
            self.dirty.set(true);
        }
    }

    /// While dirty, a click on a link or a button outside the edit asks first and is replayed
    /// once the changes are saved or dropped
    pub fn watch_clicks(self) {
        // capture, so this hears the click before its target does
        crate::ui::window_capture("click", move |ev: web_sys::MouseEvent| {
            if !self.dirty.try_get_untracked().unwrap_or(false) {
                return;
            }
            let target = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            // a link navigates away wherever it is, even inside the edit
            let control = target.and_then(|t| {
                let within = |at: &str| t.closest(at).ok().flatten().is_some();
                let control = t.closest("a[href], button").ok().flatten()?;
                let link = control.tag_name().eq_ignore_ascii_case("a");
                (link || !(within(EDITING) || within(KEEPS))).then_some(control)
            });
            let Some(control) = control.and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
            else {
                return;
            };
            ev.prevent_default();
            ev.stop_propagation();
            self.clicked.set_value(Some(control));
            self.asking.set(Some(Callback::new(move |()| {
                if let Some(control) = self.clicked.try_update_value(Option::take).flatten() {
                    control.click();
                }
            })));
        });
    }

    pub fn clean(&self) {
        self.dirty.try_set(false);
    }
}
// }}}

// Discards {{{
/// Makes escape discard inside the question, so pressing escape twice leaves without saving.
/// It registers after the window, so its escape takes precedence.
#[component]
fn Discards(discard: Callback<()>) -> impl IntoView {
    crate::escape::on_escape(|| true, discard);
}
// }}}

// ConfirmLeave {{{
/// Save, discard or stay. Enter saves, escape discards, a click outside stays.
#[component]
pub fn ConfirmLeave(guard: Guard, on_save: Callback<()>) -> impl IntoView {
    let leave = move |save: bool| {
        let Some(then) = guard.asking.get_untracked() else {
            return;
        };
        guard.asking.set(None);
        if save {
            on_save.run(());
        }
        guard.clean();
        then.run(());
    };
    let stay = Callback::new(move |()| guard.asking.set(None));
    // enter saves, the same key that saves an edit. Captured so the name box underneath does not
    // also save, except on one of the question's buttons, where enter presses that button
    crate::ui::window_capture("keydown", move |ev: web_sys::KeyboardEvent| {
        let asked = guard
            .asking
            .try_with_untracked(Option::is_some)
            .unwrap_or(false);
        let on_button = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|t| t.closest(".modal button").ok().flatten().is_some());
        if asked && ev.key() == "Enter" && !on_button {
            ev.prevent_default();
            ev.stop_propagation();
            leave(true);
        }
    });
    let choice = move |label: &'static str,
                       class: &'static str,
                       kind: IconKind,
                       on: Callback<()>| {
        view! {
            <button type="button" class=format!("ctx {class}") title=label aria-label=label on:click=move |_| on.run(())>
                <Icon kind=kind size=40 />
            </button>
        }
    };
    view! {
        <Show when=move || guard.asking.with(Option::is_some)>
            <Modal title="Unsaved Changes" on_close=stay>
                <Discards discard=Callback::new(move |()| leave(false)) />
                <div class="choices">
                    {choice("Keep editing", "", IconKind::Pencil, stay)}
                    {choice("Save and close", "save", IconKind::Tick, Callback::new(move |()| leave(true)))}
                    {choice("Discard changes", "cancel", IconKind::Close, Callback::new(move |()| leave(false)))}
                </div>
            </Modal>
        </Show>
    }
}
// }}}
