use chrono::NaiveDate;

/// The days from `start` up to but not including `end`, so a zero length range has equal ends
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

impl DateRange {
    /// `None` if `end` is before `start`
    pub fn new(start: NaiveDate, end: NaiveDate) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn start(&self) -> NaiveDate {
        self.start
    }

    pub fn end(&self) -> NaiveDate {
        self.end
    }
}
