// modules {{{
use std::collections::HashMap;

use chrono::{DateTime, NaiveDate};
use plinko::{
    availability::workload::Workload,
    availability::{
        rule::{NewRule, RuleKind, WeeklySchedule},
        rule_set::RuleSetUpdate,
    },
    codec,
    common::{
        crud::Crud,
        dates::{NodeConstraint, NodeEnd, TimeConstraint},
        short_id::ShortId,
    },
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::items::{
        dependency::PlanNode,
        milestone::{DerivedStatus, MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelayPatch},
        task::{Assignee, NewTask, PersistentState, Status, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroupPatch},
    },
    planner::resources::{
        affinity::NewAffinity,
        tag::NewTag,
        target::NewTarget,
        user::{NewPlanUser, PlanUserDataPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler::{
        Hints, solve,
        state::{PlanVolatileState, UnscheduledReason},
    },
};
use uuid::Uuid;
// }}}

// helpers {{{
/// Dates in January 2024, which starts on a Monday
fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
}

/// Today is the Sunday before, so the first day a task may start is Monday the 1st
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()
}

fn h(hours: u8) -> Workload {
    Workload::new_const(hours)
}

type Dep = PlanNode;

fn after(node: PlanNode) -> Dep {
    node
}

fn deps(deps: &[Dep]) -> Vec<PlanNode> {
    deps.to_vec()
}

// World {{{
/// A tenant with one plan, built with ids that repeat from one world to the next
struct World {
    tenant: Tenant,
    root: Uuid,
    plan: ShortId,
    next: u128,
}

impl World {
    fn new() -> Self {
        let tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), today());
        let root = *tenant.users().keys().next().unwrap();
        let mut world = Self {
            tenant,
            root,
            plan: ShortId::new(),
            next: 1,
        };
        let new = NewPlan {
            id: world.plan,
            name: "Plan".to_string(),
        };
        world.apply(TenantUpdateType::Plan(PlanTenantUpdate::Add(new)));
        world
    }

    fn id(&mut self) -> Uuid {
        self.next += 1;
        Uuid::from_u128(self.next)
    }

    fn apply(&mut self, update_type: TenantUpdateType) {
        let update = TenantUpdate {
            time: DateTime::from_timestamp(978307200, 0).unwrap(),
            actor: self.root,
            update_type,
        };
        self.tenant.apply_update(update).unwrap();
    }

    fn plan_update(&mut self, update: PlanUpdate) {
        let update = PlanTenantUpdate::Update(self.plan, Box::new(update));
        self.apply(TenantUpdateType::Plan(update));
    }

    fn user(&mut self) -> Uuid {
        let id = self.id();
        let new = NewTenantLoginUser {
            id,
            name: "User".to_string(),
            email: format!("{id}@example.com"),
            role: Role::Member,
        };
        self.apply(TenantUpdateType::User(Crud::Add(new)));
        self.plan_update(PlanUpdate::User(Crud::Add(NewPlanUser { id })));
        id
    }

    /// Replaces a user's weekly hours, Monday first
    fn weekly_hours(&mut self, user: Uuid, week: [u8; 7]) {
        let kind = RuleKind::Weekly(WeeklySchedule::new(week.map(h)));
        let new = NewRule {
            name: String::new(),
            stop_day: None,
            id: self.id(),
            start_day: NaiveDate::from_ymd_opt(2000, 1, 3).unwrap(),
            kind,
        };
        let update = RuleSetUpdate::Rule(Crud::Add(new));
        self.apply(TenantUpdateType::Availability(user, update));
    }

    fn tag(&mut self, holders: &[Uuid]) -> Uuid {
        let id = self.id();
        let new = NewTag {
            id,
            name: "Tag".to_string(),
        };
        self.plan_update(PlanUpdate::Tag(Crud::Add(new)));
        for user in holders {
            let patch = PlanUserDataPatch {
                id: *user,
                tags: Some([id].into()),
            };
            self.plan_update(PlanUpdate::User(Crud::Update(patch)));
        }
        id
    }

    fn task(&mut self, duration: u32, who: &[(Worker, u8)], on: &[Dep]) -> Uuid {
        self.task_in(Parent::Plan, duration, who, on)
    }

    fn task_in(&mut self, parent: Parent, duration: u32, who: &[(Worker, u8)], on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewTask {
            id,
            name: "Task".to_string(),
            parent,
        };
        self.plan_update(PlanUpdate::Task(Crud::Add(new)));
        let allocations: HashMap<Worker, Workload> =
            who.iter().map(|&(w, hours)| (w, h(hours))).collect();
        self.patch(id, |p| {
            p.duration = Some(duration);
            p.allocations = Some(allocations);
            p.dependencies = Some(deps(on));
        });
        id
    }

    fn patch(&mut self, id: Uuid, change: impl FnOnce(&mut TaskPatch)) {
        let mut patch = TaskPatch {
            id,
            name: None,
            parent: None,
            duration: None,
            time_constraint: None,
            dependencies: None,
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        };
        change(&mut patch);
        self.plan_update(PlanUpdate::Task(Crud::Update(patch)));
    }

    fn set_state(&mut self, id: Uuid, status: Status, from: Option<u32>, to: Option<u32>) {
        let state = PersistentState {
            status,
            actual_start_date: from.map(d),
            actual_end_date: to.map(d),
        };
        self.patch(id, |p| p.persistent_state = Some(state));
    }

    fn milestone(&mut self, on: &[Dep]) -> Uuid {
        self.milestone_in(Parent::Plan, on)
    }

    fn milestone_in(&mut self, parent: Parent, on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewMilestone {
            id,
            name: "Milestone".to_string(),
            parent,
        };
        self.plan_update(PlanUpdate::Milestone(Crud::Add(new)));
        let patch = MilestonePatch {
            id,
            name: None,
            parent: None,
            time_constraint: None,
            dependencies: Some(deps(on)),
            persistent_state: None,
        };
        self.plan_update(PlanUpdate::Milestone(Crud::Update(patch)));
        id
    }

    fn buffer(&mut self, duration: u32, on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewPlanBuffer {
            id,
            parent: Parent::Plan,
            duration,
        };
        self.plan_update(PlanUpdate::PlanBuffer(Crud::Add(new)));
        let patch = PlanBufferPatch {
            id,
            parent: None,
            dependencies: Some(deps(on)),
            set_start_day: None,
            duration: None,
        };
        self.plan_update(PlanUpdate::PlanBuffer(Crud::Update(patch)));
        id
    }

    fn delay(&mut self, duration: u32, on: &[Dep]) -> Uuid {
        self.delay_in(Parent::Plan, duration, on)
    }

    fn delay_in(&mut self, parent: Parent, duration: u32, on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewPlanDelay {
            id,
            name: None,
            parent,
            duration,
        };
        self.plan_update(PlanUpdate::PlanDelay(Crud::Add(new)));
        let patch = PlanDelayPatch {
            id,
            name: None,
            parent: None,
            dependencies: Some(deps(on)),
            duration: None,
        };
        self.plan_update(PlanUpdate::PlanDelay(Crud::Update(patch)));
        id
    }

    fn group(&mut self, on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewTaskGroup {
            id,
            name: "Group".to_string(),
            parent: Parent::Plan,
        };
        self.plan_update(PlanUpdate::TaskGroup(Crud::Add(new)));
        let patch = TaskGroupPatch {
            time_constraint: None,
            id,
            name: None,
            dependencies: Some(deps(on)),
            parent: None,
        };
        self.plan_update(PlanUpdate::TaskGroup(Crud::Update(patch)));
        id
    }

    fn target(&mut self, node: PlanNode) -> Uuid {
        let id = self.id();
        let new = NewTarget {
            id,
            node,
            priority: 0,
        };
        self.plan_update(PlanUpdate::Target(Crud::Add(new)));
        id
    }

    fn solve(&mut self) -> PlanVolatileState {
        let mut state = solve(&mut self.tenant, &Hints::default()).state;
        state.plans.remove(&self.plan).expect("the plan is solved")
    }
}
// }}}

