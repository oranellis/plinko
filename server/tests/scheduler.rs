use std::collections::HashMap;

use chrono::{DateTime, NaiveDate};
use plinko::{
    availability::{
        rule::{NewRule, RuleKind, WeeklySchedule},
        rule_set::RuleSetUpdate,
    },
    codec,
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::{
        plan::{NewPlan, PlanSchedulerDataPatch},
        plan_update::PlanUpdate,
    },
    scheduler::{
        Refreshed, Tracker, prepare, solve_tenant,
        state::{Baseline, PlanVolatileState, UnscheduledReason},
    },
    types::{
        affinity::NewAffinity,
        common::TimeConstraint,
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::{DerivedStatus, MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        short_id::ShortId,
        tag::NewTag,
        target::NewTarget,
        task::{Assignee, NewTask, PersistentState, ScheduleMode, Status, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, PlanUserDataPatch},
        workload::Workload,
    },
};
use uuid::Uuid;

/// Dates in January 2024, which starts on a Monday
fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
}

/// Today is the Sunday before, so the first day a task may start is Monday the 1st
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()
}

fn h(hours: u8) -> Workload {
    Workload::new(hours).unwrap()
}

type Dep = (DependencyNode, i32);

fn start() -> Dep {
    (DependencyNode::ParentStart, 0)
}

fn after(node: PlanNode) -> Dep {
    (DependencyNode::Node(node), 0)
}

fn lagged(node: PlanNode, lag: i32) -> Dep {
    (DependencyNode::Node(node), lag)
}

fn deps(deps: &[Dep]) -> Vec<Dependency> {
    deps.iter()
        .map(|&(node, lag)| Dependency { node, lag })
        .collect()
}

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
            start_date: NaiveDate::from_ymd_opt(2023, 12, 1).unwrap(),
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
        self.task_in(Parent::PlanStart, duration, who, on)
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
            mode: None,
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
        let id = self.id();
        let new = NewMilestone {
            id,
            name: "Milestone".to_string(),
            parent: Parent::PlanStart,
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
            parent: Parent::PlanStart,
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

    fn group(&mut self, on: &[Dep]) -> Uuid {
        let id = self.id();
        let new = NewTaskGroup {
            id,
            name: "Group".to_string(),
            parent: Parent::PlanStart,
        };
        self.plan_update(PlanUpdate::TaskGroup(Crud::Add(new)));
        let patch = TaskGroupPatch {
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

    /// Applies a plan update through a tracker, which notes what it may change
    fn tracked(&mut self, tracker: &mut Tracker, update: PlanUpdate) {
        let update = TenantUpdate {
            time: DateTime::from_timestamp(978307200, 0).unwrap(),
            actor: self.root,
            update_type: TenantUpdateType::Plan(PlanTenantUpdate::Update(
                self.plan,
                Box::new(update),
            )),
        };
        tracker.apply(&mut self.tenant, update).unwrap();
    }

    fn stored(&self) -> PlanVolatileState {
        let schedule = self.tenant.schedule().expect("scheduled");
        schedule.plans[&self.plan].clone()
    }

    fn baseline(&mut self) -> Baseline {
        prepare(&mut self.tenant);
        self.tenant.baseline(&self.plan).expect("prepared").clone()
    }

    fn solve(&mut self) -> PlanVolatileState {
        prepare(&mut self.tenant);
        let mut state = solve_tenant(&self.tenant);
        state.plans.remove(&self.plan).expect("the plan is solved")
    }
}

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

#[test]
fn a_task_works_its_duration_in_days() {
    let mut world = World::new();
    let alice = world.user();
    let task = world.task(3, &[(user(alice), 8)], &[start()]);
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

#[test]
fn tasks_with_no_workers_last_their_duration_in_plan_working_days() {
    let mut world = World::new();
    let task = world.task(6, &[], &[start()]);
    let state = world.solve();
    // Monday to the next Monday, skipping the weekend
    assert_eq!(span(&state, task), (d(1), d(9)));
    assert!(state.tasks[&task].assignments.is_empty());
}

#[test]
fn two_strict_tasks_share_a_person_who_has_the_hours() {
    let mut world = World::new();
    let alice = world.user();
    let a = world.task(1, &[(user(alice), 4)], &[start()]);
    let b = world.task(1, &[(user(alice), 4)], &[start()]);
    let state = world.solve();
    assert_eq!(span(&state, a), (d(1), d(2)));
    assert_eq!(span(&state, b), (d(1), d(2)));
}

#[test]
fn strict_workers_stay_on_the_same_days() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let task = world.task(4, &[(user(alice), 4), (user(bob), 8)], &[start()]);
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

#[test]
fn strict_runs_restart_on_a_busy_day_but_not_across_weekends() {
    let mut world = World::new();
    let alice = world.user();
    let holder = world.task(1, &[(user(alice), 4)], &[start()]);
    world.patch(holder, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(3)))
    });
    let run = world.task(3, &[(user(alice), 8)], &[start()]);
    let long = world.task(6, &[], &[start()]);
    let state = world.solve();
    assert_eq!(span(&state, holder), (d(3), d(4)));
    // Wednesday is half taken, so the run starts over on Thursday, across the weekend
    assert_eq!(only_days(&state, run, alice), [4, 5, 8]);
    assert_eq!(span(&state, run), (d(4), d(9)));
    assert_eq!(span(&state, long), (d(1), d(9)));
    assert!(state.constraint_violations.is_empty());
}

