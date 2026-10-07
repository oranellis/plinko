use chrono::{DateTime, NaiveDate};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::{
        plan::{NewPlan, PlanSchedulerDataPatch},
        plan_update::PlanUpdate,
    },
    scheduler::{prepare, solve_tenant, state::TenantVolatileState},
    types::{
        common::TimeConstraint,
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::NewMilestone,
        parent::Parent,
        short_id::ShortId,
        target::NewTarget,
        task::{NewTask, PersistentState, Status, TaskPatch, Worker},
        user::NewPlanUser,
        workload::Workload,
    },
};
use uuid::Uuid;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

struct Plans {
    tenant: Tenant,
    root: Uuid,
    shared: Uuid,
}

impl Plans {
    fn new() -> Self {
        let tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), day(2023, 12, 31));
        let root = *tenant.users().keys().next().unwrap();
        let mut plans = Self {
            tenant,
            root,
            shared: Uuid::now_v7(),
        };
        let new = NewTenantLoginUser {
            id: plans.shared,
            name: "Shared".to_string(),
            email: "shared@example.com".to_string(),
            role: Role::Member,
        };
        plans.apply(TenantUpdateType::User(Crud::Add(new)));
        plans
    }

    fn apply(&mut self, update_type: TenantUpdateType) {
        let update = TenantUpdate {
            time: DateTime::from_timestamp(978307200, 0).unwrap(),
            actor: self.root,
            update_type,
        };
        self.tenant.apply_update(update).unwrap();
    }

    fn update(&mut self, plan: ShortId, update: PlanUpdate) {
        let update = PlanTenantUpdate::Update(plan, Box::new(update));
        self.apply(TenantUpdateType::Plan(update));
    }

    /// A plan starting on `start` with no tasks
    fn empty(&mut self, start: NaiveDate) -> ShortId {
        let plan = ShortId::new();
        let new = NewPlan {
            id: plan,
            name: "Plan".to_string(),
            start_date: start,
        };
        self.apply(TenantUpdateType::Plan(PlanTenantUpdate::Add(new)));
        let new = NewPlanUser { id: self.shared };
        self.update(plan, PlanUpdate::User(Crud::Add(new)));
        plan
    }

    /// A task of `days` days for the shared person, after the tasks and milestones in `after`
    fn task(&mut self, plan: ShortId, days: u32, after: &[PlanNode]) -> Uuid {
        let task = Uuid::now_v7();
        let new = NewTask {
            id: task,
            name: "Task".to_string(),
            parent: Parent::PlanStart,
        };
        self.update(plan, PlanUpdate::Task(Crud::Add(new)));
        let after: Vec<Dependency> = match after {
            [] => vec![DependencyNode::ParentStart],
            nodes => nodes.iter().map(|n| DependencyNode::Node(*n)).collect(),
        }
        .into_iter()
        .map(|node| Dependency { node, lag: 0 })
        .collect();
        let who = [(Worker::User(self.shared), Workload::new(8).unwrap())];
        self.patch(plan, task, |p| {
            p.duration = Some(days);
            p.dependencies = Some(after);
            p.allocations = Some(who.into());
        });
        task
    }

    fn patch(&mut self, plan: ShortId, task: Uuid, change: impl FnOnce(&mut TaskPatch)) {
        let mut patch = TaskPatch {
            id: task,
            name: None,
            parent: None,
            mode: None,
            duration: None,
            time_constraint: None,
            dependencies: None,
            allocations: None,
            affinities: None,
            actual_workers: None,
            description: None,
            persistent_state: None,
        };
        change(&mut patch);
        self.update(plan, PlanUpdate::Task(Crud::Update(patch)));
    }

    fn milestone(&mut self, plan: ShortId, after: PlanNode) -> PlanNode {
        let id = Uuid::now_v7();
        let new = NewMilestone {
            id,
            name: "Milestone".to_string(),
            parent: Parent::PlanStart,
        };
        self.update(plan, PlanUpdate::Milestone(Crud::Add(new)));
        let patch = plinko::types::milestone::MilestonePatch {
            id,
            name: None,
            parent: None,
            time_constraint: None,
            dependencies: Some(vec![Dependency {
                node: DependencyNode::Node(after),
                lag: 0,
            }]),
            persistent_state: None,
        };
        self.update(plan, PlanUpdate::Milestone(Crud::Update(patch)));
        PlanNode::Milestone(id)
    }

    fn target(&mut self, plan: ShortId, node: PlanNode) {
        let new = NewTarget {
            id: Uuid::now_v7(),
            node,
            priority: 0,
        };
        self.update(plan, PlanUpdate::Target(Crud::Add(new)));
    }

    fn priority(&mut self, plan: ShortId, priority: i32) {
        let patch = PlanSchedulerDataPatch {
            start_date: None,
            priority: Some(priority),
        };
        self.update(plan, PlanUpdate::SchedulerData(patch));
    }

    /// A plan starting on `start` with one task of `days` days for the shared person
    fn plan(&mut self, start: NaiveDate, days: u32) -> (ShortId, Uuid) {
        let plan = self.empty(start);
        (plan, self.task(plan, days, &[]))
    }

    fn solve(&mut self) -> TenantVolatileState {
        prepare(&mut self.tenant);
        solve_tenant(&self.tenant)
    }
}

