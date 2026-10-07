use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate};
use plinko::{
    availability::{
        rule::{NewRule, RuleKind},
        rule_set::RuleSetUpdate,
    },
    multitenancy::{
        tenant::*,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role, TenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    planner::{
        plan::{NewPlan, Plan},
        plan_update::PlanUpdate,
    },
    scheduler::state::{PlanVolatileState, TenantVolatileState},
    types::{
        common::DateRange,
        crud::Crud,
        error::PlinkoResult,
        parent::Parent,
        short_id::ShortId,
        task::{self, Assignee, NewTask, PersistentState, Status, TaskPatch, Worker},
        user::{NewPlanUser, NewPlanUserGroup},
        workload::Workload,
    },
};
use uuid::Uuid;

fn new_tenant() -> (Tenant, Uuid) {
    let tenant = Tenant::new(ShortId::new(), "Example Tenant".to_string());
    let root = *tenant.users().keys().next().unwrap();
    (tenant, root)
}

fn apply(tenant: &mut Tenant, actor: Uuid, update_type: TenantUpdateType) -> PlinkoResult<()> {
    tenant.apply_update(TenantUpdate {
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
        actor,
        update_type,
    })
}

fn add_user(tenant: &mut Tenant, actor: Uuid, role: Role) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewTenantLoginUser {
        id,
        name: "User".to_string(),
        email: "user@example.com".to_string(),
        role,
    };
    apply(tenant, actor, TenantUpdateType::User(Crud::Add(new))).unwrap();
    id
}

fn add_group(
    tenant: &mut Tenant,
    actor: Uuid,
    parent: Option<Uuid>,
    users: &[Uuid],
) -> PlinkoResult<Uuid> {
    let id = Uuid::now_v7();
    let new = NewTenantUserGroup {
        id,
        name: "Group".to_string(),
        parent,
        users: users.iter().copied().collect(),
    };
    apply(tenant, actor, TenantUpdateType::UserGroup(Crud::Add(new))).map(|()| id)
}

fn add_plan(tenant: &mut Tenant, actor: Uuid) -> PlinkoResult<ShortId> {
    let id = ShortId::new();
    let new = NewPlan {
        id,
        name: "My Plan".to_string(),
        start_date: NaiveDate::from_ymd_opt(2000, 12, 31).unwrap(),
    };
    let update = TenantUpdateType::Plan(PlanTenantUpdate::Add(new));
    apply(tenant, actor, update).map(|()| id)
}

fn plan_update(
    tenant: &mut Tenant,
    actor: Uuid,
    plan: ShortId,
    update: PlanUpdate,
) -> PlinkoResult<()> {
    let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
    apply(tenant, actor, update)
}

fn add_task(tenant: &mut Tenant, actor: Uuid, plan: ShortId) -> PlinkoResult<()> {
    let task = NewTask {
        id: Uuid::now_v7(),
        name: "Task".to_string(),
        parent: Parent::PlanStart,
    };
    plan_update(tenant, actor, plan, PlanUpdate::Task(Crud::Add(task)))
}

#[test]
fn create_tenant_with_root_admin() {
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
    assert_eq!(user.role(), Role::Admin);
}

#[test]
fn create_tenant_with_login() {
    let user = TenantLoginUser::new(
        "Alice".to_string(),
        "alice@example.com".to_string(),
        Role::Admin,
    );
    let user_id = *user.id();
    let tenant = Tenant::new_with_login(ShortId::new(), "Example Tenant".to_string(), user);
    assert_eq!(tenant.users().len(), 1);
    assert_eq!(tenant.users()[&user_id].email(), "alice@example.com");
}

#[test]
fn plan_updates_through_tenant() {
    let (mut tenant, root) = new_tenant();
    let plan_id = add_plan(&mut tenant, root).unwrap();
    assert!(tenant.plans()[&plan_id].users().contains_key(&root));

    add_task(&mut tenant, root, plan_id).unwrap();
    assert_eq!(tenant.plans()[&plan_id].tasks().len(), 1);

    let remove = TenantUpdateType::Plan(PlanTenantUpdate::Remove(plan_id));
    apply(&mut tenant, root, remove).unwrap();
    assert!(tenant.plans().is_empty());
}

#[test]
fn missing_plan_update_errors() {
    let (mut tenant, root) = new_tenant();
    let remove = TenantUpdateType::Plan(PlanTenantUpdate::Remove(ShortId::new()));
    assert!(apply(&mut tenant, root, remove).is_err());
}

#[test]
fn unknown_actor_is_forbidden() {
    let (mut tenant, _) = new_tenant();
    assert!(add_plan(&mut tenant, Uuid::now_v7()).is_err());
}

