//! Dates and numbers as people read them.

use chrono::{Datelike, NaiveDate};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

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

pub fn parse_iso(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

/// `3 days late`, `on time`
pub fn delay(days: i64) -> String {
    match days {
        0 => "on time".to_string(),
        1 => "1 day late".to_string(),
        n => format!("{n} days late"),
    }
}

/// `1 day`, `5 days`
pub fn days(count: i64) -> String {
    match count {
        1 => "1 day".to_string(),
        n => format!("{n} days"),
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
        assert_eq!(parse_iso("2024-03-04"), Some(d(2024, 3, 4)));
        assert_eq!(parse_iso("04/03/2024"), None);
    }

    #[test]
    fn delays_and_durations_are_worded() {
        assert_eq!(delay(0), "on time");
        assert_eq!(delay(1), "1 day late");
        assert_eq!(delay(5), "5 days late");
        assert_eq!(days(1), "1 day");
        assert_eq!(days(2), "2 days");
    }
}
