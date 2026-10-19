//! A date picker: a button that opens a calendar, with the date written in the chosen format.

use chrono::{Datelike, NaiveDate};
use leptos::{html, prelude::*};
use wasm_bindgen::JsValue;

use crate::{
    format::{self, WeekStart, use_date_format, use_preferences},
    ui::use_sync,
};

/// The first day of the month `months` away from `day`'s
fn month_shifted(day: NaiveDate, months: i32) -> NaiveDate {
    let index = day.year() * 12 + day.month0() as i32 + months;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1).unwrap_or(day)
}

/// The days of the month `first` starts, a week to a row, with blanks to line them up
fn month_cells(first: NaiveDate, week: WeekStart) -> Vec<Option<NaiveDate>> {
    let blanks = week.offset(first);
    let days = month_shifted(first, 1)
        .pred_opt()
        .map_or(28, |last| last.day());
    let mut cells = vec![None; blanks];
    cells.extend((1..=days).filter_map(|d| first.with_day(d)).map(Some));
    cells
}

/// A date that is picked from a calendar. With `clearable` it can be taken away again.
#[component]
pub fn DatePicker(
    #[prop(into)] value: Signal<Option<NaiveDate>>,
    on_change: Callback<Option<NaiveDate>>,
    #[prop(optional)] clearable: bool,
    #[prop(optional)] disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let sync = use_sync();
    let layout = use_date_format();
    let week = use_preferences().week_start;
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let open = RwSignal::new(false);
    crate::escape::on_escape(move || open.get(), Callback::new(move |()| open.set(false)));
    let shown = RwSignal::new(today);
    let anchor = RwSignal::new((0.0, 0.0));
    let button = NodeRef::<html::Button>::new();
    let show = move |_| {
        if let Some(b) = button.get_untracked() {
            let r = b.get_bounding_client_rect();
            let window = web_sys::window();
            let inner =
                |size: Option<Result<JsValue, JsValue>>| size.and_then(|s| s.ok()?.as_f64());
            let width = inner(window.as_ref().map(|w| w.inner_width())).unwrap_or(1280.0);
            let height = inner(window.as_ref().map(|w| w.inner_height())).unwrap_or(800.0);
            let x = r.left().min(width - 272.0).max(4.0);
            anchor.set((x, (r.bottom() + 2.0).min(height - 320.0).max(4.0)));
        }
        shown.set(month_shifted(value.get_untracked().unwrap_or(today), 0));
        open.set(true);
    };
    let pick = move |day: Option<NaiveDate>| {
        open.set(false);
        on_change.run(day);
    };
    // the popover can sit inside a label, which would press the button again
    let inert = |ev: leptos::ev::MouseEvent| ev.prevent_default();
    let step = move |months: i32| move |_| shown.update(|d| *d = month_shifted(*d, months));
    view! {
        <button
            type="button"
            class="picker"
            node_ref=button
            disabled=move || disabled.is_some_and(|d| d.get())
            on:click=show
        >
            <span class:muted=move || value.get().is_none()>
                {move || value.get().map_or("Select date…".to_string(), |d| layout.get().show(d))}
            </span>
            <span class="chevron">"▾"</span>
        </button>
        <Show when=move || open.get()>
            <div class="picker-backdrop" on:click=move |ev| { ev.prevent_default(); open.set(false); }></div>
            <div
                class="popover calendar"
                on:click=inert
                style=move || {
                    let (x, y) = anchor.get();
                    format!("left:{x}px;top:{y}px")
                }
            >
                <div class="calhead">
                    <button type="button" class="plain" title="Previous year" on:click=step(-12)>"«"</button>
                    <button type="button" class="plain" title="Previous month" on:click=step(-1)>"‹"</button>
                    <strong>{move || { let d = shown.get(); format!("{} {}", format::month(d), d.year()) }}</strong>
                    <button type="button" class="plain" title="Next month" on:click=step(1)>"›"</button>
                    <button type="button" class="plain" title="Next year" on:click=step(12)>"»"</button>
                </div>
                <div class="days">
                    {move || {
                        week.get()
                            .weekdays()
                            .into_iter()
                            .map(|name| view! { <span class="head">{name}</span> })
                            .collect_view()
                    }}
                    {move || {
                        month_cells(shown.get(), week.get())
                            .into_iter()
                            .map(|cell| match cell {
                                None => view! { <span></span> }.into_any(),
                                Some(day) => {
                                    let is_today = day == today;
                                    let chosen = move || value.get() == Some(day);
                                    view! {
                                        <button
                                            type="button"
                                            class="day"
                                            class:today=is_today
                                            class:on=chosen
                                            on:click=move |_| pick(Some(day))
                                        >
                                            {day.day()}
                                        </button>
                                    }
                                    .into_any()
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="foot">
                    <button type="button" class="plain" on:click=move |_| pick(Some(today))>"Today"</button>
                    {clearable.then(|| view! {
                        <button type="button" class="plain" on:click=move |_| pick(None)>"Clear"</button>
                    })}
                </div>
            </div>
        </Show>
    }
}

/// A date that is always there
#[component]
pub fn DateInput(
    day: RwSignal<NaiveDate>,
    #[prop(optional)] disabled: Option<Signal<bool>>,
) -> impl IntoView {
    view! {
        <DatePicker
            value=Signal::derive(move || Some(day.get()))
            on_change=Callback::new(move |d: Option<NaiveDate>| if let Some(d) = d { day.set(d) })
            disabled=disabled.unwrap_or_else(|| Signal::stored(false))
        />
    }
}

/// A date that can be left empty
#[component]
pub fn OptionalDate(
    value: RwSignal<Option<NaiveDate>>,
    #[prop(optional)] disabled: Option<Signal<bool>>,
) -> impl IntoView {
    view! {
        <DatePicker
            value=value
            on_change=Callback::new(move |d| value.set(d))
            clearable=true
            disabled=disabled.unwrap_or_else(|| Signal::stored(false))
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn months_and_years_are_stepped_to_their_first_day() {
        assert_eq!(month_shifted(d(2024, 3, 17), 0), d(2024, 3, 1));
        assert_eq!(month_shifted(d(2024, 3, 17), 1), d(2024, 4, 1));
        assert_eq!(month_shifted(d(2024, 1, 31), -1), d(2023, 12, 1));
        assert_eq!(month_shifted(d(2024, 11, 5), 12), d(2025, 11, 1));
        assert_eq!(month_shifted(d(2024, 11, 5), -12), d(2023, 11, 1));
    }

    #[test]
    fn a_month_starts_under_its_weekday() {
        // 1 Mar 2024 was a Friday, and the month has 31 days
        let cells = month_cells(d(2024, 3, 1), WeekStart::Monday);
        assert_eq!(cells.iter().take_while(|c| c.is_none()).count(), 4);
        assert_eq!(cells.iter().flatten().count(), 31);
        assert_eq!(
            month_cells(d(2024, 2, 1), WeekStart::Monday)
                .iter()
                .flatten()
                .count(),
            29
        );
        // and one place later when weeks start on Sunday
        let cells = month_cells(d(2024, 3, 1), WeekStart::Sunday);
        assert_eq!(cells.iter().take_while(|c| c.is_none()).count(), 5);
    }
}
