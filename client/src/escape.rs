//! The escape key closes whatever was opened last.

// modules {{{
use std::cell::{Cell, RefCell};

use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};
// }}}

// state {{{
thread_local! {
    /// Last on top
    static OPEN: RefCell<Vec<(u32, Callback<()>)>> = const { RefCell::new(Vec::new()) };
    static NEXT: Cell<u32> = const { Cell::new(0) };
}
// }}}

// helpers {{{
/// Call once for the whole app
pub fn listen() {
    let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|ev: web_sys::KeyboardEvent| {
        if ev.key() != "Escape" || ev.default_prevented() {
            return;
        }
        let top = OPEN.with(|open| open.borrow().last().map(|(_, close)| *close));
        if let Some(close) = top {
            ev.prevent_default();
            close.run(());
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
}

/// While `active`, escape runs `close` unless something opened later is on top. `active` is
/// memoised so a change that keeps it true keeps its place in the stack.
pub fn on_escape(active: impl Fn() -> bool + Send + Sync + 'static, close: Callback<()>) {
    let active = Memo::new(move |_| active());
    Effect::new(move |_| {
        if active.get() {
            let id = NEXT.with(|n| {
                n.set(n.get() + 1);
                n.get()
            });
            OPEN.with(|open| open.borrow_mut().push((id, close)));
            on_cleanup(move || OPEN.with(|open| open.borrow_mut().retain(|(i, _)| *i != id)));
        }
    });
}
// }}}
