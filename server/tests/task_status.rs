use chrono::NaiveDate;
use plinko::{
    planner::{
        plan::{NewPlan, Plan, PlanSchedulerDataPatch},
        plan_update::PlanUpdate,
    },
    types::{
        crud::Crud,
        parent::Parent,
        short_id::ShortId,
        task::{NewTask, PersistentState, Status, TaskPatch},
    },
};
use uuid::Uuid;

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
}

fn plan_with_task() -> (Plan, Uuid) {
    let mut plan = Plan::from(NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
        start_date: d(1),
    });
    let id = Uuid::now_v7();
    let new = NewTask {
        id,
        name: "Task".to_string(),
        parent: Parent::PlanStart,
    };
    plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
    (plan, id)
}

fn set_today(plan: &mut Plan, day: u32) {
    let patch = PlanSchedulerDataPatch {
        start_date: None,
        today: Some(d(day)),
        scheduler_target: None,
    };
    plan.apply_update(PlanUpdate::SchedulerData(patch)).unwrap();
}

fn set_state(plan: &mut Plan, id: Uuid, state: PersistentState) {
    let patch = TaskPatch {
        id,
        name: None,
        parent: None,
        mode: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        description: None,
        persistent_state: Some(state),
    };
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
        .unwrap();
}

fn state(plan: &Plan, id: Uuid) -> PersistentState {
    plan.tasks()[&id].persistent_state()
}

fn with(status: Status, start: Option<u32>, end: Option<u32>) -> PersistentState {
    PersistentState {
        status,
        actual_start_date: start.map(d),
        actual_end_date: end.map(d),
    }
}

#[test]
fn task_status_and_actual_dates_can_be_set() {
    let (mut plan, id) = plan_with_task();
    assert_eq!(state(&plan, id), PersistentState::default());
    set_state(&mut plan, id, with(Status::InProgress, Some(2), None));
    assert_eq!(state(&plan, id), with(Status::InProgress, Some(2), None));
    set_state(&mut plan, id, with(Status::Completed, Some(2), Some(9)));
    assert_eq!(state(&plan, id), with(Status::Completed, Some(2), Some(9)));
}

#[test]
fn putting_a_task_on_hold_ends_it_today_and_resuming_clears_the_end() {
    let (mut plan, id) = plan_with_task();
    set_state(&mut plan, id, with(Status::InProgress, Some(2), None));
    set_today(&mut plan, 5);
    set_state(&mut plan, id, with(Status::OnHold, Some(2), None));
    assert_eq!(state(&plan, id), with(Status::OnHold, Some(2), Some(5)));

    // staying on hold keeps the day it started, even if the update leaves the end out
    set_today(&mut plan, 8);
    set_state(&mut plan, id, with(Status::OnHold, Some(2), None));
    assert_eq!(state(&plan, id).actual_end_date, Some(d(5)));

    set_state(&mut plan, id, with(Status::InProgress, Some(2), Some(5)));
    assert_eq!(state(&plan, id), with(Status::InProgress, Some(2), None));
}

#[test]
fn an_end_date_sent_with_the_hold_is_kept() {
    let (mut plan, id) = plan_with_task();
    set_today(&mut plan, 5);
    set_state(&mut plan, id, with(Status::OnHold, Some(2), Some(4)));
    assert_eq!(state(&plan, id).actual_end_date, Some(d(4)));
}
