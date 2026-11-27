// modules {{{
use chrono::{DateTime, NaiveDate};
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, dates::TimeConstraint, short_id::ShortId},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::items::{
        dependency::PlanNode,
        milestone::NewMilestone,
        parent::Parent,
        task::{Assignee, NewTask, PersistentState, Status, TaskPatch, Worker},
    },
    planner::resources::{target::NewTarget, user::NewPlanUser},
    planner::{
        plan::{NewPlan, PlanSchedulerDataPatch},
        plan_update::PlanUpdate,
    },
    scheduler::{Hints, expand, solve, state::TenantVolatileState},
};
use uuid::Uuid;
// }}}

// helpers {{{
fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

// Plans {{{
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
            shared: Uuid::new_v4(),
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

    /// A plan with no tasks
    fn empty(&mut self) -> ShortId {
        let plan = ShortId::new();
        let new = NewPlan {
            id: plan,
            name: "Plan".to_string(),
        };
        self.apply(TenantUpdateType::Plan(PlanTenantUpdate::Add(new)));
        let new = NewPlanUser { id: self.shared };
        self.update(plan, PlanUpdate::User(Crud::Add(new)));
        plan
    }

    /// A new person in the tenant and the plan
    fn person(&mut self, plan: ShortId) -> Uuid {
        let id = Uuid::new_v4();
        let new = NewTenantLoginUser {
            id,
            name: "Person".to_string(),
            email: format!("{id}@example.com"),
            role: Role::Member,
        };
        self.apply(TenantUpdateType::User(Crud::Add(new)));
        self.update(plan, PlanUpdate::User(Crud::Add(NewPlanUser { id })));
        id
    }

    /// A tag in the plan held by the people
    fn tag(&mut self, plan: ShortId, holders: &[Uuid]) -> Uuid {
        let id = Uuid::new_v4();
        let new = plinko::planner::resources::tag::NewTag {
            id,
            name: "Tag".to_string(),
        };
        self.update(plan, PlanUpdate::Tag(Crud::Add(new)));
        for user in holders {
            let patch = plinko::planner::resources::user::PlanUserDataPatch {
                id: *user,
                tags: Some([id].into()),
            };
            self.update(plan, PlanUpdate::User(Crud::Update(patch)));
        }
        id
    }

    /// A task of `days` days for the shared person, after the tasks and milestones in `after`
    fn task(&mut self, plan: ShortId, days: u32, after: &[PlanNode]) -> Uuid {
        let shared = Worker::User(self.shared);
        self.task_for(plan, days, shared, after)
    }

    fn task_for(&mut self, plan: ShortId, days: u32, worker: Worker, after: &[PlanNode]) -> Uuid {
        let task = Uuid::new_v4();
        let new = NewTask {
            id: task,
            name: "Task".to_string(),
            parent: Parent::Plan,
        };
        self.update(plan, PlanUpdate::Task(Crud::Add(new)));
        let after: Vec<PlanNode> = after.to_vec();
        let who = [(worker, Workload::new_const(8))];
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
            duration: None,
            time_constraint: None,
            dependencies: None,
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        };
        change(&mut patch);
        self.update(plan, PlanUpdate::Task(Crud::Update(patch)));
    }

    fn milestone(&mut self, plan: ShortId, after: PlanNode) -> PlanNode {
        let id = Uuid::new_v4();
        let new = NewMilestone {
            id,
            name: "Milestone".to_string(),
            parent: Parent::Plan,
        };
        self.update(plan, PlanUpdate::Milestone(Crud::Add(new)));
        let patch = plinko::planner::items::milestone::MilestonePatch {
            id,
            name: None,
            parent: None,
            time_constraint: None,
            dependencies: Some(vec![after]),
            persistent_state: None,
        };
        self.update(plan, PlanUpdate::Milestone(Crud::Update(patch)));
        PlanNode::Milestone(id)
    }

    fn target(&mut self, plan: ShortId, node: PlanNode) {
        let new = NewTarget {
            id: Uuid::new_v4(),
            node,
            priority: 0,
        };
        self.update(plan, PlanUpdate::Target(Crud::Add(new)));
    }

    fn priority(&mut self, plan: ShortId, priority: i32) {
        let patch = PlanSchedulerDataPatch {
            priority: Some(priority),
            default_start: None,
        };
        self.update(plan, PlanUpdate::SchedulerData(patch));
    }

    /// A plan with one task of `days` days for the shared person
    fn plan(&mut self, days: u32) -> (ShortId, Uuid) {
        let plan = self.empty();
        (plan, self.task(plan, days, &[]))
    }

    fn solve(&mut self) -> TenantVolatileState {
        solve(&mut self.tenant, &Hints::default()).state
    }
}
// }}}

