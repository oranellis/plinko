// modules {{{
use chrono::{DateTime, NaiveDate, Utc};
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, short_id::ShortId},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
    },
    planner::items::{
        parent::Parent,
        task::{NewTask, TaskPatch, Worker},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler,
    store::Store,
};
use uuid::Uuid;
// }}}

// helpers {{{
fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
}

fn time() -> DateTime<Utc> {
    DateTime::from_timestamp(978307200, 0).unwrap()
}

fn setup() -> (Store, Tenant, Uuid) {
    let tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), day(1));
    let root = *tenant.users().keys().next().unwrap();
    let store = Store::open_in_memory().unwrap();
    assert!(store.is_empty().unwrap());
    store.create(&tenant).unwrap();
    (store, tenant, root)
}

fn send(
    store: &Store,
    tenant: &mut Tenant,
    seq: u64,
    actor: Uuid,
    update: TenantUpdateType,
) -> u64 {
    let update = tenant.stage(time(), actor, update);
    store.commit(tenant, seq, update).unwrap()
}
// }}}

// a_tenant_loads_from_its_snapshot_and_the_updates_after_it {{{
#[test]
fn a_tenant_loads_from_its_snapshot_and_the_updates_after_it() {
    let (store, mut tenant, root) = setup();
    let new = NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
    };
    let mut seq = send(
        &store,
        &mut tenant,
        0,
        root,
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
    );
    let plan = *tenant.plans().keys().next().unwrap();
    store.snapshot(seq, &tenant).unwrap();
    for _ in 0..3 {
        let new = NewTask {
            id: Uuid::nil(),
            name: "Task".to_string(),
            parent: Parent::Plan,
        };
        let update = PlanUpdate::Task(Crud::Add(new));
        let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
        seq = send(&store, &mut tenant, seq, root, update);
    }
    seq = send(
        &store,
        &mut tenant,
        seq,
        root,
        TenantUpdateType::SetToday(day(5)),
    );

    let (loaded, loaded_seq) = store.load().unwrap();
    assert_eq!(loaded_seq, seq);
    assert_eq!(loaded.today(), day(5));
    assert_eq!(loaded.plans(), tenant.plans());
    assert_eq!(loaded.users(), tenant.users());
    assert_eq!(loaded.plans()[&plan].tasks().len(), 3);

    // a client that has the state at the snapshot catches up from the log
    let missed = store.since(1).unwrap();
    assert_eq!(
        missed.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
}
// }}}

// refused_updates_are_not_saved {{{
#[test]
fn refused_updates_are_not_saved() {
    let (store, mut tenant, root) = setup();
    let seq = send(
        &store,
        &mut tenant,
        0,
        root,
        TenantUpdateType::SetToday(day(2)),
    );
    let refused = tenant.stage(time(), Uuid::new_v4(), TenantUpdateType::SetToday(day(9)));
    assert!(store.commit(&mut tenant, seq, refused).is_err());
    assert_eq!(store.since(0).unwrap().len(), 1);
    assert_eq!(tenant.today(), day(2));
}
// }}}

// an_update_that_is_not_saved_leaves_the_tenant_as_it_was {{{
#[test]
fn an_update_that_is_not_saved_leaves_the_tenant_as_it_was() {
    let (store, mut tenant, root) = setup();
    let before = tenant.clone();
    send(
        &store,
        &mut tenant.clone(),
        0,
        root,
        TenantUpdateType::SetToday(day(2)),
    );
    // update 1 is taken, so saving another as 1 fails
    let new = NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
    };
    for update in [
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
        TenantUpdateType::SetToday(day(3)),
    ] {
        let update = tenant.stage(time(), root, update);
        assert!(store.commit(&mut tenant, 0, update).is_err());
    }
    assert!(tenant.plans().is_empty());
    assert_eq!(tenant.today(), before.today());
    assert_eq!(store.since(0).unwrap().len(), 1);
    assert_eq!(store.load().unwrap().0.today(), day(2));
}
// }}}

// a_snapshot_is_taken_when_the_day_changes {{{
#[test]
fn a_snapshot_is_taken_when_the_day_changes() {
    let (store, mut tenant, root) = setup();
    let mut seq = 0;
    for i in 0..5 {
        let today = TenantUpdateType::SetToday(day(2 + i));
        seq = send(&store, &mut tenant, seq, root, today);
    }
    // each day is read back from its own snapshot, with nothing to replay
    for at in 0..=5 {
        let (loaded, loaded_seq) = store.load_at(at).unwrap();
        assert_eq!((loaded_seq, loaded.today()), (at, day(1 + at as u32)));
    }
    assert_eq!(store.load().unwrap().1, 5);
}
// }}}