fn user(id: Uuid) -> Worker {
    Worker::User(id)
}

fn node(id: Uuid) -> PlanNode {
    PlanNode::Task(id)
}

/// The first day of a task and the day after its last
fn span(state: &PlanVolatileState, task: Uuid) -> (NaiveDate, NaiveDate) {
    let range = state.tasks[&task].date_range;
    (range.start(), range.end())
}

fn days_of(state: &PlanVolatileState, task: Uuid, who: Uuid) -> Vec<(NaiveDate, u8)> {
    let days = &state.tasks[&task].assignments[&Assignee::User(who)];
    days.iter()
        .map(|(day, hours)| (*day, hours.get()))
        .collect()
}

fn only_days(state: &PlanVolatileState, task: Uuid, who: Uuid) -> Vec<u32> {
    use chrono::Datelike;
    days_of(state, task, who)
        .iter()
        .map(|(day, _)| day.day())
        .collect()
}
// }}}

// a_task_works_its_duration_in_days {{{
#[test]
fn a_task_works_its_duration_in_days() {
    let mut world = World::new();
    let alice = world.user();
    let task = world.task(3, &[(user(alice), 8)], &[]);
    let state = world.solve();
    assert_eq!(span(&state, task), (d(1), d(4)));
    assert_eq!(
        days_of(&state, task, alice),
        [(d(1), 8), (d(2), 8), (d(3), 8)]
    );
    let workers = &state.tasks[&task].workers;
    assert_eq!(workers[&user(alice)], Assignee::User(alice));
    assert!(state.unscheduled.is_empty());
}
// }}}

// tasks_with_no_workers_last_their_duration_in_plan_working_days {{{
#[test]
fn tasks_with_no_workers_last_their_duration_in_plan_working_days() {
    let mut world = World::new();
    let task = world.task(6, &[], &[]);
    let state = world.solve();
    // Monday to the next Monday, skipping the weekend
    assert_eq!(span(&state, task), (d(1), d(9)));
    assert!(state.tasks[&task].assignments.is_empty());
}
// }}}

// two_strict_tasks_share_a_person_who_has_the_hours {{{
#[test]
fn two_strict_tasks_share_a_person_who_has_the_hours() {
    let mut world = World::new();
    let alice = world.user();
    let a = world.task(1, &[(user(alice), 4)], &[]);
    let b = world.task(1, &[(user(alice), 4)], &[]);
    let state = world.solve();
    assert_eq!(span(&state, a), (d(1), d(2)));
    assert_eq!(span(&state, b), (d(1), d(2)));
}
// }}}

// strict_workers_stay_on_the_same_days {{{
#[test]
fn strict_workers_stay_on_the_same_days() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let task = world.task(4, &[(user(alice), 4), (user(bob), 8)], &[]);
    let state = world.solve();
    assert_eq!(span(&state, task), (d(1), d(5)));
    assert_eq!(only_days(&state, task, alice), [1, 2, 3, 4]);
    assert_eq!(only_days(&state, task, bob), [1, 2, 3, 4]);
    assert!(
        days_of(&state, task, alice)
            .iter()
            .all(|(_, hours)| *hours == 4)
    );
}
// }}}

// strict_runs_restart_on_a_busy_day_but_not_across_weekends {{{
#[test]
fn strict_runs_restart_on_a_busy_day_but_not_across_weekends() {
    let mut world = World::new();
    let alice = world.user();
    let holder = world.task(1, &[(user(alice), 4)], &[]);
    world.patch(holder, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(3)).into())
    });
    let run = world.task(3, &[(user(alice), 8)], &[]);
    let long = world.task(6, &[], &[]);
    let state = world.solve();
    assert_eq!(span(&state, holder), (d(3), d(4)));
    // Wednesday is half taken, so the run starts over on Thursday, across the weekend
    assert_eq!(only_days(&state, run, alice), [4, 5, 8]);
    assert_eq!(span(&state, run), (d(4), d(9)));
    assert_eq!(span(&state, long), (d(1), d(9)));
    assert!(state.constraint_violations.is_empty());
}
// }}}

