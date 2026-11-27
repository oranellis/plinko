//! Asking before what is being edited is left with changes that were not saved.

// modules {{{
use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    icons::{Icon, IconKind},
    ui::Modal,
};
// }}}

// constants {{{
/// Where what is being edited is, and the question about it: clicks in there are part of the edit
const EDITING: &str = ".contextbar, .infopanel, .workerwin, .modal";
// }}}

#[derive(Clone, Copy)]
pub struct Guard {
    dirty: RwSignal<bool>,
    /// Whether a pane is open to have changes made in it
    open: RwSignal<bool>,
    /// What to do once the person has said what to do about the changes
    asking: RwSignal<Option<Callback<()>>>,
    /// What was clicked when they were asked, to click again once they have said
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

    /// Runs `then` now, or once the person has chosen when there are unsaved changes
    pub fn then(&self, then: Callback<()>) {
        if self.dirty.get_untracked() {
            self.asking.set(Some(then));
        } else {
            then.run(());
        }
    }

    /// A pane has opened, with nothing changed in it yet
    pub fn opened(&self) {
        self.dirty.set(false);
        self.open.set(true);
    }

    /// The pane has gone. Its fields can still report a last change as they are taken away, which
    /// is not a change to anything.
    pub fn closed(&self) {
        self.open.try_set(false);
        self.dirty.try_set(false);
    }

    /// What is being edited was changed from outside the pane
    pub fn dirtied(&self) {
        if self.open.get_untracked() {
            self.dirty.set(true);
        }
    }

    /// From here on a click on a link or a button outside what is being edited asks first when
    /// there are unsaved changes, and is made again once they are saved or dropped
    pub fn watch_clicks(self) {
        let on_click =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                if !self.dirty.try_get_untracked().unwrap_or(false) {
                    return;
                }
                let target = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                let control = target.and_then(|t| {
                    let editing = t.closest(EDITING).ok().flatten().is_some();
                    t.closest("a, button").ok().flatten().filter(|_| !editing)
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
        // before anything the click is on hears of it
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let heard = on_click.as_ref().unchecked_ref();
            let _ = document.add_event_listener_with_callback_and_bool("click", heard, true);
        }
        on_click.forget();
    }

    pub fn clean(&self) {
        self.dirty.try_set(false);
    }
}
// }}}

// Discards {{{
/// Inside the question, makes escape the answer to discard: it is what asked, so twice is to
/// leave without saving. It is asked for after the window is, whose own escape it is then over.
#[component]
fn Discards(discard: Callback<()>) -> impl IntoView {
    crate::escape::on_escape(|| true, discard);
}
// }}}

// ConfirmLeave {{{
/// The question asked when leaving an edit with changes: save them, drop them or stay. Enter
/// saves and escape drops them; a click off it is to stay.
#[component]
pub fn ConfirmLeave(guard: Guard, on_save: Callback<()>) -> impl IntoView {
    let leave = move |save: bool| {
        let Some(then) = guard.asking.get_untracked() else {
            return;
        };
        guard.asking.set(None);
        // saving also leaves what was being edited, before going where `then` goes
        if save {
            on_save.run(());
        }
        guard.clean();
        then.run(());
    };
    let stay = Callback::new(move |()| guard.asking.set(None));
    // enter saves, so that it is the same key as saves an edit
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            let asked = guard
                .asking
                .try_with_untracked(Option::is_some)
                .unwrap_or(false);
            if asked && ev.key() == "Enter" {
                ev.prevent_default();
                leave(true);
            }
        });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
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
            // a click off it is to go on editing
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
