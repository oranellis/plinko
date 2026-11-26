//! Where things go on the Gantt, as Plinko 1 did it: no row per task. Items are packed into shared
//! lanes, a lane holding as many items as fit one after another with at least one empty day
//! between. A group is a block of lanes of its own, its members packed together inside it, so the
//! group can be drawn as one rounded rectangle around them. All of it is worked out from the dates,
//! and nothing here knows how anything is drawn.

// modules {{{
use chrono::NaiveDate;
use plinko::planner::items::dependency::PlanNode;
use uuid::Uuid;

use crate::model::Look;
// }}}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
    pub placed: Vec<Placed>,
    pub lanes: usize,
}

// Arrangement supporting types {{{
/// What can be shown on the chart
pub type Mark = PlanNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Task,
    Milestone,
    Buffer,
    Group,
}

/// Something to place, with what the drawing needs to know about it
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub node: PlanNode,
    pub name: String,
    pub kind: Kind,
    /// The first day and the day after the last
    pub span: (NaiveDate, NaiveDate),
    pub look: Look,
    pub violated: bool,
    /// For a buffer: it has a day it is set to start on
    pub pinned: bool,
    pub target: bool,
    /// For a group, what is in it
    pub members: Vec<Entry>,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub mark: Mark,
    pub name: String,
    pub kind: Kind,
    pub span: (NaiveDate, NaiveDate),
    pub look: Look,
    pub violated: bool,
    /// For a buffer: it has a day it is set to start on
    pub pinned: bool,
    pub target: bool,
    pub lane: usize,
    /// How many groups it is inside
    pub depth: u32,
    /// For a group: the lanes it takes, its name's lane included
    pub lanes: usize,
    /// For a group: how many levels of groups are inside it, so outer ones are drawn larger
    pub levels: u32,
    pub members: usize,
    pub collapsed: bool,
}

/// The days after which each lane is free. An item may start on a day after its lane is free,
/// which leaves at least one empty day after the item before it.
#[derive(Default)]
struct Lanes {
    free: Vec<NaiveDate>,
}
// }}}

// impl Lanes {{{
impl Lanes {
    fn fits(&self, lane: usize, height: usize, start: NaiveDate) -> bool {
        (lane..lane + height).all(|l| self.free.get(l).is_none_or(|free| start > *free))
    }

    /// Takes the first lanes with room for `height` lanes from `start` to `end`, returning the first
    fn place(&mut self, start: NaiveDate, end: NaiveDate, height: usize) -> usize {
        let lane = (0..).find(|l| self.fits(*l, height, start)).unwrap_or(0);
        if self.free.len() < lane + height {
            self.free.resize(lane + height, NaiveDate::MIN);
        }
        for free in &mut self.free[lane..lane + height] {
            *free = end;
        }
        lane
    }
}
// }}}

// helpers {{{
fn placed(entry: &Entry, lane: usize, depth: u32) -> Placed {
    Placed {
        mark: entry.node,
        name: entry.name.clone(),
        kind: entry.kind,
        span: entry.span,
        look: entry.look,
        violated: entry.violated,
        pinned: entry.pinned,
        target: entry.target,
        lane,
        depth,
        lanes: 1,
        levels: 0,
        members: entry.members.len(),
        collapsed: entry.collapsed,
    }
}

fn sort_key(entry: &Entry) -> (NaiveDate, &str, Uuid) {
    (entry.span.0, entry.name.as_str(), entry.node.id())
}
// }}}

// pack {{{
/// Packs the entries into `lanes`, groups with their members, and returns what they placed
fn pack(entries: &[Entry], depth: u32, lanes: &mut Lanes, out: &mut Vec<Placed>) {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    for entry in sorted {
        let (start, end) = entry.span;
        if entry.kind != Kind::Group {
            let lane = lanes.place(start, end, 1);
            out.push(placed(entry, lane, depth));
            continue;
        }
        let mut inside = Vec::new();
        let mut inner = Lanes::default();
        if !entry.collapsed {
            pack(&entry.members, depth + 1, &mut inner, &mut inside);
        }
        // the group's own lane for its name, then the lanes of what is in it
        let height = 1 + inner.free.len();
        let lane = lanes.place(start, end, height);
        let levels = inside
            .iter()
            .filter(|p| p.kind == Kind::Group)
            .map(|p| p.levels + 1)
            .max()
            .unwrap_or(0);
        let mut group = placed(entry, lane, depth);
        group.lanes = height;
        group.levels = levels;
        out.push(group);
        for mut member in inside {
            member.lane += lane + 1;
            out.push(member);
        }
    }
}
// }}}