// work_in_progress_claims_hours_before_longer_chains {{{
#[test]
fn work_in_progress_claims_hours_before_longer_chains() {
    let mut world = World::new();
    let alice = world.user();
    let active = world.task(5, &[(user(alice), 8)], &[]);
    let first = world.task(1, &[(user(alice), 8)], &[]);
    let long = world.task(5, &[(user(alice), 8)], &[after(node(first))]);
    world.set_state(active, Status::InProgress, Some(1), None);
    let state = world.solve();
    assert_eq!(span(&state, active), (d(1), d(6)));
    assert_eq!(only_days(&state, active, alice), [1, 2, 3, 4, 5]);
    // the week is taken, so the longer chain waits for Monday
    assert_eq!(span(&state, first), (d(8), d(9)));
    assert_eq!(span(&state, long).0, d(9));
}
// }}}

// work_in_progress_runs_up_to_today_even_past_its_duration {{{
#[test]
fn work_in_progress_runs_up_to_today_even_past_its_duration() {
    let mut world = World::new();
    let alice = world.user();
    let task = world.task(1, &[(user(alice), 8)], &[]);
    world.set_state(task, Status::InProgress, Some(1), None);
    world.apply(TenantUpdateType::SetToday(d(5)));
    let state = world.solve();
    assert_eq!(only_days(&state, task, alice), [1, 2, 3, 4, 5]);
    assert_eq!(span(&state, task), (d(1), d(6)));
}
// }}}

// work_can_start_today_once_work_in_progress_has_its_hours {{{
#[test]
fn work_can_start_today_once_work_in_progress_has_its_hours() {
    let mut world = World::new();
    let (alice, bob, carol) = (world.user(), world.user(), world.user());
    let going = world.task(5, &[(user(alice), 8)], &[]);
    world.set_state(going, Status::InProgress, Some(1), None);
    let waits = world.task(1, &[(user(alice), 8)], &[]);
    let free = world.task(1, &[(user(bob), 8)], &[]);
    let done = world.task(2, &[(user(carol), 8)], &[]);
    world.set_state(done, Status::Completed, Some(2), Some(3));
    let reached = world.milestone(&[]);
    let next = world.task(
        1,
        &[(user(carol), 8)],
        &[after(PlanNode::Milestone(reached))],
    );
    world.apply(TenantUpdateType::SetToday(d(3)));
    let state = world.solve();
    // alice has today taken, bob does not
    assert_eq!(span(&state, waits).0, d(8));
    assert_eq!(span(&state, free), (d(3), d(4)));
    // a milestone with nothing to wait on is today, and what follows it can start today, on
    // hours that finished work no longer holds
    assert_eq!(state.milestones[&reached].date, d(3));
    assert_eq!(span(&state, next), (d(3), d(4)));
}
// }}}

// completed_work_is_inferred_from_its_dates {{{
#[test]
fn completed_work_is_inferred_from_its_dates() {
    let mut world = World::new();
    let alice = world.user();
    let done = world.task(3, &[(user(alice), 8)], &[]);
    let next = world.task(1, &[(user(alice), 8)], &[after(node(done))]);
    let day = |n| NaiveDate::from_ymd_opt(2023, 12, n).unwrap();
    let state = PersistentState {
        status: Status::Completed,
        actual_start_date: Some(day(18)),
        actual_end_date: Some(day(20)),
    };
    world.patch(done, |p| p.persistent_state = Some(state));
    let state = world.solve();
    assert_eq!(state.tasks[&done].date_range.start(), day(18));
    assert_eq!(state.tasks[&done].date_range.end(), day(21));
    // finished work holds no hours
    assert!(days_of(&state, done, alice).is_empty());
    assert_eq!(span(&state, next), (d(1), d(2)));
}
// }}}

// milestones_land_on_the_last_day_of_the_work_before_them {{{
#[test]
fn milestones_land_on_the_last_day_of_the_work_before_them() {
    let mut world = World::new();
    let alice = world.user();
    let first = world.task(3, &[(user(alice), 8)], &[]);
    let marker = world.milestone(&[after(node(first))]);
    let again = world.milestone(&[after(PlanNode::Milestone(marker))]);
    let next = world.task(
        1,
        &[(user(alice), 8)],
        &[after(PlanNode::Milestone(marker))],
    );
    let direct = world.task(1, &[], &[after(node(first))]);
    let state = world.solve();
    assert_eq!(state.milestones[&marker].date, d(3));
    assert_eq!(state.milestones[&again].date, d(3));
    assert_eq!(
        state.milestones[&marker].derived_status,
        DerivedStatus::NotStarted
    );
    // a task after the marker starts the day after the work finished, like one with no marker
    assert_eq!(span(&state, next), (d(4), d(5)));
    assert_eq!(span(&state, direct), (d(4), d(5)));
}
// }}}

// milestones_stay_on_a_day_worked_outside_the_usual_week {{{
#[test]
fn milestones_stay_on_a_day_worked_outside_the_usual_week() {
    let mut world = World::new();
    let alice = world.user();
    world.weekly_hours(alice, [8, 8, 8, 8, 8, 8, 0]);
    let first = world.task(6, &[(user(alice), 8)], &[]);
    let marker = world.milestone(&[after(node(first))]);
    let next = world.task(
        1,
        &[(user(alice), 8)],
        &[after(PlanNode::Milestone(marker))],
    );
    let state = world.solve();
    assert_eq!(span(&state, first), (d(1), d(7)));
    assert_eq!(state.milestones[&marker].date, d(6), "the Saturday");
    // Sunday is the earliest, but nobody works it
    assert_eq!(span(&state, next), (d(8), d(9)));
}
// }}}