// add_task {{{
/// Adds a task of `days` days for `who` to the plan, as the next update
fn add_task(
    store: &Store,
    tenant: &mut Tenant,
    seq: u64,
    (plan, who): (ShortId, Uuid),
    days: u32,
) -> u64 {
    let new = NewTask {
        id: Uuid::nil(),
        name: "Task".to_string(),
        parent: Parent::Plan,
    };
    let send = |tenant: &mut Tenant, seq: u64, update: PlanUpdate| {
        let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
        let update = tenant.stage(time(), who, update);
        store.commit(tenant, seq, update).unwrap()
    };
    let seq = send(tenant, seq, PlanUpdate::Task(Crud::Add(new)));
    // it is given an id when it is added, and is the one nobody works yet
    let mut tasks = tenant.plans()[&plan].tasks().iter();
    let id = *tasks.find(|(_, t)| t.allocations().is_empty()).unwrap().0;
    let patch = TaskPatch {
        id,
        name: None,
        parent: None,
        duration: Some(days),
        time_constraint: None,
        dependencies: None,
        allocations: Some([(Worker::User(who), Workload::new_const(8))].into()),
        affinities: None,
        actual_workers: None,
        persistent_state: None,
    };
    send(tenant, seq, PlanUpdate::Task(Crud::Update(patch)))
}
// }}}

// the_tenant_and_its_schedule_are_read_back_as_of_any_update {{{
#[test]
fn the_tenant_and_its_schedule_are_read_back_as_of_any_update() {
    let (store, mut tenant, root) = setup();
    let new = NewPlan {
        id: ShortId::new(),
        name: "Plan".to_string(),
    };
    let add = TenantUpdateType::Plan(PlanTenantUpdate::Add(new));
    let mut seq = send(&store, &mut tenant, 0, root, add);
    let plan = *tenant.plans().keys().next().unwrap();
    assert_eq!(
        store.schedule_at(seq).unwrap(),
        None,
        "nothing scheduled yet"
    );

    // what is decided after each change is saved, the first whole and the next as a change
    let mut seen = Vec::new();
    for days in [3, 2, 4] {
        seq = add_task(&store, &mut tenant, seq, (plan, root), days);
        let solution = scheduler::schedule(&mut tenant);
        store.save_schedule(seq, &solution).unwrap();
        seen.push((seq, tenant.schedule().cloned().unwrap(), solution));
    }
    // a new day takes a snapshot, and the schedule after it is whole again
    seq = send(
        &store,
        &mut tenant,
        seq,
        root,
        TenantUpdateType::SetToday(day(2)),
    );
    let solution = scheduler::schedule(&mut tenant);
    store.save_schedule(seq, &solution).unwrap();
    seen.push((seq, tenant.schedule().cloned().unwrap(), solution));

    for (at, schedule, solution) in &seen {
        let (mut then, loaded) = store.load_at(*at).unwrap();
        assert_eq!(loaded, *at);
        let saved = store.schedule_at(*at).unwrap();
        assert_eq!(saved.as_ref(), Some(&(*at, solution.clone())));
        // the saved solution gives the schedule as it was, with nothing searched for
        scheduler::restore(&mut then, solution);
        assert_eq!(then.schedule(), Some(schedule));
    }
    // between two schedules the last one before is what there is, and it says which update
    // it was for
    let (first, second) = (seen[0].0, seen[1].0);
    assert_eq!(first + 2, second);
    assert_eq!(store.schedule_at(first + 1).unwrap().unwrap().0, first);
    assert_eq!(store.schedule_at(u64::MAX).unwrap().unwrap().0, seq);
}
// }}}

// passwords_are_checked_and_sessions_come_and_go {{{
#[test]
fn passwords_are_checked_and_sessions_come_and_go() {
    let (store, _, _) = setup();
    let user = Uuid::new_v4();
    assert!(
        !store.check_password(user, "anything").unwrap(),
        "no password set"
    );
    store.set_password(user, "correct horse").unwrap();
    assert!(store.check_password(user, "correct horse").unwrap());
    assert!(!store.check_password(user, "wrong").unwrap());
    store.set_password(user, "new one").unwrap();
    assert!(!store.check_password(user, "correct horse").unwrap());

    let (first, second) = (
        store.start_session(user).unwrap(),
        store.start_session(user).unwrap(),
    );
    assert_ne!(first, second);
    assert_eq!(store.session_user(&first).unwrap(), Some(user));
    assert_eq!(store.session_user("made up").unwrap(), None);
    store.end_session(&first).unwrap();
    assert_eq!(store.session_user(&first).unwrap(), None);
    assert_eq!(store.session_user(&second).unwrap(), Some(user));
    store.end_sessions_of(user).unwrap();
    assert_eq!(store.session_user(&second).unwrap(), None);
}
// }}}
