// modules {{{
use chrono::{DateTime, NaiveDate, Utc};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    types::{
        crud::Crud,
        dependency::{Dependency, DependencyNode},
        parent::Parent,
        short_id::ShortId,
        task::{Assignee, NewTask, PersistentState, Status, TaskPatch, Worker},
        user::NewPlanUser,
        workload::Workload,
    },
};
use uuid::Uuid;
// }}}

// helpers {{{
const CLIENT_ID: Uuid = Uuid::nil();

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
}

fn time() -> DateTime<Utc> {
    DateTime::from_timestamp(978307200, 0).unwrap()
}

fn patch(id: Uuid) -> TaskPatch {
    TaskPatch {
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
    }
}
// }}}

// updates_are_staged_applied_and_scheduled {{{
#[test]
fn updates_are_staged_applied_and_scheduled() {
    let mut tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), day(1));
    let root = *tenant.users().keys().next().unwrap();
    let send = |tenant: &mut Tenant, update_type| {
        let update = tenant.stage(time(), root, update_type);
        tenant.apply_and_schedule(update)
    };

    // clients send whatever ids, the server gives new ones
    let new = NewPlan {
        id: ShortId::from_str_checked("AAAAAAAA").unwrap(),
        name: "Plan".to_string(),
    };
    send(
        &mut tenant,
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
    )
    .unwrap();
    let plan = *tenant.plans().keys().next().unwrap();
    assert_ne!(plan, ShortId::from_str_checked("AAAAAAAA").unwrap());

    let new = NewTenantLoginUser {
        id: CLIENT_ID,
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
        role: Role::Member,
    };
    send(&mut tenant, TenantUpdateType::User(Crud::Add(new))).unwrap();
    let bob = *tenant.users().keys().find(|u| **u != root).unwrap();
    let in_plan = |update| TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
    let add_bob = PlanUpdate::User(Crud::Add(NewPlanUser { id: bob }));
    send(&mut tenant, in_plan(add_bob)).unwrap();

    let new = NewTask {
        id: CLIENT_ID,
        name: "Task".to_string(),
        parent: Parent::Plan,
    };
    send(&mut tenant, in_plan(PlanUpdate::Task(Crud::Add(new)))).unwrap();
    let task = *tenant.plans()[&plan].tasks().keys().next().unwrap();
    assert_ne!(task, CLIENT_ID);
    let setup = TaskPatch {
        duration: Some(2),
        dependencies: Some(vec![Dependency {
            node: DependencyNode::ParentStart,
            lag: 0,
        }]),
        allocations: Some([(Worker::User(bob), Workload::new(8).unwrap())].into()),
        ..patch(task)
    };
    send(&mut tenant, in_plan(PlanUpdate::Task(Crud::Update(setup)))).unwrap();

    // every applied update leaves a current schedule
    let scheduled = &tenant.schedule().unwrap().plans[&plan].tasks[&task];
    assert_eq!(scheduled.date_range.start(), day(2));
    assert_eq!(tenant.schedule().unwrap().users[&bob].booked_on(day(2)), 8);

    // staging fills in the people last scheduled, so the logged update holds them
    let started = TaskPatch {
        persistent_state: Some(PersistentState {
            status: Status::InProgress,
            actual_start_date: None,
            actual_end_date: None,
        }),
        ..patch(task)
    };
    let update = tenant.stage(
        time(),
        root,
        in_plan(PlanUpdate::Task(Crud::Update(started))),
    );
    tenant.apply_and_schedule(update).unwrap();
    let recorded = tenant.plans()[&plan].tasks()[&task]
        .actual_workers()
        .clone();
    assert_eq!(recorded[&Worker::User(bob)], Assignee::User(bob));
    assert_eq!(
        tenant.plans()[&plan].tasks()[&task]
            .persistent_state()
            .actual_start_date,
        Some(day(1))
    );

    // a refused update changes nothing and keeps the schedule
    let before = tenant.schedule().unwrap().clone();
    let refused = tenant.stage(time(), Uuid::now_v7(), TenantUpdateType::SetToday(day(9)));
    assert!(tenant.apply_and_schedule(refused).is_err());
    assert_eq!(tenant.today(), day(1));
    assert_eq!(tenant.schedule(), Some(&before));
}
// }}}
