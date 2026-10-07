use chrono::{DateTime, NaiveDate};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler::{prepare, solve_tenant, state::TenantVolatileState},
    types::{
        crud::Crud,
        dependency::{Dependency, DependencyNode},
        parent::Parent,
        short_id::ShortId,
        task::{NewTask, TaskPatch, Worker},
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

    /// A plan starting on `start` with one task of `days` days for the shared person
    fn plan(&mut self, start: NaiveDate, days: u32) -> (ShortId, Uuid) {
        let (plan, task) = (ShortId::new(), Uuid::now_v7());
        let new = NewPlan {
            id: plan,
            name: "Plan".to_string(),
            start_date: start,
        };
        self.apply(TenantUpdateType::Plan(PlanTenantUpdate::Add(new)));
        let new = NewPlanUser { id: self.shared };
        self.update(plan, PlanUpdate::User(Crud::Add(new)));
        let new = NewTask {
            id: task,
            name: "Task".to_string(),
            parent: Parent::PlanStart,
        };
        self.update(plan, PlanUpdate::Task(Crud::Add(new)));
        let patch = TaskPatch {
            id: task,
            name: None,
            parent: None,
            mode: None,
            duration: Some(days),
            time_constraint: None,
            dependencies: Some(vec![Dependency {
                node: DependencyNode::ParentStart,
                lag: 0,
            }]),
            allocations: Some([(Worker::User(self.shared), Workload::new(8).unwrap())].into()),
            affinities: None,
            actual_workers: None,
            description: None,
            persistent_state: None,
        };
        self.update(plan, PlanUpdate::Task(Crud::Update(patch)));
        (plan, task)
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