/// The first day of the task
fn first_day(state: &TenantVolatileState, plan: ShortId, task: Uuid) -> NaiveDate {
    state.plans[&plan].tasks[&task].date_range.start()
}
// }}}

// repair_puts_the_short_plan_first_to_cut_the_worst_delay {{{
#[test]
fn repair_puts_the_short_plan_first_to_cut_the_worst_delay() {
    let mut plans = Plans::new();
    let (short, short_task) = plans.plan(1);
    let (long, long_task) = plans.plan(3);
    let state = plans.solve();
    // the long plan goes first by the usual order, leaving the short one three days late, so
    // repair swaps them to make the long one one day late instead
    assert_eq!(first_day(&state, short, short_task), day(2024, 1, 1));
    assert_eq!(first_day(&state, long, long_task), day(2024, 1, 2));
    assert_eq!(state.users[&plans.shared].booked_on(day(2024, 1, 4)), 8);
}
// }}}

// equal_ends_go_by_id {{{
#[test]
fn equal_ends_go_by_id() {
    let mut plans = Plans::new();
    let ids = [plans.plan(1), plans.plan(1)];
    let state = plans.solve();
    let (low, high) = if ids[0].0 < ids[1].0 {
        (ids[0], ids[1])
    } else {
        (ids[1], ids[0])
    };
    assert_eq!(first_day(&state, low.0, low.1), day(2024, 1, 1));
    assert_eq!(first_day(&state, high.0, high.1), day(2024, 1, 2));
}
// }}}

// forecasts_are_compared_with_the_plan_on_its_own {{{
#[test]
fn forecasts_are_compared_with_the_plan_on_its_own() {
    let mut plans = Plans::new();
    let (short, _) = plans.plan(1);
    let (long, _) = plans.plan(3);
    let state = plans.solve();
    let delay = |plan| state.plans[&plan].plan_end.unwrap().delay();
    assert_eq!(delay(short), 0);
    // it waits for the short plan's day
    assert_eq!(delay(long), 1);
    assert_eq!(
        state.plans[&long].plan_end.unwrap().baseline,
        day(2024, 1, 3)
    );
}
// }}}

