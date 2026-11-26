// modules {{{
use crate::{
    planner::items::{dependency::PlanNode, task::Worker},
    scheduler::{
        day::Day,
        network::VertexId,
        table::{MaskId, RowId},
    },
};
// }}}

/// Something that takes days between two vertices: a task, a delay, a buffer or an empty group
#[derive(Clone, Debug, PartialEq)]
pub struct Arc {
    pub node: PlanNode,
    /// The position of its plan in `Network::plans`
    pub plan: u32,
    pub start: VertexId,
    pub end: VertexId,
    pub kind: ArcKind,
}

// Arc supporting types {{{
/// An arc in `Network::arcs`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArcId(pub u32);

/// Arcs differ by how their days are worked out
#[derive(Clone, Debug, PartialEq)]
pub enum ArcKind {
    Work(Work),
    Buffer(Buffer),
}

/// A number of days that every slot has its hours on and the mask allows, in one run that other
/// work does not break up. A delay is work with no slots on the full calendar.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    pub days: u32,
    pub slots: Vec<Slot>,
    pub mask: Option<MaskId>,
    /// Whose working days count when no slot asks for hours
    pub calendar: RowId,
    pub progress: Progress,
    /// Not started before this day
    pub floor_start: Option<Day>,
    /// Not ended before this day
    pub floor_end: Option<Day>,
    /// A hard date for its start
    pub latest_start: Option<Day>,
    /// A hard date for its end
    pub latest_end: Option<Day>,
}

/// What has happened to work, which decides what the scheduler can move
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Open,
    /// Started, working its hours through today and then what is left of its days
    Active {
        begun: Day,
    },
    /// Completed or on hold, fixed to its actual days
    Done {
        begun: Day,
        ended: Day,
    },
}

/// One worker a task asks for and the rows of the people who could be it
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub worker: Worker,
    pub hours: u8,
    /// In row order, a single row for a named or recorded person
    pub candidates: Vec<RowId>,
}

/// A plan buffer: it ends `until` but never before it starts, and no later than `shrink_to`,
/// the hard dates of what depends on it directly
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buffer {
    pub until: Until,
    pub calendar: RowId,
    pub shrink_to: Option<Day>,
}

/// Where a buffer ends when nothing shrinks it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// This many working days of its calendar after it starts, an unset buffer
    Days(u32),
    /// A fixed day, a set buffer whose start floats
    Day(Day),
}
// }}}

// impl Arc {{{
impl Arc {
    pub fn work(&self) -> Option<&Work> {
        match &self.kind {
            ArcKind::Work(work) => Some(work),
            ArcKind::Buffer(_) => None,
        }
    }
}
// }}}

// impl Work {{{
impl Work {
    /// A delay of calendar days, or of the working days of `calendar`
    pub fn wait(days: u32, calendar: RowId) -> Self {
        Self {
            days,
            slots: Vec::new(),
            mask: None,
            calendar,
            progress: Progress::Open,
            floor_start: None,
            floor_end: None,
            latest_start: None,
            latest_end: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.progress == Progress::Open
    }
}
// }}}

// impl Buffer {{{
impl Buffer {
    /// The end of the buffer when it starts on `start`, given the day its calendar puts the end
    /// of an unset one
    pub fn end(&self, start: Day, after_days: impl Fn(u32) -> Option<Day>) -> Option<Day> {
        let nominal = match self.until {
            Until::Days(days) => after_days(days)?,
            Until::Day(day) => day,
        };
        Some(nominal.min(self.shrink_to.unwrap_or(nominal)).max(start))
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(until: Until, shrink_to: Option<u32>) -> Buffer {
        Buffer {
            until,
            calendar: RowId(0),
            shrink_to: shrink_to.map(Day),
        }
    }

    #[test]
    fn buffers_end_where_asked_but_shrink_and_never_go_backwards() {
        let after = |days| Some(Day(10 + days));
        assert_eq!(
            buffer(Until::Days(5), None).end(Day(10), after),
            Some(Day(15))
        );
        assert_eq!(
            buffer(Until::Days(5), Some(12)).end(Day(10), after),
            Some(Day(12))
        );
        assert_eq!(
            buffer(Until::Days(5), Some(8)).end(Day(10), after),
            Some(Day(10))
        );
        // a set buffer keeps its end while its start floats, until the start passes it
        assert_eq!(
            buffer(Until::Day(Day(20)), None).end(Day(10), after),
            Some(Day(20))
        );
        assert_eq!(
            buffer(Until::Day(Day(20)), None).end(Day(25), after),
            Some(Day(25))
        );
    }
}
// }}}