#[test]
fn relaxed_tasks_use_what_is_free() {
    let mut world = World::new();
    let alice = world.user();
    let holder = world.task(1, &[(user(alice), 4)], &[start()]);
    world.patch(holder, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(3)))
    });
    let task = world.task(3, &[(user(alice), 8)], &[start()]);
    world.patch(task, |p| p.mode = Some(ScheduleMode::Relaxed));
    let state = world.solve();
    assert_eq!(
        days_of(&state, task, alice),
        [(d(1), 8), (d(2), 8), (d(3), 4), (d(4), 4)]
    );
    assert_eq!(span(&state, task), (d(1), d(5)));
}

#[test]
fn work_in_progress_claims_hours_before_longer_chains() {
    let mut world = World::new();
    let alice = world.user();
    let active = world.task(5, &[(user(alice), 8)], &[start()]);
    let first = world.task(1, &[(user(alice), 8)], &[start()]);
    let long = world.task(5, &[(user(alice), 8)], &[after(node(first))]);
    world.set_state(active, Status::InProgress, Some(1), None);
    let state = world.solve();
    assert_eq!(span(&state, active), (d(1), d(6)));
    assert_eq!(only_days(&state, active, alice), [1, 2, 3, 4, 5]);
    // the week is taken, so the longer chain waits for Monday
    assert_eq!(span(&state, first), (d(8), d(9)));
    assert_eq!(span(&state, long).0, d(9));
}

#[test]
fn work_in_progress_runs_up_to_today_even_past_its_duration() {
    let mut world = World::new();
    let alice = world.user();
    let task = world.task(1, &[(user(alice), 8)], &[start()]);
    world.set_state(task, Status::InProgress, Some(1), None);
    world.apply(TenantUpdateType::SetToday(d(5)));
    let state = world.solve();
    assert_eq!(only_days(&state, task, alice), [1, 2, 3, 4, 5]);
    assert_eq!(span(&state, task), (d(1), d(6)));
}

#[test]
fn completed_work_is_inferred_from_its_dates() {
    let mut world = World::new();
    let alice = world.user();
    let done = world.task(3, &[(user(alice), 8)], &[start()]);
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
    let days: Vec<_> = days_of(&state, done, alice);
    assert_eq!(days, [(day(18), 8), (day(19), 8), (day(20), 8)]);
    // history takes no hours from the future
    assert_eq!(span(&state, next), (d(1), d(2)));
}

