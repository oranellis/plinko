//! Dates and numbers as people read them.

use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// How dates are written where they are typed or picked
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFormat {
    #[default]
    DayMonthYear,
    MonthDayYear,
    YearMonthDay,
}

impl DateFormat {
    pub const ALL: [DateFormat; 3] = [
        DateFormat::DayMonthYear,
        DateFormat::MonthDayYear,
        DateFormat::YearMonthDay,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DateFormat::DayMonthYear => "DD / MM / YYYY",
            DateFormat::MonthDayYear => "MM / DD / YYYY",
            DateFormat::YearMonthDay => "YYYY / MM / DD",
        }
    }

    pub fn show(self, date: NaiveDate) -> String {
        let (d, m, y) = (date.day(), date.month(), date.year());
        match self {
            DateFormat::DayMonthYear => format!("{d:02} / {m:02} / {y}"),
            DateFormat::MonthDayYear => format!("{m:02} / {d:02} / {y}"),
            DateFormat::YearMonthDay => format!("{y} / {m:02} / {d:02}"),
        }
    }
}

const STORED: &str = "plinko.date_format";

/// The format this browser uses, kept between visits. Call once, above everything that shows dates.
pub fn provide_date_format() {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    let saved = storage
        .as_ref()
        .and_then(|s| s.get_item(STORED).ok().flatten())
        .and_then(|label| DateFormat::ALL.into_iter().find(|f| f.label() == label));
    let chosen = RwSignal::new(saved.unwrap_or_default());
    Effect::new(move |_| {
        if let Some(s) = &storage {
            let _ = s.set_item(STORED, chosen.get().label());
        }
    });
    provide_context(chosen);
}

pub fn use_date_format() -> RwSignal<DateFormat> {
    expect_context()
}

pub fn month(date: NaiveDate) -> &'static str {
    MONTHS[date.month0() as usize]
}

pub fn weekday(date: NaiveDate) -> &'static str {
    WEEKDAYS[date.weekday().num_days_from_monday() as usize]
}

/// `Mon 4 Mar`
pub fn day(date: NaiveDate) -> String {
    format!("{} {} {}", weekday(date), date.day(), month(date))
}

/// `2024-03-04`, which is what a date input holds
pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// `3 days late`, `on time`
pub fn delay(days: i64) -> String {
    match days {
        0 => "on time".to_string(),
        1 => "1 day late".to_string(),
        n => format!("{n} days late"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn dates_read_as_people_say_them() {
        assert_eq!(day(d(2024, 3, 4)), "Mon 4 Mar");
        assert_eq!(iso(d(2024, 3, 4)), "2024-03-04");
    }

    #[test]
    fn dates_are_written_in_the_chosen_order() {
        let date = d(2024, 3, 4);
        assert_eq!(DateFormat::default().show(date), "04 / 03 / 2024");
        assert_eq!(DateFormat::MonthDayYear.show(date), "03 / 04 / 2024");
        assert_eq!(DateFormat::YearMonthDay.show(date), "2024 / 03 / 04");
    }

    #[test]
    fn delays_and_durations_are_worded() {
        assert_eq!(delay(0), "on time");
        assert_eq!(delay(1), "1 day late");
        assert_eq!(delay(5), "5 days late");
    }
}
