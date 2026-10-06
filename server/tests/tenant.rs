use std::collections::HashMap;

use chrono::{DateTime, NaiveDate};
use plinko::{
    multitenancy::{
        tenant::*,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::TenantLoginUser,
    },
    planner::{
        plan::{NewPlan, Plan},
        plan_update::{PlanUpdate, TaskUpdate},
    },
    types::{parent::Parent, short_id::ShortId, task::NewTask},
};
use uuid::Uuid;

#[test]
fn create_tenant_with_root_user() {
    let tenant_id = ShortId::new();
    let tenant_with_root = Tenant::new(tenant_id, "Example Tenant".to_string());
    assert_eq!(*tenant_with_root.id(), tenant_id);
    assert_eq!(tenant_with_root.name(), "Example Tenant");
    assert_eq!(*tenant_with_root.plans(), HashMap::<ShortId, Plan>::new());
    let users = tenant_with_root.users();
    assert!(users.len() == 1);
    let (_, user) = users.iter().next().unwrap();
    assert_eq!(user.name(), "Root User");
    assert_eq!(user.email(), "root@localhost");
}

#[test]
fn create_tenant_with_login() {
    let user = TenantLoginUser::new("Alice".to_string(), "alice@example.com".to_string());
    let user_id = *user.id();
    let tenant = Tenant::new_with_login(ShortId::new(), "Example Tenant".to_string(), user);
    assert_eq!(tenant.users().len(), 1);
    assert_eq!(tenant.users()[&user_id].email(), "alice@example.com");
}

fn update(update_type: TenantUpdateType) -> TenantUpdate {
    TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
        update_type,
    }
}

#[test]
fn plan_updates_through_tenant() {
    let mut tenant = Tenant::new(ShortId::new(), "Example Tenant".to_string());
    let plan_id = ShortId::new();
    let add = PlanTenantUpdate::Add(NewPlan {
        id: plan_id,
        name: "My Plan".to_string(),
        start_date: NaiveDate::from_ymd_opt(2000, 12, 31).unwrap(),
    });
    tenant
        .apply_update(update(TenantUpdateType::Plan(add)))
        .unwrap();

    let task = NewTask {
        id: Uuid::now_v7(),
        name: "Task".to_string(),
        parent: Parent::PlanStart,
    };
    let task_id = task.id;
    let inner = PlanUpdate::Task(TaskUpdate::Add(task));
    let change = PlanTenantUpdate::Update(plan_id, inner);
    tenant
        .apply_update(update(TenantUpdateType::Plan(change)))
        .unwrap();
    assert!(tenant.plans()[&plan_id].tasks().contains_key(&task_id));

    let remove = PlanTenantUpdate::Remove(plan_id);
    tenant
        .apply_update(update(TenantUpdateType::Plan(remove)))
        .unwrap();
    assert!(tenant.plans().is_empty());
}

#[test]
fn missing_plan_update_errors() {
    let mut tenant = Tenant::new(ShortId::new(), "Example Tenant".to_string());
    let remove = PlanTenantUpdate::Remove(ShortId::new());
    assert!(
        tenant
            .apply_update(update(TenantUpdateType::Plan(remove)))
            .is_err()
    );
}
