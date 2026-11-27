//! Dates and numbers as people read them.

// modules {{{
use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;
// }}}

// constants {{{
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
// }}}

/// How dates are written where they are typed or picked
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFormat {
    #[default]
    DayMonthYear,
    MonthDayYear,
    YearMonthDay,
}

// impl DateFormat {{{
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
// }}}

// DateFormat supporting types {{{
/// The day calendars start their weeks on
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeekStart {
    #[default]
    Monday,
    Sunday,
}

/// What a person has chosen for how things look, kept in this browser between visits
#[derive(Clone, Copy)]
pub struct Preferences {
    pub date_format: RwSignal<DateFormat>,
    pub week_start: RwSignal<WeekStart>,
}
// }}}

// impl WeekStart {{{
impl WeekStart {
    pub const ALL: [WeekStart; 2] = [WeekStart::Monday, WeekStart::Sunday];

    pub fn label(self) -> &'static str {
        match self {
            WeekStart::Monday => "Monday",
            WeekStart::Sunday => "Sunday",
        }
    }

    /// How many days after the start of the week a day falls
    pub fn offset(self, date: NaiveDate) -> usize {
        match self {
            WeekStart::Monday => date.weekday().num_days_from_monday() as usize,
            WeekStart::Sunday => date.weekday().num_days_from_sunday() as usize,
        }
    }

    /// The weekday names in the order a calendar shows them
    pub fn weekdays(self) -> [&'static str; 7] {
        let mut names = WEEKDAYS;
        if self == WeekStart::Sunday {
            names.rotate_right(1);
        }
        names
    }
}
// }}}

// constants {{{
const DATE_FORMAT: &str = "plinko.date_format";
const WEEK_START: &str = "plinko.week_start";
// }}}

// provide_preferences {{{
/// Reads the saved choices and keeps saving them. Call once, above everything that shows dates.
pub fn provide_preferences() {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    let saved = |key: &str| {
        storage
            .as_ref()
            .and_then(|s| s.get_item(key).ok().flatten())
    };
    let date_format = RwSignal::new(
        saved(DATE_FORMAT)
            .and_then(|label| DateFormat::ALL.into_iter().find(|f| f.label() == label))
            .unwrap_or_default(),
    );
    let week_start = RwSignal::new(
        saved(WEEK_START)
            .and_then(|label| WeekStart::ALL.into_iter().find(|w| w.label() == label))
            .unwrap_or_default(),
    );
    Effect::new(move |_| {
        if let Some(s) = &storage {
            let _ = s.set_item(DATE_FORMAT, date_format.get().label());
            let _ = s.set_item(WEEK_START, week_start.get().label());
        }
    });
    provide_context(Preferences {
        date_format,
        week_start,
    });
}
// }}}

// helpers {{{
pub fn use_preferences() -> Preferences {
    expect_context()
}

pub fn use_date_format() -> RwSignal<DateFormat> {
    use_preferences().date_format
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
// }}}

// tests {{{
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
    fn weeks_start_where_asked() {
        let sunday = d(2024, 3, 3);
        assert_eq!(WeekStart::Monday.offset(sunday), 6);
        assert_eq!(WeekStart::Sunday.offset(sunday), 0);
        assert_eq!(WeekStart::Sunday.weekdays()[0], "Sun");
        assert_eq!(WeekStart::Monday.weekdays()[0], "Mon");
    }
}
// }}}