#[test]
fn milestones_land_on_the_last_day_of_the_work_before_them() {
    let mut world = World::new();
    let alice = world.user();
    let first = world.task(3, &[(user(alice), 8)], &[start()]);
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

#[test]
fn milestones_stay_on_a_day_worked_outside_the_usual_week() {
    let mut world = World::new();
    let alice = world.user();
    world.weekly_hours(alice, [8, 8, 8, 8, 8, 8, 0]);
    let first = world.task(6, &[(user(alice), 8)], &[start()]);
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

#[test]
fn milestone_statuses_follow_what_they_depend_on() {
    let mut world = World::new();
    let alice = world.user();
    let done = world.task(1, &[(user(alice), 8)], &[start()]);
    let busy = world.task(1, &[(user(alice), 8)], &[start()]);
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

#[test]
fn dropped_tasks_are_transparent_and_on_hold_tasks_end_when_held() {
    let mut world = World::new();
    let alice = world.user();
    let first = world.task(2, &[(user(alice), 8)], &[start()]);
    let dropped = world.task(5, &[(user(alice), 8)], &[after(node(first))]);
    let last = world.task(1, &[], &[after(node(dropped))]);
    world.set_state(dropped, Status::Dropped, None, None);
    let bob = world.user();
    let held = world.task(3, &[(user(bob), 8)], &[start()]);
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

#[test]
fn negative_lag_starts_before_the_end_of_a_dependency() {
    let mut world = World::new();
    let alice = world.user();
    let first = world.task(3, &[(user(alice), 8)], &[start()]);
    let early = world.task(1, &[], &[lagged(node(first), -2)]);
    let state = world.solve();
    assert_eq!(span(&state, early), (d(2), d(3)));
}

#[test]
fn time_constraints_hold_or_are_reported_violated() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    world.weekly_hours(bob, [8, 8, 8, 8, 8, 0, 0]);
    let fixed = world.task(1, &[(user(alice), 8)], &[start()]);
    world.patch(fixed, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(10)))
    });
    let first = world.task(3, &[(user(alice), 8)], &[start()]);
    let late = world.task(1, &[], &[after(node(first))]);
    world.patch(late, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(2)))
    });
    // a Saturday that nobody works
    let weekend = world.task(1, &[(user(bob), 8)], &[start()]);
    world.patch(weekend, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(6)))
    });
    let early = world.task(1, &[], &[start()]);
    world.patch(early, |p| {
        p.time_constraint = Some(TimeConstraint::Earliest(d(3)))
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

#[test]
fn hard_dated_chains_go_before_target_chains_before_the_rest() {
    let mut world = World::new();
    let alice = world.user();
    let hard = world.task(1, &[(user(alice), 8)], &[start()]);
    world.patch(hard, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(3)))
    });
    let chain = world.task(3, &[(user(alice), 8)], &[start()]);
    let goal = world.milestone(&[after(node(chain))]);
    world.target(PlanNode::Milestone(goal));
    let rest = world.task(1, &[(user(alice), 8)], &[start()]);
    let state = world.solve();
    assert_eq!(span(&state, hard), (d(3), d(4)));
    assert!(state.constraint_violations.is_empty());
    // the target chain works around the dated task, the rest takes what is left
    assert_eq!(only_days(&state, chain, alice), [4, 5, 8]);
    assert_eq!(state.milestones[&goal].date, d(8));
    assert_eq!(span(&state, rest), (d(1), d(2)));
    assert_eq!(state.targets.len(), 1);
}

#[test]
fn tasks_off_the_target_chain_never_delay_it() {
    let mut world = World::new();
    let alice = world.user();
    let rest = world.task(5, &[(user(alice), 8)], &[start()]);
    let chain = world.task(2, &[(user(alice), 8)], &[start()]);
    let goal = world.milestone(&[after(node(chain))]);
    let target = world.target(PlanNode::Milestone(goal));
    let state = world.solve();
    assert_eq!(span(&state, chain), (d(1), d(3)));
    assert_eq!(state.targets[&target].forecast, d(2));
    assert_eq!(span(&state, rest), (d(3), d(10)));
}