// milestone_statuses_follow_what_they_depend_on {{{
#[test]
fn milestone_statuses_follow_what_they_depend_on() {
    let mut world = World::new();
    let alice = world.user();
    let done = world.task(1, &[(user(alice), 8)], &[]);
    let busy = world.task(1, &[(user(alice), 8)], &[]);
    world.set_state(done, Status::Completed, Some(1), Some(1));
    world.set_state(busy, Status::InProgress, Some(1), None);
    let complete = world.milestone(&[after(node(done))]);
    let active = world.milestone(&[after(node(done)), after(node(busy))]);
    let waiting = world.milestone(&[after(PlanNode::Milestone(active))]);
    let state = world.solve();
    assert_eq!(
        state.milestones[&complete].derived_status,
        DerivedStatus::Completed
    );
    assert_eq!(
        state.milestones[&active].derived_status,
        DerivedStatus::InProgress
    );
    assert_eq!(
        state.milestones[&waiting].derived_status,
        DerivedStatus::NotStarted
    );
    // finished work stays where it happened, the rest cannot be in the past
    assert_eq!(state.milestones[&complete].date, d(1));
    assert!(state.milestones[&waiting].date >= today());
}
// }}}

// a_milestone_that_happened_is_on_the_last_day_of_the_work_before_it {{{
#[test]
fn a_milestone_that_happened_is_on_the_last_day_of_the_work_before_it() {
    let mut world = World::new();
    let done = world.task(2, &[], &[]);
    world.set_state(done, Status::Completed, Some(2), Some(3));
    let dropped = world.task(1, &[], &[]);
    world.set_state(dropped, Status::Dropped, None, None);
    let group = world.group(&[]);
    let grouped = world.milestone_in(Parent::Node(group), &[after(node(done))]);
    let both = world.milestone(&[after(node(done)), after(node(dropped))]);
    let only_dropped = world.milestone(&[after(node(dropped))]);
    let later = world.group(&[]);
    constrain_group(&mut world, later, TimeConstraint::Earliest(d(4)).into());
    let empty = world.milestone_in(Parent::Node(later), &[]);
    world.apply(TenantUpdateType::SetToday(d(10)));
    let state = world.solve();
    // the start of today does not count as something that happened
    assert_eq!(state.milestones[&grouped].date, d(3));
    assert_eq!(state.milestones[&both].date, d(3));
    // with no work that happened before it, it is today
    assert_eq!(state.milestones[&only_dropped].date, d(10));
    assert_eq!(state.milestones[&empty].date, d(10));
}
// }}}

// dropped_tasks_are_transparent_and_on_hold_tasks_end_when_held {{{
#[test]
fn dropped_tasks_are_transparent_and_on_hold_tasks_end_when_held() {
    let mut world = World::new();
    let alice = world.user();
    let first = world.task(2, &[(user(alice), 8)], &[]);
    let dropped = world.task(5, &[(user(alice), 8)], &[after(node(first))]);
    let last = world.task(1, &[], &[after(node(dropped))]);
    world.set_state(dropped, Status::Dropped, None, None);
    let bob = world.user();
    let held = world.task(3, &[(user(bob), 8)], &[]);
    world.set_state(held, Status::OnHold, Some(1), Some(2));
    let after_hold = world.task(1, &[], &[after(node(held))]);
    let marker = world.milestone(&[after(node(held))]);
    let state = world.solve();
    assert_eq!(
        state.unscheduled[&node(dropped)],
        UnscheduledReason::Dropped
    );
    assert!(!state.tasks.contains_key(&dropped));
    assert_eq!(span(&state, last), (d(3), d(4)));
    assert_eq!(span(&state, held), (d(1), d(3)));
    assert_eq!(span(&state, after_hold), (d(3), d(4)));
    assert_eq!(
        state.milestones[&marker].derived_status,
        DerivedStatus::NotStarted
    );
}
// }}}

// time_constraints_hold_or_are_reported_violated {{{
#[test]
fn time_constraints_hold_or_are_reported_violated() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    world.weekly_hours(bob, [8, 8, 8, 8, 8, 0, 0]);
    let fixed = world.task(1, &[(user(alice), 8)], &[]);
    world.patch(fixed, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(10)).into())
    });
    let first = world.task(3, &[(user(alice), 8)], &[]);
    let late = world.task(1, &[], &[after(node(first))]);
    world.patch(late, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(2)).into())
    });
    // a Saturday that nobody works
    let weekend = world.task(1, &[(user(bob), 8)], &[]);
    world.patch(weekend, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(6)).into())
    });
    let early = world.task(1, &[], &[]);
    world.patch(early, |p| {
        p.time_constraint = Some(TimeConstraint::Earliest(d(3)).into())
    });
    let state = world.solve();
    assert_eq!(span(&state, fixed), (d(10), d(11)));
    assert_eq!(span(&state, early), (d(3), d(4)));
    assert_eq!(span(&state, late), (d(4), d(5)));
    let violations = &state.constraint_violations;
    assert_eq!(violations.len(), 2);
    assert_eq!(violations[&node(late)].scheduled, d(4));
    assert_eq!(violations[&node(weekend)].scheduled, d(8));
}
// }}}

// hard_dated_chains_go_before_target_chains_before_the_rest {{{
#[test]
fn hard_dated_chains_go_before_target_chains_before_the_rest() {
    let mut world = World::new();
    let alice = world.user();
    let hard = world.task(1, &[(user(alice), 8)], &[]);
    world.patch(hard, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(3)).into())
    });
    let chain = world.task(3, &[(user(alice), 8)], &[]);
    let goal = world.milestone(&[after(node(chain))]);
    world.target(PlanNode::Milestone(goal));
    let rest = world.task(1, &[(user(alice), 8)], &[]);
    let state = world.solve();
    assert_eq!(span(&state, hard), (d(3), d(4)));
    assert!(state.constraint_violations.is_empty());
    // the target chain works around the dated task, the rest takes what is left
    assert_eq!(only_days(&state, chain, alice), [4, 5, 8]);
    assert_eq!(state.milestones[&goal].date, d(8));
    assert_eq!(span(&state, rest), (d(1), d(2)));
    assert_eq!(state.targets.len(), 1);
}
// }}}

