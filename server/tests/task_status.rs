// modules {{{
use chrono::{DateTime, NaiveDate};
use plinko::{
    common::{crud::Crud, error::PlinkoResult, short_id::ShortId},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
    },
    planner::items::{
        parent::Parent,
        task::{NewTask, PersistentState, Status, TaskPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
};
use uuid::Uuid;
// }}}

// helpers {{{
fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
}

struct Setup {
    tenant: Tenant,
    root: Uuid,
    plan: ShortId,
    task: Uuid,
}

fn apply(setup: &mut Setup, update_type: TenantUpdateType) -> PlinkoResult<()> {
    setup.tenant.apply_update(TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).unwrap(),
        actor: setup.root,
        update_type,
    })
}

fn setup() -> Setup {
    let tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), d(1));
    let root = *tenant.users().keys().next().unwrap();
    let mut setup = Setup {
        tenant,
        root,
        plan: ShortId::new(),
        task: Uuid::new_v4(),
    };
    let new = NewPlan {
        id: setup.plan,
        name: "Plan".to_string(),
    };
    apply(
        &mut setup,
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
    )
    .unwrap();
    let new = NewTask {
        id: setup.task,
        name: "Task".to_string(),
        parent: Parent::Plan,
    };
    let update = PlanUpdate::Task(Crud::Add(new));
    let update = PlanTenantUpdate::Update(setup.plan, Box::new(update));
    apply(&mut setup, TenantUpdateType::Plan(update)).unwrap();
    setup
}

fn set_today(setup: &mut Setup, day: u32) {
    apply(setup, TenantUpdateType::SetToday(d(day))).unwrap();
}

fn set_state(setup: &mut Setup, state: PersistentState) {
    let patch = TaskPatch {
        id: setup.task,
        name: None,
        parent: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: Some(state),
    };
    let update = PlanUpdate::Task(Crud::Update(patch));
    let update = PlanTenantUpdate::Update(setup.plan, Box::new(update));
    apply(setup, TenantUpdateType::Plan(update)).unwrap();
}

fn state(setup: &Setup) -> PersistentState {
    setup.tenant.plans()[&setup.plan].tasks()[&setup.task].persistent_state()
}

fn with(status: Status, start: Option<u32>, end: Option<u32>) -> PersistentState {
    PersistentState {
        status,
        actual_start_date: start.map(d),
        actual_end_date: end.map(d),
    }
}
// }}}

// task_status_and_actual_dates_can_be_set {{{
#[test]
fn task_status_and_actual_dates_can_be_set() {
    let mut s = setup();
    assert_eq!(state(&s), PersistentState::default());
    set_state(&mut s, with(Status::InProgress, Some(2), None));
    assert_eq!(state(&s), with(Status::InProgress, Some(2), None));
    set_state(&mut s, with(Status::Completed, Some(2), Some(9)));
    assert_eq!(state(&s), with(Status::Completed, Some(2), Some(9)));
}
// }}}

// putting_a_task_on_hold_ends_it_today_and_resuming_clears_the_end {{{
#[test]
fn putting_a_task_on_hold_ends_it_today_and_resuming_clears_the_end() {
    let mut s = setup();
    set_state(&mut s, with(Status::InProgress, Some(2), None));
    set_today(&mut s, 5);
    set_state(&mut s, with(Status::OnHold, Some(2), None));
    assert_eq!(state(&s), with(Status::OnHold, Some(2), Some(5)));

    // staying on hold keeps the day it started, even if the update leaves the end out
    set_today(&mut s, 8);
    set_state(&mut s, with(Status::OnHold, Some(2), None));
    assert_eq!(state(&s).actual_end_date, Some(d(5)));

    set_state(&mut s, with(Status::InProgress, Some(2), Some(5)));
    assert_eq!(state(&s), with(Status::InProgress, Some(2), None));
}
// }}}

// started_and_completed_tasks_default_their_dates_to_today {{{
#[test]
fn started_and_completed_tasks_default_their_dates_to_today() {
    let mut s = setup();
    set_today(&mut s, 3);
    set_state(&mut s, with(Status::InProgress, None, None));
    assert_eq!(state(&s), with(Status::InProgress, Some(3), None));

    set_today(&mut s, 6);
    set_state(&mut s, with(Status::Completed, None, None));
    assert_eq!(state(&s), with(Status::Completed, Some(3), Some(6)));

    // dates that come with the update are kept
    set_state(&mut s, with(Status::Completed, Some(2), Some(4)));
    assert_eq!(state(&s), with(Status::Completed, Some(2), Some(4)));
}
// }}}

// an_end_date_sent_with_the_hold_is_kept {{{
#[test]
fn an_end_date_sent_with_the_hold_is_kept() {
    let mut s = setup();
    set_today(&mut s, 5);
    set_state(&mut s, with(Status::OnHold, Some(2), Some(4)));
    assert_eq!(state(&s).actual_end_date, Some(d(4)));
}
// }}}

// the_clock_moves_by_update {{{
#[test]
fn the_clock_moves_by_update() {
    let mut s = setup();
    assert_eq!(s.tenant.today(), d(1));
    set_today(&mut s, 7);
    assert_eq!(s.tenant.today(), d(7));
}
// }}}