#[test]
fn members_only_edit_their_own_plans() {
    let (mut tenant, root) = new_tenant();
    let (alice, bob) = (
        add_user(&mut tenant, root, Role::Member),
        add_user(&mut tenant, root, Role::Member),
    );
    let plan = add_plan(&mut tenant, alice).unwrap();
    add_task(&mut tenant, alice, plan).unwrap();
    assert!(add_task(&mut tenant, bob, plan).is_err());
    add_task(&mut tenant, root, plan).unwrap();

    // bob can be added by someone in the plan, after which he can edit it
    let new = NewPlanUser { id: bob };
    plan_update(&mut tenant, alice, plan, PlanUpdate::User(Crud::Add(new))).unwrap();
    add_task(&mut tenant, bob, plan).unwrap();

    // only users that exist in the tenant can be added to a plan
    let ghost = NewPlanUser { id: Uuid::now_v7() };
    let ghost = PlanUpdate::User(Crud::Add(ghost));
    assert!(plan_update(&mut tenant, alice, plan, ghost).is_err());
}

#[test]
fn only_admins_manage_users_and_groups() {
    let (mut tenant, root) = new_tenant();
    let alice = add_user(&mut tenant, root, Role::Member);
    let new = NewTenantLoginUser {
        id: Uuid::now_v7(),
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
        role: Role::Member,
    };
    assert!(apply(&mut tenant, alice, TenantUpdateType::User(Crud::Add(new))).is_err());
    assert!(add_group(&mut tenant, alice, None, &[]).is_err());

    // members can edit their own profile but not their role
    let patch = |role| TenantLoginUserPatch {
        id: alice,
        name: Some("Alice".to_string()),
        email: None,
        role,
    };
    let profile = TenantUpdateType::User(Crud::Update(patch(None)));
    apply(&mut tenant, alice, profile).unwrap();
    assert_eq!(tenant.users()[&alice].name(), "Alice");
    let promote = TenantUpdateType::User(Crud::Update(patch(Some(Role::Admin))));
    assert!(apply(&mut tenant, alice, promote).is_err());
    assert_eq!(tenant.users()[&alice].role(), Role::Member);
}

#[test]
fn user_groups_nest_and_include_subgroup_members() {
    let (mut tenant, root) = new_tenant();
    let (alice, bob) = (
        add_user(&mut tenant, root, Role::Member),
        add_user(&mut tenant, root, Role::Member),
    );
    let team = add_group(&mut tenant, root, None, &[alice]).unwrap();
    let subteam = add_group(&mut tenant, root, Some(team), &[bob]).unwrap();
    assert_eq!(tenant.group_members(team), HashSet::from([alice, bob]));
    assert_eq!(tenant.group_members(subteam), HashSet::from([bob]));

    // loops, missing parents and non-tenant members are rejected
    let loops = TenantUserGroupPatch {
        id: team,
        name: None,
        parent: Some(Some(subteam)),
        users: None,
    };
    let loops = TenantUpdateType::UserGroup(Crud::Update(loops));
    assert!(apply(&mut tenant, root, loops).is_err());
    assert!(add_group(&mut tenant, root, Some(Uuid::now_v7()), &[]).is_err());
    assert!(add_group(&mut tenant, root, None, &[Uuid::now_v7()]).is_err());
    assert_eq!(tenant.user_groups().len(), 2);

    // groups with subgroups or users that are in them cannot be removed
    let remove = |id| TenantUpdateType::UserGroup(Crud::Remove(id));
    assert!(apply(&mut tenant, root, remove(team)).is_err());
    apply(&mut tenant, root, remove(subteam)).unwrap();
    apply(&mut tenant, root, remove(team)).unwrap();
}

#[test]
fn group_members_edit_plans_their_group_is_assigned_to() {
    let (mut tenant, root) = new_tenant();
    let alice = add_user(&mut tenant, root, Role::Member);
    let team = add_group(&mut tenant, root, None, &[alice]).unwrap();
    let plan = add_plan(&mut tenant, root).unwrap();
    assert!(add_task(&mut tenant, alice, plan).is_err());

    let assign = PlanUpdate::UserGroup(Crud::Add(NewPlanUserGroup { id: team }));
    plan_update(&mut tenant, root, plan, assign).unwrap();
    add_task(&mut tenant, alice, plan).unwrap();

    // users and groups that plans use cannot be removed from the tenant
    let remove_group = TenantUpdateType::UserGroup(Crud::Remove(team));
    assert!(apply(&mut tenant, root, remove_group).is_err());
    let remove_user = TenantUpdateType::User(Crud::Remove(root));
    assert!(apply(&mut tenant, root, remove_user).is_err());
}

fn holiday(day: NaiveDate) -> RuleSetUpdate<Workload> {
    let new = NewRule {
        id: Uuid::now_v7(),
        start_day: day,
        kind: RuleKind::Holiday,
    };
    RuleSetUpdate::Rule(Crud::Add(new))
}