// tasks_off_the_target_chain_never_delay_it {{{
#[test]
fn tasks_off_the_target_chain_never_delay_it() {
    let mut world = World::new();
    let alice = world.user();
    let rest = world.task(5, &[(user(alice), 8)], &[]);
    let chain = world.task(2, &[(user(alice), 8)], &[]);
    let goal = world.milestone(&[after(node(chain))]);
    let target = world.target(PlanNode::Milestone(goal));
    let state = world.solve();
    assert_eq!(span(&state, chain), (d(1), d(3)));
    assert_eq!(state.targets[&target].forecast, d(2));
    assert_eq!(span(&state, rest), (d(3), d(10)));
}
// }}}

// tags_and_groups_pick_whoever_finishes_first {{{
#[test]
fn tags_and_groups_pick_whoever_finishes_first() {
    let mut world = World::new();
    let (alice, bob, carol) = (world.user(), world.user(), world.user());
    let tag = world.tag(&[alice, bob]);
    let busy = world.task(2, &[(user(alice), 8)], &[]);
    world.patch(busy, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(1)).into())
    });
    let by_tag = world.task(1, &[(Worker::Tag(tag), 8)], &[]);
    let state = world.solve();
    assert_eq!(
        state.tasks[&by_tag].workers[&Worker::Tag(tag)],
        Assignee::User(bob)
    );
    assert_eq!(span(&state, by_tag), (d(1), d(2)));

    // carol is in a group nobody else is in
    let group = world.id();
    let new = plinko::multitenancy::user_group::NewTenantUserGroup {
        id: group,
        name: "Group".to_string(),
        parent: None,
        users: [carol].into(),
    };
    world.apply(TenantUpdateType::UserGroup(Crud::Add(new)));
    world.plan_update(PlanUpdate::UserGroup(Crud::Add(
        plinko::planner::resources::user::NewPlanUserGroup { id: group },
    )));
    let by_group = world.task(1, &[(Worker::UserGroup(group), 8)], &[]);
    let state = world.solve();
    let picked = state.tasks[&by_group].workers[&Worker::UserGroup(group)];
    assert_eq!(picked, Assignee::User(carol));
}
// }}}

// recorded_workers_replace_the_named_user {{{
#[test]
fn recorded_workers_replace_the_named_user() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let task = world.task(1, &[(user(alice), 8)], &[]);
    world.patch(task, |p| {
        p.actual_workers = Some(HashMap::from([(user(alice), Assignee::User(bob))]));
    });
    let state = world.solve();
    assert_eq!(days_of(&state, task, bob), [(d(1), 8)]);
    assert!(
        !state.tasks[&task]
            .assignments
            .contains_key(&Assignee::User(alice))
    );
}
// }}}

// a_task_that_depends_on_nothing_starts_with_the_plan {{{
#[test]
fn a_task_that_depends_on_nothing_starts_with_the_plan() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let free = world.task(1, &[(user(alice), 8)], &[]);
    let anchored = world.task(1, &[(user(bob), 8)], &[]);
    let state = world.solve();
    assert_eq!(span(&state, free).0, span(&state, anchored).0);
}
// }}}

// what_cannot_be_scheduled_says_why {{{
#[test]
fn what_cannot_be_scheduled_says_why() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let nobody = world.tag(&[]);
    // bob never works
    world.weekly_hours(bob, [0; 7]);
    let no_one = world.task(1, &[(Worker::Tag(nobody), 8)], &[]);
    let no_time = world.task(1, &[(user(bob), 8)], &[]);
    let behind = world.task(1, &[(user(alice), 8)], &[after(node(no_one))]);
    let first = world.task(1, &[(user(alice), 8)], &[]);
    let fine = world.task(
        1,
        &[(user(alice), 8)],
        &[after(node(no_one)), after(node(first))],
    );
    let state = world.solve();
    let reason = |task| state.unscheduled[&node(task)];
    assert_eq!(reason(no_one), UnscheduledReason::NoEligibleWorkers);
    assert_eq!(reason(no_time), UnscheduledReason::NoCapacity);
    // waiting for the plan to start is not enough when all it depends on is unscheduled
    assert_eq!(reason(behind), UnscheduledReason::DependsOnUnscheduled);
    // with something it depends on scheduled, it goes after that
    assert_eq!(span(&state, fine), (d(2), d(3)));
}
// }}}

// what_is_in_a_group_waits_for_the_group_whatever_else_it_depends_on {{{
#[test]
fn what_is_in_a_group_waits_for_the_group_whatever_else_it_depends_on() {
    let mut world = World::new();
    let early = world.task(1, &[], &[]);
    let late = world.task(5, &[], &[]);
    let group = world.group(&[after(node(late))]);
    // it depends only on the early task, but its group does not start until the late one ends
    let member = world.task_in(Parent::Node(group), 1, &[], &[after(node(early))]);
    let hollow = world.group(&[]);
    let nobody = world.tag(&[]);
    let stuck = world.task(1, &[(Worker::Tag(nobody), 8)], &[]);
    let inside = world.task_in(Parent::Node(hollow), 1, &[], &[after(node(stuck))]);
    let state = world.solve();
    assert_eq!(span(&state, early), (d(1), d(2)));
    assert_eq!(span(&state, member), (d(8), d(9)));
    // its group having started does not schedule what depends on nothing that is scheduled
    let reason = state.unscheduled[&node(inside)];
    assert_eq!(reason, UnscheduledReason::DependsOnUnscheduled);
}
// }}}