/// The first day of the task
fn first_day(state: &TenantVolatileState, plan: ShortId, task: Uuid) -> NaiveDate {
    state.plans[&plan].tasks[&task].date_range.start()
}

#[test]
fn the_plan_that_ends_last_alone_goes_first() {
    let mut plans = Plans::new();
    let start = day(2023, 12, 1);
    let (short, short_task) = plans.plan(start, 1);
    let (long, long_task) = plans.plan(start, 3);
    let state = plans.solve();
    assert_eq!(first_day(&state, long, long_task), day(2024, 1, 1));
    assert_eq!(first_day(&state, short, short_task), day(2024, 1, 4));
    assert_eq!(state.users[&plans.shared].booked_on(day(2024, 1, 4)), 8);
}

#[test]
fn equal_ends_go_by_the_earlier_start_date_then_id() {
    let mut plans = Plans::new();
    let (later, later_task) = plans.plan(day(2023, 12, 15), 1);
    let (earlier, earlier_task) = plans.plan(day(2023, 12, 1), 1);
    let state = plans.solve();
    assert_eq!(first_day(&state, earlier, earlier_task), day(2024, 1, 1));
    assert_eq!(first_day(&state, later, later_task), day(2024, 1, 2));

    let mut plans = Plans::new();
    let start = day(2023, 12, 1);
    let ids = [plans.plan(start, 1), plans.plan(start, 1)];
    let state = plans.solve();
    let (low, high) = if ids[0].0 < ids[1].0 {
        (ids[0], ids[1])
    } else {
        (ids[1], ids[0])
    };
    assert_eq!(first_day(&state, low.0, low.1), day(2024, 1, 1));
    assert_eq!(first_day(&state, high.0, high.1), day(2024, 1, 2));
}

#[test]
fn forecasts_are_compared_with_the_plan_on_its_own() {
    let mut plans = Plans::new();
    let start = day(2023, 12, 1);
    let (short, _) = plans.plan(start, 1);
    let (long, _) = plans.plan(start, 3);
    let state = plans.solve();
    let delay = |plan| state.plans[&plan].plan_end.unwrap().delay();
    assert_eq!(delay(long), 0);
    // it waits for the long plan's three days
    assert_eq!(delay(short), 3);
    assert_eq!(
        state.plans[&short].plan_end.unwrap().baseline,
        day(2024, 1, 1)
    );
}

#[test]
fn baselines_are_cached_and_dropped_when_what_they_depend_on_changes() {
    let mut plans = Plans::new();
    let start = day(2023, 12, 1);
    let ((one, _), (two, _)) = (plans.plan(start, 1), plans.plan(start, 1));
    assert!(plans.tenant.baseline(&one).is_none());
    plans.solve();
    assert!(plans.tenant.baseline(&one).is_some());
    assert!(plans.tenant.baseline(&two).is_some());

    // changing a plan only drops its own
    let new = NewTask {
        id: Uuid::now_v7(),
        name: "More".to_string(),
        parent: Parent::PlanStart,
    };
    plans.update(one, PlanUpdate::Task(Crud::Add(new)));
    assert!(plans.tenant.baseline(&one).is_none());
    assert!(plans.tenant.baseline(&two).is_some());

    // the clock, or anything about people, drops them all
    plans.solve();
    plans.apply(TenantUpdateType::SetToday(day(2024, 1, 2)));
    assert!(plans.tenant.baseline(&one).is_none());
    assert!(plans.tenant.baseline(&two).is_none());
    plans.solve();
    assert!(plans.tenant.baseline(&two).is_some());
}

