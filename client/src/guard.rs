//! Asking before what is being edited is left with changes that were not saved.

// modules {{{
use leptos::prelude::*;

use crate::ui::Modal;
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

    /// What is being edited was changed from outside the pane
    pub fn dirtied(&self) {
        if self.open.get_untracked() {
            self.dirty.set(true);
        }
    }

    pub fn clean(&self) {
        self.dirty.try_set(false);
    }
}
// }}}

// ConfirmLeave {{{
/// The question asked when leaving an edit with changes: save them, drop them or stay
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