// a_wait_after_finished_work_starts_where_that_ended {{{
#[test]
fn a_wait_after_finished_work_starts_where_that_ended() {
    let mut world = World::new();
    let alice = world.user();
    let done = world.task(1, &[(user(alice), 8)], &[]);
    let wait = world.delay(10, &[after(node(done))]);
    let day = |n| NaiveDate::from_ymd_opt(2023, 12, n).unwrap();
    let state = PersistentState {
        status: Status::Completed,
        actual_start_date: Some(day(20)),
        actual_end_date: Some(day(20)),
    };
    world.patch(done, |p| p.persistent_state = Some(state));
    let state = world.solve();
    // ten days from the day after it was done, not from today
    let range = state.plan_delays[&wait].date_range;
    assert_eq!((range.start(), range.end()), (day(21), day(31)));
}
// }}}

// affinities_limit_the_days_worked {{{
#[test]
fn affinities_limit_the_days_worked() {
    let mut world = World::new();
    let alice = world.user();
    let affinity = |world: &mut World, allowed: [bool; 7]| {
        let id = world.id();
        let new = NewAffinity {
            id,
            name: "Days".to_string(),
        };
        world.plan_update(PlanUpdate::Affinity(Crud::Add(new)));
        let kind = RuleKind::Weekly(WeeklySchedule::new(allowed));
        let rule = NewRule {
            name: String::new(),
            stop_day: None,
            id: world.id(),
            start_day: NaiveDate::from_ymd_opt(2000, 1, 3).unwrap(),
            kind,
        };
        let update = RuleSetUpdate::Rule(Crud::Add(rule));
        world.plan_update(PlanUpdate::AffinityRules(id, update));
        id
    };
    let late_week = affinity(&mut world, [false, false, false, true, true, false, false]);
    let never = affinity(&mut world, [false; 7]);
    let limited = world.task(2, &[(user(alice), 8)], &[]);
    world.patch(limited, |p| p.affinities = Some([late_week].into()));
    let impossible = world.task(1, &[(user(alice), 8)], &[]);
    world.patch(impossible, |p| p.affinities = Some([never].into()));
    let state = world.solve();
    assert_eq!(only_days(&state, limited, alice), [4, 5]);
    assert_eq!(
        state.unscheduled[&node(impossible)],
        UnscheduledReason::NoCapacity
    );
}
// }}}

// groups_cover_their_members_and_start_them_together {{{
#[test]
fn groups_cover_their_members_and_start_them_together() {
    let mut world = World::new();
    let alice = world.user();
    let group = world.group(&[]);
    let parent = Parent::Node(group);
    let wait = world.delay_in(parent, 2, &[]);
    let one = world.task_in(
        parent,
        2,
        &[(user(alice), 8)],
        &[after(PlanNode::PlanDelay(wait))],
    );
    let two = world.task_in(parent, 1, &[(user(alice), 8)], &[after(node(one))]);
    let outside = world.task(1, &[], &[after(PlanNode::TaskGroup(group))]);
    let state = world.solve();
    // members may start a two day delay after the group's own start, which is the Sunday
    assert_eq!(span(&state, one), (d(2), d(4)));
    assert_eq!(span(&state, two), (d(4), d(5)));
    // the group begins with the delay and ends with its last member
    assert_eq!(state.task_groups[&group].date_range.start(), today());
    assert_eq!(state.task_groups[&group].date_range.end(), d(5));
    assert_eq!(span(&state, outside), (d(5), d(6)));
}
// }}}

// empty_groups_last_three_calendar_days_and_groups_of_unscheduled_members_are_unscheduled {{{
#[test]
fn empty_groups_last_three_calendar_days_and_groups_of_unscheduled_members_are_unscheduled() {
    let mut world = World::new();
    let empty = world.group(&[]);
    let after_empty = world.task(1, &[], &[after(PlanNode::TaskGroup(empty))]);
    let hollow = world.group(&[]);
    let nobody = world.tag(&[]);
    world.task_in(Parent::Node(hollow), 1, &[(Worker::Tag(nobody), 8)], &[]);
    let state = world.solve();
    // three calendar days from the Sunday before, and the task after it works that Wednesday
    assert_eq!(state.task_groups[&empty].date_range.end(), d(3));
    assert_eq!(span(&state, after_empty), (d(3), d(4)));
    let reason = state.unscheduled[&PlanNode::TaskGroup(hollow)];
    assert_eq!(reason, UnscheduledReason::NoScheduledMembers);
}
// }}}

// buffers_hold_back_what_follows_and_shrink_for_hard_dates {{{
#[test]
fn buffers_hold_back_what_follows_and_shrink_for_hard_dates() {
    let mut world = World::new();
    let first = world.task(1, &[], &[]);
    let buffer = world.buffer(3, &[after(node(first))]);
    let next = world.task(1, &[], &[after(PlanNode::PlanBuffer(buffer))]);
    let state = world.solve();
    assert_eq!(state.plan_buffers[&buffer].date_range.start(), d(2));
    assert_eq!(state.plan_buffers[&buffer].date_range.end(), d(5));
    assert_eq!(span(&state, next), (d(5), d(6)));

    let pinned = world.task(1, &[], &[after(PlanNode::PlanBuffer(buffer))]);
    world.patch(pinned, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(4)).into())
    });
    let state = world.solve();
    assert_eq!(state.plan_buffers[&buffer].date_range.end(), d(4));
    assert_eq!(span(&state, pinned), (d(4), d(5)));
    assert!(state.constraint_violations.is_empty());
    assert_eq!(span(&state, next), (d(4), d(5)));
}
// }}}