#[test]
fn tags_and_groups_pick_whoever_finishes_first() {
    let mut world = World::new();
    let (alice, bob, carol) = (world.user(), world.user(), world.user());
    let tag = world.tag(&[alice, bob]);
    let busy = world.task(2, &[(user(alice), 8)], &[start()]);
    world.patch(busy, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(1)))
    });
    let by_tag = world.task(1, &[(Worker::Tag(tag), 8)], &[start()]);
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
        plinko::types::user::NewPlanUserGroup { id: group },
    )));
    let by_group = world.task(1, &[(Worker::UserGroup(group), 8)], &[start()]);
    let state = world.solve();
    let picked = state.tasks[&by_group].workers[&Worker::UserGroup(group)];
    assert_eq!(picked, Assignee::User(carol));
}

#[test]
fn recorded_workers_replace_the_named_user() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let task = world.task(1, &[(user(alice), 8)], &[start()]);
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

#[test]
fn what_cannot_be_scheduled_says_why() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let nobody = world.tag(&[]);
    // bob never works
    world.weekly_hours(bob, [0; 7]);
    let floating = world.task(1, &[], &[]);
    let no_one = world.task(1, &[(Worker::Tag(nobody), 8)], &[start()]);
    let no_time = world.task(1, &[(user(bob), 8)], &[start()]);
    let behind = world.task(1, &[(user(alice), 8)], &[after(node(no_one))]);
    let fine = world.task(1, &[(user(alice), 8)], &[start(), after(node(no_one))]);
    let state = world.solve();
    let reason = |task| state.unscheduled[&node(task)];
    assert_eq!(reason(floating), UnscheduledReason::NoPathToPlanStart);
    assert_eq!(reason(no_one), UnscheduledReason::NoEligibleWorkers);
    assert_eq!(reason(no_time), UnscheduledReason::NoCapacity);
    assert_eq!(reason(behind), UnscheduledReason::DependsOnUnscheduled);
    // another path to the start is enough
    assert_eq!(span(&state, fine), (d(1), d(2)));
}

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
    let limited = world.task(2, &[(user(alice), 8)], &[start()]);
    world.patch(limited, |p| p.affinities = Some([late_week].into()));
    let impossible = world.task(1, &[(user(alice), 8)], &[start()]);
    world.patch(impossible, |p| p.affinities = Some([never].into()));
    let state = world.solve();
    assert_eq!(only_days(&state, limited, alice), [4, 5]);
    assert_eq!(
        state.unscheduled[&node(impossible)],
        UnscheduledReason::NoCapacity
    );
}

#[test]
fn groups_cover_their_members_and_start_them_together() {
    let mut world = World::new();
    let alice = world.user();
    let group = world.group(&[start()]);
    let parent = Parent::Node(group);
    let one = world.task_in(
        parent,
        2,
        &[(user(alice), 8)],
        &[(DependencyNode::ParentStart, 2)],
    );
    let two = world.task_in(parent, 1, &[(user(alice), 8)], &[after(node(one))]);
    let outside = world.task(1, &[], &[after(PlanNode::TaskGroup(group))]);
    let state = world.solve();
    // members may start two days after the group's own start, which is the Sunday
    assert_eq!(span(&state, one), (d(2), d(4)));
    assert_eq!(span(&state, two), (d(4), d(5)));
    assert_eq!(state.task_groups[&group].date_range.start(), d(2));
    assert_eq!(state.task_groups[&group].date_range.end(), d(5));
    assert_eq!(span(&state, outside), (d(5), d(6)));
}

#[test]
fn empty_groups_last_three_calendar_days_and_groups_of_unscheduled_members_are_unscheduled() {
    let mut world = World::new();
    let empty = world.group(&[start()]);
    let after_empty = world.task(1, &[], &[after(PlanNode::TaskGroup(empty))]);
    let hollow = world.group(&[start()]);
    let nobody = world.tag(&[]);
    world.task_in(
        Parent::Node(hollow),
        1,
        &[(Worker::Tag(nobody), 8)],
        &[start()],
    );
    let state = world.solve();
    // three calendar days from the Sunday before, and the task after it works that Wednesday
    assert_eq!(state.task_groups[&empty].date_range.end(), d(3));
    assert_eq!(span(&state, after_empty), (d(3), d(4)));
    let reason = state.unscheduled[&PlanNode::TaskGroup(hollow)];
    assert_eq!(reason, UnscheduledReason::NoScheduledMembers);
}