// chain {{{
/// A plan of one task for each of the given days, one after the other, with a target at the end
fn chain(plans: &mut Plans, days: &[u32]) -> (ShortId, Vec<Uuid>) {
    let plan = plans.empty();
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
// }}}

// plan_priority_decides_who_gets_contended_hours {{{
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
// }}}

// target_priority_counts_beside_plan_priority {{{
#[test]
fn target_priority_counts_beside_plan_priority() {
    let mut plans = Plans::new();
    let made = [chain(&mut plans, &[1]), chain(&mut plans, &[1])];
    let plan = made[1].0;
    let target = *plans.tenant.plans()[&plan].targets().keys().next().unwrap();
    let patch = plinko::planner::resources::target::TargetPatch {
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
// }}}

// work_off_the_target_chain_waits_for_every_chain {{{
#[test]
fn work_off_the_target_chain_waits_for_every_chain() {
    let mut plans = Plans::new();
    let (long, _) = chain(&mut plans, &[3]);
    let rest = plans.task(long, 1, &[]);
    let (short, short_chain) = chain(&mut plans, &[1]);
    let state = plans.solve();
    // the loose task goes after both target chains, the short one having been moved first
    assert_eq!(first_day(&state, short, short_chain[0]), day(2024, 1, 1));
    assert_eq!(first_day(&state, long, rest), day(2024, 1, 5));
}
// }}}

// chains_to_close_hard_dates_go_before_target_chains {{{
#[test]
fn chains_to_close_hard_dates_go_before_target_chains() {
    let mut plans = Plans::new();
    let (hard_plan, hard) = plans.plan(1);
    plans.patch(hard_plan, hard, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(day(2024, 1, 3)).into());
    });
    let (long, long_chain) = chain(&mut plans, &[3]);
    let state = plans.solve();
    assert_eq!(first_day(&state, hard_plan, hard), day(2024, 1, 3));
    assert!(state.plans[&hard_plan].constraint_violations.is_empty());
    // a three day run cannot straddle the day taken, so it starts after it
    assert_eq!(first_day(&state, long, long_chain[0]), day(2024, 1, 4));
}
// }}}

// work_in_progress_in_any_plan_claims_hours_first {{{
#[test]
fn work_in_progress_in_any_plan_claims_hours_first() {
    let mut plans = Plans::new();
    let (first, first_chain) = chain(&mut plans, &[4]);
    let (active, active_task) = plans.plan(3);
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
// }}}

// Random {{{
/// A small random generator, so the same plans come out every run
struct Random(u64);

impl Random {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}
// }}}

// random_plans_never_overbook_a_person_or_start_before_their_dependencies {{{
#[test]
fn random_plans_never_overbook_a_person_or_start_before_their_dependencies() {
    let mut random = Random(12345);
    for _ in 0..40 {
        let mut plans = Plans::new();
        let mut made: Vec<(ShortId, Uuid, Vec<Uuid>, Option<NaiveDate>)> = Vec::new();
        for _ in 0..=random.below(3) {
            let plan = plans.empty();
            let mut tasks: Vec<Uuid> = Vec::new();
            for _ in 0..(2 + random.below(6)) {
                let after: Vec<Uuid> = tasks
                    .iter()
                    .filter(|_| random.below(3) == 0)
                    .copied()
                    .collect();
                let nodes: Vec<PlanNode> = after.iter().map(|t| PlanNode::Task(*t)).collect();
                let task = plans.task(plan, 1 + random.below(3) as u32, &nodes);
                let fixed =
                    (random.below(4) == 0).then(|| day(2024, 1, 1 + random.below(10) as u32));
                if let Some(date) = fixed {
                    plans.patch(plan, task, |p| {
                        p.time_constraint = Some(TimeConstraint::Fixed(date).into())
                    });
                }
                made.push((plan, task, after, fixed));
                tasks.push(task);
            }
            if random.below(2) == 0 {
                let end = plans.milestone(plan, PlanNode::Task(*tasks.last().unwrap()));
                plans.target(plan, end);
            }
        }
        let solved = solve(&mut plans.tenant, &Hints::default());
        // what was decided gives the same schedule back without searching
        let expanded = expand(&mut plans.tenant, &solved.solution);
        assert_eq!(expanded.state, solved.state);
        assert_eq!(expanded.solution, solved.solution);
        let state = solved.state;
        for schedule in state.users.values() {
            for day in schedule.assignments.keys() {
                assert!(schedule.booked_on(*day) <= 8, "overbooked on {day}");
            }
        }
        for (plan, task, after, fixed) in &made {
            let placed = &state.plans[plan].tasks[task].date_range;
            for before in after {
                assert!(placed.start() >= state.plans[plan].tasks[before].date_range.end());
            }
            assert!(fixed.is_none_or(|date| placed.start() >= date));
        }
    }
}
// }}}

// fixed {{{
fn fixed(plans: &mut Plans, plan: ShortId, task: Uuid, date: NaiveDate) {
    plans.patch(plan, task, |p| {
        p.time_constraint = Some(TimeConstraint::Fixed(date).into())
    });
}
// }}}

// repair_moves_tag_work_off_a_bottleneck_a_bump_cannot_help {{{
#[test]
fn repair_moves_tag_work_off_a_bottleneck_a_bump_cannot_help() {
    let mut plans = Plans::new();
    let tagged = plans.empty();
    let (alice, bob) = (plans.person(tagged), plans.person(tagged));
    let tag = plans.tag(tagged, &[alice, bob]);
    let (low, high) = (alice.min(bob), alice.max(bob));
    // a dated task comes before any target chain, and the tie goes to the lower id
    let held = plans.task_for(tagged, 2, Worker::Tag(tag), &[]);
    fixed(&mut plans, tagged, held, day(2024, 1, 1));
    let waiting = plans.empty();
    plans.update(
        waiting,
        PlanUpdate::User(Crud::Add(NewPlanUser { id: low })),
    );
    let wait = plans.task_for(waiting, 1, Worker::User(low), &[]);
    let end = plans.milestone(waiting, PlanNode::Task(wait));
    plans.target(waiting, end);
    let state = plans.solve();
    assert_eq!(first_day(&state, waiting, wait), day(2024, 1, 1));
    let worker = state.plans[&tagged].tasks[&held].workers[&Worker::Tag(tag)];
    assert_eq!(worker, Assignee::User(high));
}
// }}}
