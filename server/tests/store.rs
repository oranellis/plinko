// modules {{{
use chrono::{DateTime, NaiveDate, Utc};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    store::Store,
    types::{crud::Crud, parent::Parent, short_id::ShortId, task::NewTask},
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
// }}}

// fn setup {{{
fn setup() -> (Store, Tenant, Uuid) {
    let tenant = Tenant::new(ShortId::new(), "Tenant".to_string(), day(1));
    let root = *tenant.users().keys().next().unwrap();
    let store = Store::open_in_memory().unwrap();
    assert!(store.is_empty().unwrap());
    store.create(&tenant).unwrap();
    (store, tenant, root)
}
// }}}

// fn send {{{
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

// fn a_tenant_loads_from_its_snapshot_and_the_updates_after_it {{{
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

// fn refused_updates_are_not_saved {{{
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
    let refused = tenant.stage(time(), Uuid::now_v7(), TenantUpdateType::SetToday(day(9)));
    assert!(store.commit(&mut tenant, seq, refused).is_err());
    assert_eq!(store.since(0).unwrap().len(), 1);
    assert_eq!(tenant.today(), day(2));
}
// }}}

// fn snapshots_are_taken_as_the_log_grows {{{
#[test]
fn snapshots_are_taken_as_the_log_grows() {
    let (store, mut tenant, root) = setup();
    let mut seq = 0;
    for i in 0..205 {
        let today = TenantUpdateType::SetToday(day(1 + i % 20));
        seq = send(&store, &mut tenant, seq, root, today);
    }
    let (loaded, loaded_seq) = store.load().unwrap();
    assert_eq!((loaded_seq, loaded.today()), (205, tenant.today()));
}
// }}}

// fn passwords_are_checked_and_sessions_come_and_go {{{
#[test]
fn passwords_are_checked_and_sessions_come_and_go() {
    let (store, _, _) = setup();
    let user = Uuid::now_v7();
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