#[test]
fn buffers_hold_back_what_follows_and_shrink_for_hard_dates() {
    let mut world = World::new();
    let first = world.task(1, &[], &[start()]);
    let buffer = world.buffer(3, &[after(node(first))]);
    let next = world.task(1, &[], &[after(PlanNode::PlanBuffer(buffer))]);
    let state = world.solve();
    assert_eq!(state.plan_buffers[&buffer].date_range.start(), d(2));
    assert_eq!(state.plan_buffers[&buffer].date_range.end(), d(5));
    assert_eq!(span(&state, next), (d(5), d(6)));

    let pinned = world.task(1, &[], &[after(PlanNode::PlanBuffer(buffer))]);
    world.patch(pinned, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(d(4)))
    });
    let state = world.solve();
    assert_eq!(state.plan_buffers[&buffer].date_range.end(), d(4));
    assert_eq!(span(&state, pinned), (d(4), d(5)));
    assert!(state.constraint_violations.is_empty());
    assert_eq!(span(&state, next), (d(4), d(5)));
}

#[test]
fn compaction_fills_room_left_by_other_tasks() {
    let mut world = World::new();
    let alice = world.user();
    let blocker = world.task(2, &[(user(alice), 8)], &[start()]);
    let tail = world.task(1, &[(user(alice), 8)], &[after(node(blocker))]);
    let gapped = world.task(2, &[(user(alice), 8)], &[start()]);
    let state = world.solve();
    for task in [blocker, tail, gapped] {
        assert!(span(&state, task).1 <= d(8));
    }
    assert_eq!(span(&state, blocker), (d(1), d(3)));
}

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
        let on = previous.map_or(vec![start()], |p| vec![after(node(p))]);
        let task = world.task(1 + i % 3, &[(who, 4 + (i % 2) as u8 * 4)], &on);
        if i % 4 != 3 {
            previous = Some(task);
        }
    }
    world.solve()
}

#[test]
fn the_same_updates_give_the_same_schedule() {
    let (mut a, mut b) = (World::new(), World::new());
    // maps differ in order between the two, the schedules must not
    let (first, second) = (build(&mut a), build(&mut b));
    assert_eq!(first, second);
    assert_eq!(first.tasks.len(), 12);
}

#[test]
fn slack_is_how_much_later_a_node_can_finish_before_the_plan_does() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    let first = world.task(2, &[(user(alice), 8)], &[start()]);
    let second = world.task(2, &[(user(alice), 8)], &[after(node(first))]);
    let aside = world.task(1, &[(user(bob), 8)], &[start()]);
    let slack = world.baseline().slack;
    assert_eq!(slack[&node(first)], 0);
    assert_eq!(slack[&node(second)], 0);
    // it ends on the 2nd and the plan on the 5th
    assert_eq!(slack[&node(aside)], 3);
}

#[test]
fn slack_counts_the_order_people_work_in() {
    let mut world = World::new();
    let (alice, bob, carol) = (world.user(), world.user(), world.user());
    let one = world.task(1, &[(user(alice), 8)], &[start()]);
    let two = world.task(1, &[(user(alice), 8)], &[start()]);
    let apart = world.task(1, &[(user(carol), 8)], &[start()]);
    world.task(2, &[(user(bob), 8)], &[start()]);
    let state = world.solve();
    let slack = world.baseline().slack;
    let (first, second) = if span(&state, one).0 < span(&state, two).0 {
        (one, two)
    } else {
        (two, one)
    };
    // the first has to be done for the second to be on time, the one on her own has a day
    assert_eq!(slack[&node(first)], 0);
    assert_eq!(slack[&node(second)], 0);
    assert_eq!(slack[&node(apart)], 1);
}

#[test]
fn slack_follows_targets_and_hard_dates() {
    let mut world = World::new();
    let chain = world.task(1, &[], &[start()]);
    let goal = world.milestone(&[after(node(chain))]);
    world.target(PlanNode::Milestone(goal));
    let dated = world.task(1, &[], &[start()]);
    world.patch(dated, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(10)))
    });
    let rest = world.task(1, &[], &[start()]);
    let slack = world.baseline().slack;
    assert_eq!(slack[&node(chain)], 0);
    assert_eq!(slack[&PlanNode::Milestone(goal)], 0);
    // it may start on the 10th, so end on the 11th, having ended on the 2nd
    assert_eq!(slack[&node(dated)], 9);
    assert!(!slack.contains_key(&node(rest)));
}

