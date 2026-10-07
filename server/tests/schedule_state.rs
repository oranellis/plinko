use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
    },
    planner::{
        plan::{NewPlan, Plan},
        plan_update::PlanUpdate,
    },
    scheduler::state::{PlanVolatileState, TenantVolatileState},
    types::{
        common::{DateRange, TimeConstraint},
        crud::Crud,
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
        short_id::ShortId,
        task::{Assignee, VolatileState},
        workload::Workload,
    },
};
use uuid::Uuid;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn hours(h: u8) -> Workload {
    Workload::new(h).unwrap()
}

fn task_state(assignments: &[(Assignee, &[(NaiveDate, u8)])]) -> VolatileState {
    let assignments = assignments
        .iter()
        .map(|(who, days)| {
            let days: BTreeMap<_, _> = days.iter().map(|&(day, h)| (day, hours(h))).collect();
            (*who, days)
        })
        .collect();
    VolatileState {
        date_range: DateRange::new(d(2024, 1, 1), d(2024, 1, 5)).unwrap(),
        assignments,
    }
}

#[test]
fn user_schedules_are_derived_from_assignments_across_plans() {
    let (alice, bob) = (Uuid::now_v7(), Uuid::now_v7());
    let (plan_a, plan_b) = (ShortId::new(), ShortId::new());
    let (task_a, task_b) = (Uuid::now_v7(), Uuid::now_v7());
    let monday = d(2024, 1, 1);

    let mut in_a = PlanVolatileState::default();
    in_a.tasks.insert(
        task_a,
        task_state(&[
            (Assignee::User(alice), &[(monday, 4), (d(2024, 1, 2), 4)]),
            (Assignee::Placeholder, &[(monday, 8)]),
        ]),
    );
    let mut in_b = PlanVolatileState::default();
    in_b.tasks.insert(
        task_b,
        task_state(&[
            (Assignee::User(alice), &[(monday, 3)]),
            (Assignee::User(bob), &[(monday, 8)]),
        ]),
    );
    let plans = HashMap::from([(plan_a, in_a), (plan_b, in_b)]);
    let state = TenantVolatileState::from_plans(plans);

    let alice_schedule = &state.users[&alice];
    assert_eq!(alice_schedule.booked_on(monday), 7);
    assert_eq!(alice_schedule.booked_on(d(2024, 1, 2)), 4);
    assert_eq!(alice_schedule.booked_on(d(2024, 1, 3)), 0);
    assert_eq!(alice_schedule.assignments[&monday].len(), 2);
    assert_eq!(state.users[&bob].booked_on(monday), 8);
    assert_eq!(state.users.len(), 2, "placeholders have no schedule");
}

fn update(actor: Uuid, update_type: TenantUpdateType) -> TenantUpdate {
    TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).unwrap(),
        actor,
        update_type,
    }
}

#[test]
fn a_successful_update_clears_the_stored_schedule() {
    let mut tenant = Tenant::new(ShortId::new(), "Tenant".to_string());
    let root = *tenant.users().keys().next().unwrap();
    tenant.store_schedule(TenantVolatileState::default());
    assert!(tenant.schedule().is_some());

    let remove = TenantUpdateType::Plan(PlanTenantUpdate::Remove(ShortId::new()));
    assert!(tenant.apply_update(update(root, remove)).is_err());
    assert!(
        tenant.schedule().is_some(),
        "a failed update changes nothing"
    );

    let new = NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
        start_date: d(2024, 1, 1),
    };
    let add = TenantUpdateType::Plan(PlanTenantUpdate::Add(new));
    tenant.apply_update(update(root, add)).unwrap();
    assert!(tenant.schedule().is_none());
}

#[test]
fn milestones_have_a_time_constraint() {
    let mut plan = Plan::from(NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
        start_date: d(2024, 1, 1),
    });
    let id = Uuid::now_v7();
    let new = NewMilestone {
        id,
        name: "Launch".to_string(),
        parent: Parent::PlanStart,
    };
    plan.apply_update(PlanUpdate::Milestone(Crud::Add(new)))
        .unwrap();
    assert_eq!(
        plan.milestones()[&id].time_constraint(),
        TimeConstraint::None
    );

    let patch = MilestonePatch {
        id,
        name: None,
        parent: None,
        time_constraint: Some(TimeConstraint::Fixed(d(2024, 3, 1))),
        dependencies: None,
        persistent_state: None,
    };
    plan.apply_update(PlanUpdate::Milestone(Crud::Update(patch)))
        .unwrap();
    let constraint = plan.milestones()[&id].time_constraint();
    assert_eq!(constraint, TimeConstraint::Fixed(d(2024, 3, 1)));
}