#[test]
fn availability_is_edited_by_the_user_or_an_admin() {
    let (mut tenant, root) = new_tenant();
    let (alice, bob) = (
        add_user(&mut tenant, root, Role::Member),
        add_user(&mut tenant, root, Role::Member),
    );
    let day = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(); // a Wednesday
    let availability = |user, update| TenantUpdateType::Availability(user, update);

    apply(&mut tenant, alice, availability(alice, holiday(day))).unwrap();
    assert_eq!(tenant.users()[&alice].availability().value_on(day).get(), 0);
    assert_eq!(tenant.users()[&bob].availability().value_on(day).get(), 8);

    assert!(apply(&mut tenant, bob, availability(alice, holiday(day))).is_err());
    apply(&mut tenant, root, availability(alice, holiday(day))).unwrap();
    assert_eq!(tenant.users()[&alice].availability().rules().len(), 2);

    let missing = availability(Uuid::now_v7(), holiday(day));
    assert!(apply(&mut tenant, root, missing).is_err());
}

#[test]
fn plan_default_availability_is_set_through_plan_updates() {
    let (mut tenant, root) = new_tenant();
    let plan = add_plan(&mut tenant, root).unwrap();
    let day = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    plan_update(
        &mut tenant,
        root,
        plan,
        PlanUpdate::Availability(holiday(day)),
    )
    .unwrap();
    let default = tenant.plans()[&plan].default_availability();
    assert_eq!(default.value_on(day).get(), 0);
}

#[test]
fn calendars_are_rebuilt_only_when_availability_changes() {
    let (mut tenant, root) = new_tenant();
    let alice = add_user(&mut tenant, root, Role::Member);
    let (from, until) = (
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
    );
    assert!(tenant.calendar(&alice).is_none());
    tenant.refresh_calendars(from, until);
    let day = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    assert_eq!(tenant.calendar(&alice).unwrap().on(day).unwrap().get(), 8);

    // unrelated updates keep the calendar, availability updates clear it until the next refresh
    add_plan(&mut tenant, root).unwrap();
    assert!(tenant.calendar(&alice).is_some());
    let update = TenantUpdateType::Availability(alice, holiday(day));
    apply(&mut tenant, alice, update).unwrap();
    assert!(tenant.calendar(&alice).is_none());
    assert!(tenant.calendar(&root).is_some());
    tenant.refresh_calendars(from, until);
    assert_eq!(tenant.calendar(&alice).unwrap().on(day).unwrap().get(), 0);

    // a wider range forces a rebuild, and removing a user drops theirs
    let wider = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    tenant.refresh_calendars(from, wider);
    assert_eq!(tenant.calendar(&alice).unwrap().end(), wider);
    apply(
        &mut tenant,
        root,
        TenantUpdateType::User(Crud::Remove(alice)),
    )
    .unwrap();
    assert!(tenant.calendar(&alice).is_none());
}

#[test]
fn plan_default_calendars_are_cached_until_their_rules_change() {
    let (mut tenant, root) = new_tenant();
    let plan = add_plan(&mut tenant, root).unwrap();
    let other = add_plan(&mut tenant, root).unwrap();
    let day = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    let (from, until) = (day, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    tenant.refresh_calendars(from, until);
    assert_eq!(
        tenant.plan_calendar(&plan).unwrap().on(day).unwrap().get(),
        8
    );

    plan_update(
        &mut tenant,
        root,
        plan,
        PlanUpdate::Availability(holiday(day)),
    )
    .unwrap();
    assert!(tenant.plan_calendar(&plan).is_none());
    assert!(tenant.plan_calendar(&other).is_some());
    tenant.refresh_calendars(from, until);
    assert_eq!(
        tenant.plan_calendar(&plan).unwrap().on(day).unwrap().get(),
        0
    );

    add_task(&mut tenant, root, plan).unwrap();
    assert!(tenant.plan_calendar(&plan).is_some());
    let remove = TenantUpdateType::Plan(PlanTenantUpdate::Remove(plan));
    apply(&mut tenant, root, remove).unwrap();
    assert!(tenant.plan_calendar(&plan).is_none());
}

#[test]
fn the_last_admin_cannot_be_demoted_or_removed() {
    let (mut tenant, root) = new_tenant();
    let alice = add_user(&mut tenant, root, Role::Member);
    let role = |id, role| {
        TenantUpdateType::User(Crud::Update(TenantLoginUserPatch {
            id,
            name: None,
            email: None,
            role: Some(role),
        }))
    };
    assert!(apply(&mut tenant, root, role(root, Role::Member)).is_err());
    assert_eq!(tenant.users()[&root].role(), Role::Admin);
    let remove = TenantUpdateType::User(Crud::Remove(root));
    assert!(apply(&mut tenant, root, remove).is_err());

    // with a second admin either can step down
    apply(&mut tenant, root, role(alice, Role::Admin)).unwrap();
    apply(&mut tenant, root, role(root, Role::Member)).unwrap();
    let remove = TenantUpdateType::User(Crud::Remove(root));
    apply(&mut tenant, alice, remove).unwrap();
}

fn add_task_for_placeholder(tenant: &mut Tenant, actor: Uuid, plan: ShortId) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewTask {
        id,
        name: "Task".to_string(),
        parent: Parent::PlanStart,
    };
    plan_update(tenant, actor, plan, PlanUpdate::Task(Crud::Add(new))).unwrap();
    let patch = task_patch(id, |p| {
        p.allocations = Some(HashMap::from([(
            Worker::Placeholder,
            Workload::new(4).unwrap(),
        )]));
    });
    plan_update(tenant, actor, plan, PlanUpdate::Task(Crud::Update(patch))).unwrap();
    id
}

