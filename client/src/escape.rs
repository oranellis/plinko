//! The escape key, which closes whatever was opened last: a popover, a window or the edit pane.

// modules {{{
use std::cell::{Cell, RefCell};

use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};
// }}}

// items {{{
thread_local! {
    /// What is open, the last on top, each with what escape does to it
    static OPEN: RefCell<Vec<(u32, Callback<()>)>> = const { RefCell::new(Vec::new()) };
    static NEXT: Cell<u32> = const { Cell::new(0) };
}
// }}}

// fn listen {{{
/// Starts listening to the keyboard, once for the whole app
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
// }}}

// fn on_escape {{{
/// While `active`, escape runs `close` unless something opened later is on top of it
pub fn on_escape(active: impl Fn() -> bool + 'static, close: Callback<()>) {
    Effect::new(move |_| {
        if active() {
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