#[test]
fn slack_is_negative_once_a_hard_date_is_missed() {
    let mut world = World::new();
    let first = world.task(3, &[], &[start()]);
    let late = world.task(1, &[], &[after(node(first))]);
    world.patch(late, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(2)))
    });
    let slack = world.baseline().slack;
    // it starts on the 4th and ends on the 5th, but could end on the 3rd
    assert_eq!(slack[&node(late)], -2);
    assert_eq!(slack[&node(first)], -2);
}

#[test]
fn a_snapshot_of_a_tenant_schedules_the_same() {
    let mut world = World::new();
    let (alice, bob) = (world.user(), world.user());
    world.weekly_hours(bob, [4, 4, 4, 4, 4, 0, 0]);
    let tag = world.tag(&[alice, bob]);
    let first = world.task(2, &[(Worker::Tag(tag), 4), (user(alice), 4)], &[start()]);
    world.patch(first, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(9)))
    });
    let marker = world.milestone(&[after(node(first))]);
    let buffer = world.buffer(2, &[after(PlanNode::Milestone(marker))]);
    let group = world.group(&[after(PlanNode::PlanBuffer(buffer))]);
    world.task_in(Parent::Node(group), 1, &[(user(bob), 2)], &[start()]);
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

#[test]
fn what_hangs_from_a_plan_start_in_the_future_begins_on_that_day_and_milestones_land_on_it() {
    let mut world = World::new();
    let alice = world.user();
    // the plan starts on a Wednesday, well after today
    let patch = PlanSchedulerDataPatch {
        start_date: Some(d(10)),
        priority: None,
    };
    world.plan_update(PlanUpdate::SchedulerData(patch));
    let first = world.task(1, &[(user(alice), 8)], &[start()]);
    let marker = world.milestone(&[start()]);
    let state = world.solve();
    assert_eq!(span(&state, first), (d(10), d(11)));
    assert_eq!(state.milestones[&marker].date, d(10));
}

#[test]
fn a_task_with_a_latest_date_is_ordered_ahead_of_its_equals_even_when_the_date_is_days_off() {
    let mut world = World::new();
    let alice = world.user();
    let tests: Vec<Uuid> = (0..4)
        .map(|_| world.task(5, &[(user(alice), 8)], &[start()]))
        .collect();
    // it starts on the 15th as the third of four, but may start no later than the 8th
    world.patch(tests[2], |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(8)))
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

/// What the tracker gives back for the tenant, which has a full schedule to start from
fn scheduled_world() -> (World, Uuid, [Uuid; 3], Uuid) {
    let mut world = World::new();
    let alice = world.user();
    let bob = world.user();
    let a = world.task(2, &[(user(alice), 8)], &[start()]);
    let b = world.task(2, &[(user(alice), 8)], &[after(node(a))]);
    let c = world.task(1, &[(user(alice), 8)], &[after(node(b))]);
    let apart = world.task(3, &[(user(bob), 8)], &[start()]);
    plinko::scheduler::schedule(&mut world.tenant);
    (world, alice, [a, b, c], apart)
}

#[test]
fn a_change_places_what_depends_on_it_again_and_leaves_the_rest() {
    let (mut world, _, [a, b, c], apart) = scheduled_world();
    let before = world.stored();
    let mut tracker = Tracker::default();
    world.tracked(
        &mut tracker,
        PlanUpdate::Task(Crud::Update(TaskPatch {
            id: a,
            name: None,
            parent: None,
            mode: None,
            duration: Some(4),
            time_constraint: None,
            dependencies: None,
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        })),
    );
    assert_eq!(tracker.schedule(&mut world.tenant), Refreshed::Partly);
    let after = world.stored();
    // the task that does not depend on it did not move and the chain follows
    assert_eq!(after.tasks[&apart], before.tasks[&apart]);
    assert_eq!(span(&after, a), (d(1), d(5)));
    assert_eq!(span(&after, b), (d(5), d(9)));
    assert_eq!(span(&after, c), (d(9), d(10)));
    // as a full run would
    let mut full = World::new();
    std::mem::swap(&mut full.tenant, &mut world.tenant);
    plinko::scheduler::schedule(&mut full.tenant);
    full.plan = world.plan;
    for task in [a, b, c, apart] {
        assert_eq!(
            full.stored().tasks[&task].date_range,
            after.tasks[&task].date_range
        );
    }
}