fn task_patch(id: Uuid, change: impl FnOnce(&mut TaskPatch)) -> TaskPatch {
    let mut patch = TaskPatch {
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
        description: None,
        persistent_state: None,
    };
    change(&mut patch);
    patch
}

fn start(task: Uuid) -> TaskPatch {
    task_patch(task, |p| {
        p.persistent_state = Some(PersistentState {
            status: Status::InProgress,
            actual_start_date: None,
            actual_end_date: None,
        });
    })
}

fn scheduled_for(task: Uuid, plan: ShortId, people: &[(Worker, Assignee)]) -> TenantVolatileState {
    let state = task::VolatileState {
        date_range: DateRange::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
        )
        .unwrap(),
        assignments: HashMap::new(),
        workers: people.iter().copied().collect(),
    };
    let mut plan_state = PlanVolatileState::default();
    plan_state.tasks.insert(task, state);
    TenantVolatileState::from_plans(HashMap::from([(plan, plan_state)]))
}

fn started_update(plan: ShortId, patch: TaskPatch) -> TenantUpdateType {
    let update = PlanUpdate::Task(Crud::Update(patch));
    TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)))
}

#[test]
fn starting_a_task_records_the_people_last_scheduled_for_it() {
    let (mut tenant, root) = new_tenant();
    let bob = add_user(&mut tenant, root, Role::Member);
    let plan = add_plan(&mut tenant, root).unwrap();
    let task = add_task_for_placeholder(&mut tenant, root, plan);
    let people = [(Worker::Placeholder, Assignee::User(bob))];

    // without a stored schedule there is nothing to record
    let mut update = started_update(plan, start(task));
    tenant.fill_defaults(&mut update);
    assert!(recorded_in(&update).is_none());

    tenant.store_schedule(scheduled_for(task, plan, &people));
    let mut update = started_update(plan, start(task));
    tenant.fill_defaults(&mut update);
    apply(&mut tenant, root, update).unwrap();
    let recorded = tenant.plans()[&plan].tasks()[&task]
        .actual_workers()
        .clone();
    assert_eq!(recorded, HashMap::from(people));

    // an update that names people keeps them, and a task with a record keeps it
    tenant.store_schedule(scheduled_for(
        task,
        plan,
        &[(Worker::Placeholder, Assignee::Placeholder)],
    ));
    let mut update = started_update(plan, start(task));
    tenant.fill_defaults(&mut update);
    assert!(recorded_in(&update).is_none());
}

#[test]
fn recorded_people_must_exist_and_cannot_be_removed() {
    let (mut tenant, root) = new_tenant();
    let bob = add_user(&mut tenant, root, Role::Member);
    let plan = add_plan(&mut tenant, root).unwrap();
    let task = add_task_for_placeholder(&mut tenant, root, plan);
    let record = |who| {
        started_update(
            plan,
            task_patch(task, |p| {
                p.actual_workers = Some(HashMap::from([(Worker::Placeholder, who)]));
            }),
        )
    };
    let ghost = record(Assignee::User(Uuid::now_v7()));
    assert!(apply(&mut tenant, root, ghost).is_err());

    apply(&mut tenant, root, record(Assignee::User(bob))).unwrap();
    let remove = TenantUpdateType::User(Crud::Remove(bob));
    assert!(apply(&mut tenant, root, remove).is_err());

    apply(&mut tenant, root, record(Assignee::Placeholder)).unwrap();
    let remove = TenantUpdateType::User(Crud::Remove(bob));
    apply(&mut tenant, root, remove).unwrap();
}

fn recorded_in(update: &TenantUpdateType) -> Option<&HashMap<Worker, Assignee>> {
    let TenantUpdateType::Plan(PlanTenantUpdate::Update(_, inner)) = update else {
        panic!("expected a plan update");
    };
    match &**inner {
        PlanUpdate::Task(Crud::Update(patch)) => patch.actual_workers.as_ref(),
        _ => panic!("expected a task update"),
    }
}