// delays_hold_back_what_follows_by_calendar_days {{{
#[test]
fn delays_hold_back_what_follows_by_calendar_days() {
    let mut world = World::new();
    let first = world.task(1, &[], &[]);
    let delay = world.delay(3, &[after(node(first))]);
    let next = world.task(1, &[], &[after(PlanNode::PlanDelay(delay))]);
    let state = world.solve();
    assert_eq!(state.plan_delays[&delay].date_range.start(), d(2));
    assert_eq!(state.plan_delays[&delay].date_range.end(), d(5));
    assert_eq!(span(&state, next), (d(5), d(6)));

    // a longer wait pushes the task back with it, to the next working day
    world.plan_update(PlanUpdate::PlanDelay(Crud::Update(PlanDelayPatch {
        id: delay,
        name: Some(Some("Cure".into())),
        parent: None,
        dependencies: None,
        duration: Some(5),
    })));
    let state = world.solve();
    assert_eq!(state.plan_delays[&delay].date_range.end(), d(7));
    assert_eq!(span(&state, next), (d(8), d(9)));
}
// }}}

// compaction_fills_room_left_by_other_tasks {{{
#[test]
fn compaction_fills_room_left_by_other_tasks() {
    let mut world = World::new();
    let alice = world.user();
    let blocker = world.task(2, &[(user(alice), 8)], &[]);
    let tail = world.task(1, &[(user(alice), 8)], &[after(node(blocker))]);
    let gapped = world.task(2, &[(user(alice), 8)], &[]);
    let state = world.solve();
    for task in [blocker, tail, gapped] {
        assert!(span(&state, task).1 <= d(8));
    }
    assert_eq!(span(&state, blocker), (d(1), d(3)));
}
// }}}

// build {{{
fn build(world: &mut World) -> PlanVolatileState {
    let people: Vec<Uuid> = (0..3).map(|_| world.user()).collect();
    let tag = world.tag(&people);
    let mut previous = None;
    for i in 0..12u32 {
        let who = if i % 3 == 0 {
            Worker::Tag(tag)
        } else {
            user(people[(i % 3) as usize])
        };
        let on = previous.map_or(vec![], |p| vec![after(node(p))]);
        let task = world.task(1 + i % 3, &[(who, 4 + (i % 2) as u8 * 4)], &on);
        if i % 4 != 3 {
            previous = Some(task);
        }
    }
    world.solve()
}
// }}}

// the_same_updates_give_the_same_schedule {{{
#[test]
fn the_same_updates_give_the_same_schedule() {
    let (mut a, mut b) = (World::new(), World::new());
    // maps differ in order between the two, the schedules must not
    let (first, second) = (build(&mut a), build(&mut b));
    assert_eq!(first, second);
    assert_eq!(first.tasks.len(), 12);
}
// }}}

// a_snapshot_of_a_tenant_schedules_the_same {{{
#[test]
fn a_snapshot_of_a_tenant_schedules_the_same() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    world.weekly_hours(bob, [4, 4, 4, 4, 4, 0, 0]);
    let tag = world.tag(&[alice, bob]);
    let first = world.task(2, &[(Worker::Tag(tag), 4), (user(alice), 4)], &[]);
    world.patch(first, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(9)).into())
    });
    let marker = world.milestone(&[after(node(first))]);
    let buffer = world.buffer(2, &[after(PlanNode::Milestone(marker))]);
    let group = world.group(&[after(PlanNode::PlanBuffer(buffer))]);
    world.task_in(Parent::Node(group), 1, &[(user(bob), 2)], &[]);
    world.target(PlanNode::Milestone(marker));
    let expected = world.solve();

    let bytes = codec::encode(&world.tenant).unwrap();
    let mut loaded: Tenant = codec::decode(&bytes).unwrap();
    assert!(loaded.schedule().is_none(), "derived state is not saved");
    plinko::scheduler::schedule(&mut loaded);
    let state = loaded.schedule().unwrap().plans[&world.plan].clone();
    assert_eq!(state, expected);
    assert!(!state.tasks.is_empty() && !state.task_groups.is_empty());
}
// }}}

// what_hangs_from_a_start_milestone_in_the_future_begins_on_that_day {{{
#[test]
fn what_hangs_from_a_start_milestone_in_the_future_begins_on_that_day() {
    let mut world = World::new();
    let alice = world.user();
    // a plan that starts on a Wednesday, well after today, is a milestone not before then
    let begin = world.milestone(&[]);
    world.plan_update(PlanUpdate::Milestone(Crud::Update(MilestonePatch {
        id: begin,
        name: None,
        parent: None,
        time_constraint: Some(TimeConstraint::Earliest(d(10))),
        dependencies: None,
        persistent_state: None,
    })));
    let first = world.task(1, &[(user(alice), 8)], &[after(PlanNode::Milestone(begin))]);
    let marker = world.milestone(&[after(PlanNode::Milestone(begin))]);
    let state = world.solve();
    assert_eq!(span(&state, first), (d(11), d(12)));
    assert_eq!(state.milestones[&marker].date, d(10));
}
// }}}

// a_task_with_a_latest_date_is_ordered_ahead_of_its_equals_even_when_the_date_is_days_off {{{
#[test]
fn a_task_with_a_latest_date_is_ordered_ahead_of_its_equals_even_when_the_date_is_days_off() {
    let mut world = World::new();
    let alice = world.user();
    let tests: Vec<Uuid> = (0..4)
        .map(|_| world.task(5, &[(user(alice), 8)], &[]))
        .collect();
    // it starts on the 15th as the third of four, but may start no later than the 8th
    world.patch(tests[2], |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(8)).into())
    });
    let state = world.solve();
    assert!(
        state.constraint_violations.is_empty(),
        "{:?}",
        state.constraint_violations
    );
    assert!(span(&state, tests[2]).0 <= d(8));
    // the others still follow each other without gaps
    let mut starts: Vec<NaiveDate> = tests.iter().map(|t| span(&state, *t).0).collect();
    starts.sort();
    assert_eq!(starts, [d(1), d(8), d(15), d(22)]);
}
// }}}