#[test]
fn updates_that_change_nothing_scheduled_leave_the_schedule_alone() {
    let (mut world, _, [a, ..], _) = scheduled_world();
    let before = world.stored();
    let mut tracker = Tracker::default();
    world.tracked(
        &mut tracker,
        PlanUpdate::Task(Crud::Update(TaskPatch {
            id: a,
            name: Some("Renamed".to_string()),
            parent: None,
            mode: None,
            duration: None,
            time_constraint: None,
            dependencies: None,
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        })),
    );
    world.tracked(
        &mut tracker,
        PlanUpdate::Describe(node(a), Some("Words".to_string())),
    );
    assert_eq!(tracker.schedule(&mut world.tenant), Refreshed::Unchanged);
    assert_eq!(world.stored(), before);
}

#[test]
fn a_new_task_is_placed_around_what_is_there() {
    let (mut world, alice, [a, b, c], apart) = scheduled_world();
    let before = world.stored();
    let mut tracker = Tracker::default();
    let id = world.id();
    let new = NewTask {
        id,
        name: "New".to_string(),
        parent: Parent::PlanStart,
    };
    world.tracked(&mut tracker, PlanUpdate::Task(Crud::Add(new)));
    world.tracked(
        &mut tracker,
        PlanUpdate::Task(Crud::Update(TaskPatch {
            id,
            name: None,
            parent: None,
            mode: None,
            duration: Some(2),
            time_constraint: None,
            dependencies: Some(deps(&[start()])),
            allocations: Some(HashMap::from([(user(alice), h(8))])),
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        })),
    );
    assert_eq!(tracker.schedule(&mut world.tenant), Refreshed::Partly);
    let after = world.stored();
    for task in [a, b, c, apart] {
        assert_eq!(after.tasks[&task], before.tasks[&task]);
    }
    // alice is busy through Friday the 5th with the chain, so it goes after the weekend
    assert_eq!(span(&after, id), (d(8), d(10)));
}

#[test]
fn changes_to_people_or_the_clock_schedule_everything_again() {
    let (mut world, alice, ..) = scheduled_world();
    let mut tracker = Tracker::default();
    let update = TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).unwrap(),
        actor: world.root,
        update_type: TenantUpdateType::Availability(
            alice,
            RuleSetUpdate::Rule(Crud::Add(NewRule {
                name: String::new(),
                stop_day: None,
                id: Uuid::now_v7(),
                start_day: NaiveDate::from_ymd_opt(2000, 1, 3).unwrap(),
                kind: RuleKind::Weekly(WeeklySchedule::new([h(4); 7])),
            })),
        ),
    };
    tracker.apply(&mut world.tenant, update).unwrap();
    assert_eq!(tracker.schedule(&mut world.tenant), Refreshed::Fully);
}

#[test]
fn a_date_the_work_before_it_cannot_meet_is_not_counted_as_avoidable() {
    let mut world = World::new();
    let alice = world.user();
    let a = world.task(5, &[(user(alice), 8)], &[start()]);
    let b = world.task(5, &[(user(alice), 8)], &[after(node(a))]);
    let c = world.task(1, &[(user(alice), 8)], &[after(node(b))]);
    world.patch(c, |p| {
        p.time_constraint = Some(TimeConstraint::Latest(d(5)))
    });
    let state = world.solve();
    let violation = state.constraint_violations[&node(c)];
    // two weeks of work come first, so the day it starts is the first it could
    assert_eq!(violation.earliest, Some(violation.scheduled));
    assert_eq!(violation.avoidable(), 0);
}
