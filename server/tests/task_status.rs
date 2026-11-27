// modules {{{
use chrono::{DateTime, NaiveDate};
use plinko::{
    common::{crud::Crud, dates::TimeConstraint, error::PlinkoResult, short_id::ShortId},
    multitenancy::{
        tenant::{MAX_DURATION, Tenant},
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

fn patch_task(setup: &mut Setup, change: impl FnOnce(&mut TaskPatch)) -> PlinkoResult<()> {
    let mut patch = TaskPatch {
        id: setup.task,
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
    let update = PlanUpdate::Task(Crud::Update(patch));
    let update = PlanTenantUpdate::Update(setup.plan, Box::new(update));
    apply(setup, TenantUpdateType::Plan(update))
}

fn set_state(setup: &mut Setup, state: PersistentState) {
    patch_task(setup, |p| p.persistent_state = Some(state)).unwrap();
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

// a_task_put_back_to_not_started_forgets_its_days {{{
#[test]
fn a_task_put_back_to_not_started_forgets_its_days() {
    let mut s = setup();
    set_today(&mut s, 3);
    set_state(&mut s, with(Status::InProgress, None, None));
    // the dates it had come with the update, as the client sends what it has
    set_state(&mut s, with(Status::NotStarted, Some(3), None));
    assert_eq!(state(&s), with(Status::NotStarted, None, None));
    set_today(&mut s, 6);
    set_state(&mut s, with(Status::InProgress, None, None));
    assert_eq!(state(&s), with(Status::InProgress, Some(6), None));
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

// a_reopened_task_forgets_its_end_and_completing_it_again_ends_it_today {{{
#[test]
fn a_reopened_task_forgets_its_end_and_completing_it_again_ends_it_today() {
    let mut s = setup();
    set_today(&mut s, 3);
    set_state(&mut s, with(Status::Completed, Some(2), None));
    assert_eq!(state(&s), with(Status::Completed, Some(2), Some(3)));
    // the client sends back the dates it has with the new status
    set_state(&mut s, with(Status::InProgress, Some(2), Some(3)));
    assert_eq!(state(&s), with(Status::InProgress, Some(2), None));
    set_today(&mut s, 8);
    set_state(&mut s, with(Status::Completed, Some(2), None));
    assert_eq!(state(&s), with(Status::Completed, Some(2), Some(8)));
    set_state(&mut s, with(Status::Dropped, Some(2), Some(8)));
    set_state(&mut s, with(Status::InProgress, Some(2), Some(8)));
    assert_eq!(state(&s), with(Status::InProgress, Some(2), None));
}
// }}}

// a_completed_task_put_on_hold_ends_today_unless_given_a_new_end {{{
#[test]
fn a_completed_task_put_on_hold_ends_today_unless_given_a_new_end() {
    let mut s = setup();
    set_today(&mut s, 3);
    set_state(&mut s, with(Status::Completed, Some(2), None));
    set_today(&mut s, 6);
    set_state(&mut s, with(Status::OnHold, Some(2), Some(3)));
    assert_eq!(state(&s), with(Status::OnHold, Some(2), Some(6)));
    // a different end date is one the user picked
    set_state(&mut s, with(Status::Completed, Some(2), Some(5)));
    assert_eq!(state(&s), with(Status::Completed, Some(2), Some(5)));
}
// }}}

// dates_and_durations_out_of_range_are_refused {{{
#[test]
fn dates_and_durations_out_of_range_are_refused() {
    let mut s = setup();
    let far_back = PersistentState {
        status: Status::InProgress,
        actual_start_date: Some(NaiveDate::MIN),
        actual_end_date: None,
    };
    assert!(patch_task(&mut s, |p| p.persistent_state = Some(far_back)).is_err());
    let backwards = with(Status::Completed, Some(5), Some(4));
    assert!(patch_task(&mut s, |p| p.persistent_state = Some(backwards)).is_err());
    assert_eq!(state(&s), PersistentState::default());

    let far_ahead = TimeConstraint::Latest(NaiveDate::from_ymd_opt(2100, 1, 1).unwrap());
    assert!(patch_task(&mut s, |p| p.time_constraint = Some(far_ahead.into())).is_err());
    assert!(patch_task(&mut s, |p| p.duration = Some(MAX_DURATION + 1)).is_err());

    // a year either way and a long task are fine
    let late = TimeConstraint::Latest(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    patch_task(&mut s, |p| p.time_constraint = Some(late.into())).unwrap();
    patch_task(&mut s, |p| p.duration = Some(MAX_DURATION)).unwrap();
    let last_year = PersistentState {
        status: Status::Completed,
        actual_start_date: NaiveDate::from_ymd_opt(2023, 1, 1),
        actual_end_date: Some(d(1)),
    };
    set_state(&mut s, last_year);
    assert_eq!(state(&s), last_year);
}
// }}}

// an_update_saved_before_a_check_existed_still_loads {{{
#[test]
fn an_update_saved_before_a_check_existed_still_loads() {
    let mut s = setup();
    let far_ahead = TimeConstraint::Latest(NaiveDate::from_ymd_opt(2100, 1, 1).unwrap());
    let mut patch = TaskPatch {
        id: s.task,
        name: None,
        parent: None,
        duration: None,
        time_constraint: Some(far_ahead.into()),
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: None,
    };
    patch.duration = Some(MAX_DURATION + 1);
    let update = PlanUpdate::Task(Crud::Update(patch));
    let update = TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).unwrap(),
        actor: s.root,
        update_type: TenantUpdateType::Plan(PlanTenantUpdate::Update(s.plan, Box::new(update))),
    };
    assert!(s.tenant.clone().apply_update(update.clone()).is_err());
    s.tenant.apply_logged(update).unwrap();
    // and the check is back for what comes next
    assert!(patch_task(&mut s, |p| p.duration = Some(MAX_DURATION + 1)).is_err());
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