// a_date_the_work_before_it_cannot_meet_is_not_counted_as_avoidable {{{
#[test]
fn a_date_the_work_before_it_cannot_meet_is_not_counted_as_avoidable() {
    let mut world = World::new();
    let alice = world.user();
    let a = world.task(5, &[(user(alice), 8)], &[]);
    let b = world.task(5, &[(user(alice), 8)], &[after(node(a))]);
    let c = world.task(1, &[(user(alice), 8)], &[after(node(b))]);
    world.patch(c, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(5)).into())
    });
    let state = world.solve();
    let violation = state.constraint_violations[&node(c)];
    // two weeks of work come first, so the day it starts is the first it could
    assert_eq!(violation.earliest, Some(violation.scheduled));
    assert_eq!(violation.avoidable(), 0);
}
// }}}

// on_end {{{
/// A constraint on the last day
fn on_end(constraint: TimeConstraint) -> NodeConstraint {
    NodeConstraint {
        constraint,
        end: NodeEnd::End,
    }
}

fn constrain_group(world: &mut World, group: Uuid, constraint: NodeConstraint) {
    world.plan_update(PlanUpdate::TaskGroup(Crud::Update(TaskGroupPatch {
        id: group,
        name: None,
        dependencies: None,
        parent: None,
        time_constraint: Some(constraint),
    })));
}
// }}}

// a_date_can_be_for_the_last_day_of_a_task {{{
#[test]
fn a_date_can_be_for_the_last_day_of_a_task() {
    let mut world = World::new();
    let (alice, bob, carol) = (world.user(), world.user(), world.user());
    let held = world.task(3, &[(user(alice), 8)], &[]);
    world.patch(held, |p| {
        p.time_constraint = Some(on_end(TimeConstraint::Earliest(d(10))))
    });
    let exact = world.task(2, &[(user(bob), 8)], &[]);
    world.patch(exact, |p| {
        p.time_constraint = Some(on_end(TimeConstraint::Fixed(d(5))))
    });
    let late = world.task(3, &[(user(carol), 8)], &[]);
    world.patch(late, |p| {
        p.time_constraint = Some(on_end(TimeConstraint::Latest(d(2))))
    });
    let state = world.solve();
    // it starts as late as it must for its last day to be the 10th, a Wednesday
    assert_eq!(only_days(&state, held, alice), [8, 9, 10]);
    assert_eq!(only_days(&state, exact, bob), [4, 5]);
    // three days from the 1st cannot end by the 2nd, which is said of its last day
    assert_eq!(span(&state, late), (d(1), d(4)));
    assert_eq!(state.constraint_violations.len(), 1);
    let violation = state.constraint_violations[&node(late)];
    assert_eq!((violation.end, violation.scheduled), (NodeEnd::End, d(3)));
    assert_eq!(violation.avoidable(), 0);
}
// }}}

// a_group_can_have_a_date_for_its_start_or_its_end {{{
#[test]
fn a_group_can_have_a_date_for_its_start_or_its_end() {
    let mut world = World::new();
    let alice = world.user();
    let later = world.group(&[]);
    constrain_group(&mut world, later, TimeConstraint::Earliest(d(10)).into());
    let inside = world.task_in(Parent::Node(later), 1, &[(user(alice), 8)], &[]);
    // the day is for the group, not for what else starts with the plan
    let beside = world.task(1, &[(user(alice), 8)], &[]);

    let held = world.group(&[]);
    constrain_group(&mut world, held, on_end(TimeConstraint::Earliest(d(12))));
    let first = world.task_in(Parent::Node(held), 1, &[], &[]);
    let last = world.task_in(Parent::Node(held), 1, &[], &[after(node(first))]);

    let tight = world.group(&[]);
    constrain_group(&mut world, tight, on_end(TimeConstraint::Latest(d(1))));
    let one = world.task_in(Parent::Node(tight), 1, &[], &[]);
    world.task_in(Parent::Node(tight), 1, &[], &[after(node(one))]);

    let state = world.solve();
    assert_eq!(span(&state, inside), (d(10), d(11)));
    assert_eq!(span(&state, beside), (d(1), d(2)));
    // only its last member is held back, so the group really ends on the 12th
    assert_eq!(span(&state, first), (d(1), d(2)));
    assert_eq!(span(&state, last), (d(12), d(13)));
    assert_eq!(state.task_groups[&held].date_range.end(), d(13));
    let violation = state.constraint_violations[&PlanNode::TaskGroup(tight)];
    assert_eq!((violation.end, violation.scheduled), (NodeEnd::End, d(2)));
    assert_eq!(state.constraint_violations.len(), 1);
}
// }}}

// set_buffers_keep_their_end_and_shrink_to_nothing_but_never_below {{{
#[test]
fn set_buffers_keep_their_end_and_shrink_to_nothing_but_never_below() {
    let mut world = World::new();
    let first = world.task(1, &[], &[]);
    let buffer = world.buffer(5, &[after(node(first))]);
    // set on Monday the 1st, so it ends after five working days whenever it starts
    world.plan_update(PlanUpdate::PlanBuffer(Crud::Update(PlanBufferPatch {
        id: buffer,
        parent: None,
        dependencies: None,
        set_start_day: Some(Some(d(1))),
        duration: None,
    })));
    let next = world.task(1, &[], &[after(PlanNode::PlanBuffer(buffer))]);
    let state = world.solve();
    let range = state.plan_buffers[&buffer].date_range;
    assert_eq!((range.start(), range.end()), (d(2), d(6)));
    assert_eq!(span(&state, next), (d(8), d(9)));

    // the work before it overruns the buffer, which is then nothing and holds nothing back
    world.patch(first, |p| p.duration = Some(8));
    let state = world.solve();
    let range = state.plan_buffers[&buffer].date_range;
    assert_eq!((range.start(), range.end()), (d(11), d(11)));
    assert_eq!(span(&state, next), (d(11), d(12)));
}
// }}}
