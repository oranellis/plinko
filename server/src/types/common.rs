use chrono::NaiveDate;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}