// arrange {{{
/// Packs a plan's items
pub fn arrange(entries: &[Entry]) -> Arrangement {
    let mut lanes = Lanes::default();
    let mut out = Vec::new();
    pack(entries, 0, &mut lanes, &mut out);
    Arrangement {
        lanes: lanes.free.len(),
        placed: out,
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Days;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + Days::new(u64::from(day))
    }

    fn entry(id: u128, name: &str, from: u32, last: u32) -> Entry {
        Entry {
            node: PlanNode::Task(Uuid::from_u128(id)),
            name: name.to_string(),
            kind: Kind::Task,
            span: (d(from), d(last + 1)),
            look: Look::NotStarted,
            violated: false,
            pinned: false,
            target: false,
            members: Vec::new(),
            collapsed: false,
        }
    }

    fn group(id: u128, name: &str, from: u32, last: u32, members: Vec<Entry>) -> Entry {
        Entry {
            node: PlanNode::TaskGroup(Uuid::from_u128(id)),
            kind: Kind::Group,
            members,
            ..entry(id, name, from, last)
        }
    }

    fn lane_of(arrangement: &Arrangement, name: &str) -> usize {
        arrangement
            .placed
            .iter()
            .find(|p| p.name == name)
            .unwrap()
            .lane
    }

    #[test]
    fn items_share_a_lane_when_there_is_an_empty_day_between_them() {
        let entries = [
            entry(1, "A", 2, 4),
            entry(2, "B", 2, 3),
            entry(3, "C", 5, 7),
            entry(4, "D", 7, 9),
        ];
        let arranged = arrange(&entries);
        // A is days 2-4 so the next on its lane starts after day 5. C starts on day 5, with
        // nothing between, so it takes the lane B opened. D is far enough on to go back.
        assert_eq!(lane_of(&arranged, "A"), 0);
        assert_eq!(lane_of(&arranged, "B"), 1);
        assert_eq!(lane_of(&arranged, "C"), 1);
        assert_eq!(lane_of(&arranged, "D"), 0);
        assert_eq!(arranged.lanes, 2);
    }

    #[test]
    fn the_arrangement_follows_start_then_name_and_not_the_order_given() {
        let forward = [entry(1, "B", 2, 3), entry(2, "A", 2, 3)];
        let reversed = [entry(2, "A", 2, 3), entry(1, "B", 2, 3)];
        assert_eq!(arrange(&forward), arrange(&reversed));
        let arranged = arrange(&forward);
        assert_eq!(lane_of(&arranged, "A"), 0);
        assert_eq!(lane_of(&arranged, "B"), 1);
    }

    #[test]
    fn a_milestone_takes_its_one_day() {
        let mut milestone = entry(1, "M", 4, 4);
        milestone.kind = Kind::Milestone;
        let entries = [entry(2, "A", 2, 3), milestone, entry(3, "B", 5, 6)];
        let arranged = arrange(&entries);
        // A ends on day 3, the milestone is on day 4 right after, so it cannot share A's lane
        assert_eq!(lane_of(&arranged, "A"), 0);
        assert_eq!(lane_of(&arranged, "M"), 1);
        // B starts on day 5, a day after the milestone and two after A, so it can follow A
        assert_eq!(lane_of(&arranged, "B"), 0);
    }

    #[test]
    fn a_group_is_a_block_with_its_members_packed_inside() {
        let members = vec![
            entry(2, "M1", 3, 5),
            entry(3, "M2", 4, 6),
            entry(4, "M3", 8, 9),
        ];
        let entries = [group(1, "G", 3, 9, members), entry(5, "X", 3, 4)];
        let arranged = arrange(&entries);
        let g = arranged.placed.iter().find(|p| p.name == "G").unwrap();
        // the name's lane and two lanes of members, since M1 and M2 overlap
        assert_eq!((g.lane, g.lanes, g.members), (0, 3, 3));
        // M1 and M3 share a lane, M2 has the next, inside the block below the name
        assert_eq!(lane_of(&arranged, "M1"), 1);
        assert_eq!(lane_of(&arranged, "M2"), 2);
        assert_eq!(lane_of(&arranged, "M3"), 1);
        assert!(
            arranged
                .placed
                .iter()
                .filter(|p| p.name.starts_with('M'))
                .all(|p| p.depth == 1)
        );
        // X starts with the group so goes under it, the block is not shared
        assert_eq!(lane_of(&arranged, "X"), 3);
        assert_eq!(arranged.lanes, 4);
    }

    #[test]
    fn groups_nest_and_outer_groups_know_how_deep_they_are() {
        let inner = group(2, "Inner", 3, 5, vec![entry(3, "T", 3, 5)]);
        let outer = group(1, "Outer", 3, 5, vec![inner]);
        let arranged = arrange(&[outer]);
        let get = |name: &str| arranged.placed.iter().find(|p| p.name == name).unwrap();
        assert_eq!(
            (get("Outer").lane, get("Outer").lanes, get("Outer").levels),
            (0, 3, 1)
        );
        assert_eq!(
            (get("Inner").lane, get("Inner").lanes, get("Inner").levels),
            (1, 2, 0)
        );
        assert_eq!((get("T").lane, get("T").depth), (2, 2));
    }

    #[test]
    fn collapsed_groups_take_one_lane_and_show_no_members() {
        let mut g = group(
            1,
            "G",
            3,
            9,
            vec![entry(2, "M1", 3, 5), entry(3, "M2", 4, 6)],
        );
        g.collapsed = true;
        let arranged = arrange(&[g]);
        assert_eq!(arranged.placed.len(), 1, "just the group");
        assert_eq!(arranged.placed[0].lanes, 1);
        assert_eq!(arranged.placed[0].members, 2);
    }

    #[test]
    fn empty_groups_still_take_their_lane() {
        let arranged = arrange(&[group(1, "G", 6, 8, Vec::new())]);
        assert_eq!(lane_of(&arranged, "G"), 0);
        assert_eq!(arranged.lanes, 1);
    }
}
// }}}