fn start() -> NaiveDate {
    day(2023, 12, 1)
}

/// A plan of one task for each of the given days, one after the other, with a target at the end
fn chain(plans: &mut Plans, days: &[u32]) -> (ShortId, Vec<Uuid>) {
    let plan = plans.empty(start());
    let mut tasks: Vec<Uuid> = Vec::new();
    for d in days {
        let after: Vec<PlanNode> = tasks
            .last()
            .map(|t| PlanNode::Task(*t))
            .into_iter()
            .collect();
        tasks.push(plans.task(plan, *d, &after));
    }
    let last = PlanNode::Task(*tasks.last().unwrap());
    let end = plans.milestone(plan, last);
    plans.target(plan, end);
    (plan, tasks)
}

#[test]
fn plan_priority_decides_who_gets_contended_hours() {
    for favoured in [0, 1] {
        let mut plans = Plans::new();
        let made = [chain(&mut plans, &[1, 1]), chain(&mut plans, &[1, 1])];
        plans.priority(made[favoured].0, 5);
        let state = plans.solve();
        let first = |i: usize| first_day(&state, made[i].0, made[i].1[0]);
        assert_eq!(first(favoured), day(2024, 1, 1));
        assert_eq!(first(1 - favoured), day(2024, 1, 3));
        let plan = &state.plans[&made[1 - favoured].0];
        assert_eq!(plan.plan_end.unwrap().delay(), 2);
        assert_eq!(plan.targets.values().next().unwrap().delay(), 2);
    }
}

#[test]
fn target_priority_counts_beside_plan_priority() {
    let mut plans = Plans::new();
    let made = [chain(&mut plans, &[1]), chain(&mut plans, &[1])];
    let plan = made[1].0;
    let target = *plans.tenant.plans()[&plan].targets().keys().next().unwrap();
    let patch = plinko::types::target::TargetPatch {
        id: target,
        node: None,
        priority: Some(3),
    };
    plans.update(plan, PlanUpdate::Target(Crud::Update(patch)));
    plans.priority(made[0].0, 2);
    let state = plans.solve();
    // 3 beats 2 whichever plan is first by the usual order
    assert_eq!(first_day(&state, plan, made[1].1[0]), day(2024, 1, 1));
}

#[test]
fn work_off_the_target_chain_waits_for_every_chain() {
    let mut plans = Plans::new();
    let (long, _) = chain(&mut plans, &[3]);
    let rest = plans.task(long, 1, &[]);
    let (short, short_chain) = chain(&mut plans, &[1]);
    let state = plans.solve();
    // the long plan is first, but its loose task still goes after the short plan's target chain
    assert_eq!(first_day(&state, short, short_chain[0]), day(2024, 1, 4));
    assert_eq!(first_day(&state, long, rest), day(2024, 1, 5));
}

#[test]
fn chains_to_close_hard_dates_go_before_target_chains() {
    let mut plans = Plans::new();
    let (hard_plan, hard) = plans.plan(start(), 1);
    plans.patch(hard_plan, hard, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(day(2024, 1, 3)));
    });
    let (long, long_chain) = chain(&mut plans, &[3]);
    let state = plans.solve();
    assert_eq!(first_day(&state, hard_plan, hard), day(2024, 1, 3));
    assert!(state.plans[&hard_plan].constraint_violations.is_empty());
    // a three day run cannot straddle the day taken, so it starts after it
    assert_eq!(first_day(&state, long, long_chain[0]), day(2024, 1, 4));
}

#[test]
fn work_in_progress_in_any_plan_claims_hours_first() {
    let mut plans = Plans::new();
    let (first, first_chain) = chain(&mut plans, &[4]);
    let (active, active_task) = plans.plan(start(), 3);
    let state = PersistentState {
        status: Status::InProgress,
        actual_start_date: Some(day(2024, 1, 1)),
        actual_end_date: None,
    };
    plans.patch(active, active_task, |p| p.persistent_state = Some(state));
    let state = plans.solve();
    assert_eq!(first_day(&state, active, active_task), day(2024, 1, 1));
    assert_eq!(first_day(&state, first, first_chain[0]), day(2024, 1, 4));
}
