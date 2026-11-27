// modules {{{
use chrono::NaiveDate;
use plinko::{
    availability::workload::Workload,
    availability::{
        rule::{NewRule, RuleKind},
        rule_set::RuleSetUpdate,
    },
    common::{crud::Crud, short_id::ShortId},
    multitenancy::{
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
        user_group::NewTenantUserGroup,
    },
    planner::items::{
        dependency::PlanNode, milestone::NewMilestone, parent::Parent, plan_buffer::NewPlanBuffer,
        task::NewTask, task_group::NewTaskGroup,
    },
    planner::resources::{
        affinity::NewAffinity,
        data_block::{BlockContent, NewDataBlock},
        tag::NewTag,
        target::NewTarget,
        user::{NewPlanUser, NewPlanUserGroup},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
};
use uuid::Uuid;
// }}}

// helpers {{{
const CLIENT_ID: Uuid = Uuid::nil();

fn rule_update<V>(kind: RuleKind<V>) -> RuleSetUpdate<V> {
    RuleSetUpdate::Rule(Crud::Add(NewRule {
        name: String::new(),
        stop_day: None,
        id: CLIENT_ID,
        start_day: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        kind,
    }))
}

fn rule_id<V>(update: &RuleSetUpdate<V>) -> Uuid {
    match update {
        RuleSetUpdate::Rule(Crud::Add(new)) => new.id,
        _ => panic!("expected a rule add"),
    }
}

fn plan_update_ids(mut update: PlanUpdate) -> Uuid {
    update.assign_ids();
    match update {
        PlanUpdate::Task(Crud::Add(new)) => new.id,
        PlanUpdate::TaskGroup(Crud::Add(new)) => new.id,
        PlanUpdate::Milestone(Crud::Add(new)) => new.id,
        PlanUpdate::PlanBuffer(Crud::Add(new)) => new.id,
        PlanUpdate::Tag(Crud::Add(new)) => new.id,
        PlanUpdate::Target(Crud::Add(new)) => new.id,
        PlanUpdate::Affinity(Crud::Add(new)) => new.id,
        PlanUpdate::AffinityRules(_, update) => rule_id(&update),
        PlanUpdate::Data(_, Crud::Add(new)) => new.id,
        PlanUpdate::Availability(update) => rule_id(&update),
        PlanUpdate::User(Crud::Add(new)) => new.id,
        PlanUpdate::UserGroup(Crud::Add(new)) => new.id,
        _ => panic!("expected an add"),
    }
}
// }}}

// plan_adds_get_fresh_ids {{{
#[test]
fn plan_adds_get_fresh_ids() {
    let parent = Parent::Plan;
    let name = || "Name".to_string();
    let content = || BlockContent::TodoList(Vec::new());
    let updates = [
        PlanUpdate::Task(Crud::Add(NewTask {
            id: CLIENT_ID,
            name: name(),
            parent,
        })),
        PlanUpdate::TaskGroup(Crud::Add(NewTaskGroup {
            id: CLIENT_ID,
            name: name(),
            parent,
        })),
        PlanUpdate::Milestone(Crud::Add(NewMilestone {
            id: CLIENT_ID,
            name: name(),
            parent,
        })),
        PlanUpdate::PlanBuffer(Crud::Add(NewPlanBuffer {
            id: CLIENT_ID,
            parent,
            duration: 1,
        })),
        PlanUpdate::Tag(Crud::Add(NewTag {
            id: CLIENT_ID,
            name: name(),
        })),
        PlanUpdate::Target(Crud::Add(NewTarget {
            id: CLIENT_ID,
            node: PlanNode::Task(CLIENT_ID),
            priority: 0,
        })),
        PlanUpdate::Affinity(Crud::Add(NewAffinity {
            id: CLIENT_ID,
            name: name(),
        })),
        PlanUpdate::AffinityRules(CLIENT_ID, rule_update(RuleKind::Holiday)),
        PlanUpdate::Data(
            PlanNode::Task(CLIENT_ID),
            Crud::Add(NewDataBlock {
                id: CLIENT_ID,
                title: name(),
                content: content(),
            }),
        ),
        PlanUpdate::Availability(rule_update::<Workload>(RuleKind::Holiday)),
    ];
    for update in updates {
        assert_ne!(plan_update_ids(update), CLIENT_ID);
    }
}
// }}}

// existing_tenant_users_and_groups_keep_their_ids_in_a_plan {{{
#[test]
fn existing_tenant_users_and_groups_keep_their_ids_in_a_plan() {
    let user = PlanUpdate::User(Crud::Add(NewPlanUser { id: CLIENT_ID }));
    assert_eq!(plan_update_ids(user), CLIENT_ID);
    let group = PlanUpdate::UserGroup(Crud::Add(NewPlanUserGroup { id: CLIENT_ID }));
    assert_eq!(plan_update_ids(group), CLIENT_ID);
}
// }}}

// updates_and_removals_keep_the_ids_they_name {{{
#[test]
fn updates_and_removals_keep_the_ids_they_name() {
    let mut remove = PlanUpdate::Task(Crud::Remove(CLIENT_ID));
    remove.assign_ids();
    assert!(matches!(remove, PlanUpdate::Task(Crud::Remove(id)) if id == CLIENT_ID));
}
// }}}

// tenant_adds_get_fresh_ids {{{
#[test]
fn tenant_adds_get_fresh_ids() {
    let mut user = TenantUpdateType::User(Crud::Add(NewTenantLoginUser {
        id: CLIENT_ID,
        name: "User".to_string(),
        email: "user@example.com".to_string(),
        role: Role::Member,
    }));
    user.assign_ids();
    assert!(matches!(user, TenantUpdateType::User(Crud::Add(new)) if new.id != CLIENT_ID));

    let mut group = TenantUpdateType::UserGroup(Crud::Add(NewTenantUserGroup {
        id: CLIENT_ID,
        name: "Group".to_string(),
        parent: None,
        users: Default::default(),
    }));
    group.assign_ids();
    assert!(matches!(group, TenantUpdateType::UserGroup(Crud::Add(new)) if new.id != CLIENT_ID));

    let mut availability =
        TenantUpdateType::Availability(CLIENT_ID, rule_update(RuleKind::Holiday));
    availability.assign_ids();
    let TenantUpdateType::Availability(user, update) = availability else {
        panic!("expected availability");
    };
    assert_eq!(user, CLIENT_ID);
    assert_ne!(rule_id::<Workload>(&update), CLIENT_ID);
}
// }}}

// new_plans_get_fresh_ids_and_nested_plan_updates_are_assigned {{{
#[test]
fn new_plans_get_fresh_ids_and_nested_plan_updates_are_assigned() {
    let client_plan = ShortId::from_str_checked("AAAAAAAA").unwrap();
    let new = NewPlan {
        id: client_plan,
        name: "Plan".to_string(),
    };
    let mut add = TenantUpdateType::Plan(PlanTenantUpdate::Add(new));
    add.assign_ids();
    let TenantUpdateType::Plan(PlanTenantUpdate::Add(new)) = add else {
        panic!("expected a plan add");
    };
    assert_ne!(new.id, client_plan);

    let task = NewTask {
        id: CLIENT_ID,
        name: "Task".to_string(),
        parent: Parent::Plan,
    };
    let inner = PlanUpdate::Task(Crud::Add(task));
    let mut update = TenantUpdateType::Plan(PlanTenantUpdate::Update(client_plan, Box::new(inner)));
    update.assign_ids();
    let TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, inner)) = update else {
        panic!("expected a plan update");
    };
    let PlanUpdate::Task(Crud::Add(new)) = *inner else {
        panic!("expected a task add");
    };
    assert_eq!(plan, client_plan, "the plan being edited is not reassigned");
    assert_ne!(new.id, CLIENT_ID);
}
// }}}
