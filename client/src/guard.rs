//! Asking before an edit pane is left with changes that were not saved.

// modules {{{
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::ui::Modal;
// }}}

// constants {{{
/// The elements inside a pane that change what is being edited when they are clicked
const CHANGERS: &str = ".segmented button, .hours .cell, .popover li button, .list button, \
                       button.add, .calendar .day, .calendar .foot button";
// }}}

#[derive(Clone, Copy)]
pub struct Guard {
    dirty: RwSignal<bool>,
    /// Whether a pane is open to have changes made in it
    open: RwSignal<bool>,
    /// What to do once the person has said what to do about the changes
    asking: RwSignal<Option<Callback<()>>>,
}

// impl Guard {{{
impl Guard {
    pub fn new() -> Self {
        Self {
            dirty: RwSignal::new(false),
            open: RwSignal::new(false),
            asking: RwSignal::new(None),
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

    pub fn clean(&self) {
        self.dirty.try_set(false);
    }

    /// Notes that an event in the pane may have changed what is being edited
    pub fn touched(&self, ev: &web_sys::Event) {
        if !self.open.get_untracked() {
            return;
        }
        let target = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        let Some(target) = target else {
            return;
        };
        // typing in a picker's search is not a change until something is picked
        if target.matches("input.search").unwrap_or(false) {
            return;
        }
        let changes = match ev.type_().as_str() {
            "click" => target.closest(CHANGERS).ok().flatten().is_some(),
            _ => true,
        };
        if changes {
            self.dirty.set(true);
        }
    }
}
// }}}

// ConfirmLeave {{{
/// The question asked when leaving an edit with changes: save them, drop them or stay
#[component]
pub fn ConfirmLeave(guard: Guard) -> impl IntoView {
    let leave = move |save: bool| {
        let Some(then) = guard.asking.get_untracked() else {
            return;
        };
        guard.asking.set(None);
        if save {
            // the form saves itself and closes the pane, as when its own Save is pressed
            let form = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector(".pane form").ok().flatten())
                .and_then(|f| f.dyn_into::<web_sys::HtmlFormElement>().ok());
            if let Some(form) = form {
                let _ = form.request_submit();
            }
        }
        guard.clean();
        then.run(());
    };
    view! {
        <Show when=move || guard.asking.with(Option::is_some)>
            <Modal title="Unsaved changes" on_close=Callback::new(move |()| guard.asking.set(None))>
                <p>"Do you want to save the changes you made?"</p>
                <div class="actions">
                    <span class="grow"></span>
                    <button type="button" on:click=move |_| guard.asking.set(None)>"Keep editing"</button>
                    <button type="button" class="danger" on:click=move |_| leave(false)>"Discard"</button>
                    <button type="button" class="primary" on:click=move |_| leave(true)>"Save"</button>
                </div>
            </Modal>
        </Show>
    }
}
// }}}
