//! Times a full schedule of a busy tenant: `cargo run --release --example bench [plans] [tasks]`.
//! The login of the saved tenant is root@localhost with the password password123.

// modules {{{
use std::{collections::HashMap, time::Instant};

use chrono::{DateTime, NaiveDate};
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, dates::TimeConstraint, short_id::ShortId},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::items::{
        dependency::{Dependency, PlanNode},
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
        task::{NewTask, TaskPatch, Worker},
    },
    planner::resources::{
        tag::NewTag,
        target::NewTarget,
        user::{NewPlanUser, PlanUserDataPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler,
};
use uuid::Uuid;
// }}}

/// Ids that are the same every run, so runs can be compared
struct Ids(u128);

// impl Ids {{{
impl Ids {
    fn next(&mut self) -> Uuid {
        self.0 += 1;
        Uuid::from_u128(self.0)
    }
}
// }}}

// Random {{{
struct Random(u64);

impl Random {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}
// }}}

// apply {{{
fn apply(tenant: &mut Tenant, actor: Uuid, update_type: TenantUpdateType) {
    let time = DateTime::from_timestamp(978307200, 0).unwrap();
    tenant
        .apply_update(TenantUpdate {
            time,
            actor,
            update_type,
        })
        .unwrap();
}
// }}}

// main {{{
fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let (plans, per_plan) = (
        args.first().copied().unwrap_or(5),
        args.get(1).copied().unwrap_or(200),
    );
    let today = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut tenant = Tenant::new(ShortId::new(), "Bench".to_string(), today);
    let root = *tenant.users().keys().next().unwrap();
    let mut random = Random(7);
    let mut ids = Ids(0);

    let people: Vec<Uuid> = (0..20)
        .map(|i| {
            let id = ids.next();
            let new = NewTenantLoginUser {
                id,
                name: format!("User {i}"),
                email: format!("user{i}@example.com"),
                role: Role::Member,
            };
            apply(&mut tenant, root, TenantUpdateType::User(Crud::Add(new)));
            id
        })
        .collect();
    for _ in 0..plans {
        let plan = ShortId::from_str_checked(&format!("plan{:04}", ids.0)).unwrap();
        ids.0 += 1;
        let new = NewPlan {
            id: plan,
            name: "Plan".to_string(),
        };
        apply(
            &mut tenant,
            root,
            TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
        );
        let send = |tenant: &mut Tenant, update| {
            let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
            apply(tenant, root, update);
        };
        for person in &people {
            send(
                &mut tenant,
                PlanUpdate::User(Crud::Add(NewPlanUser { id: *person })),
            );
        }
        let tags: Vec<Uuid> = (0..4)
            .map(|_| {
                let id = ids.next();
                send(
                    &mut tenant,
                    PlanUpdate::Tag(Crud::Add(NewTag {
                        id,
                        name: "Tag".to_string(),
                    })),
                );
                id
            })
            .collect();
        for (i, person) in people.iter().enumerate() {
            let patch = PlanUserDataPatch {
                id: *person,
                tags: Some([tags[i % 4]].into()),
            };
            send(&mut tenant, PlanUpdate::User(Crud::Update(patch)));
        }
        let mut tasks: Vec<Uuid> = Vec::new();
        for i in 0..per_plan {
            let id = ids.next();
            let new = NewTask {
                id,
                name: "Task".to_string(),
                parent: Parent::Plan,
            };
            send(&mut tenant, PlanUpdate::Task(Crud::Add(new)));
            let mut on: Vec<Dependency> = Vec::new();
            if tasks.is_empty() || random.below(10) == 0 {
                on.push(Dependency::ParentStart);
            }
            for _ in 0..random.below(3) {
                if let Some(t) = tasks.get(tasks.len().saturating_sub(1 + random.below(8) as usize))
                {
                    on.push(Dependency::Node(PlanNode::Task(*t)));
                }
            }
            if on.is_empty() {
                on.push(Dependency::ParentStart);
            }
            let worker = match random.below(3) {
                0 => Worker::Tag(tags[random.below(4) as usize]),
                _ => Worker::User(people[random.below(20) as usize]),
            };
            let hours = Workload::new_const([4, 8][random.below(2) as usize]);
            let constraint = (i % 40 == 39).then(|| {
                let date = today + chrono::Days::new(30 + random.below(60));
                TimeConstraint::Latest(date)
            });
            let patch = TaskPatch {
                id,
                name: None,
                parent: None,
                mode: None,
                duration: Some(1 + random.below(4) as u32),
                time_constraint: constraint,
                dependencies: Some(on),
                allocations: Some(HashMap::from([(worker, hours)])),
                affinities: None,
                actual_workers: None,
                persistent_state: None,
            };
            send(&mut tenant, PlanUpdate::Task(Crud::Update(patch)));
            tasks.push(id);
        }
        let end = ids.next();
        let new = NewMilestone {
            id: end,
            name: "End".to_string(),
            parent: Parent::Plan,
        };
        send(&mut tenant, PlanUpdate::Milestone(Crud::Add(new)));
        let after = tasks
            .iter()
            .rev()
            .take(5)
            .map(|t| Dependency::Node(PlanNode::Task(*t)));
        let patch = MilestonePatch {
            id: end,
            name: None,
            parent: None,
            time_constraint: None,
            dependencies: Some(after.collect()),
            persistent_state: None,
        };
        send(&mut tenant, PlanUpdate::Milestone(Crud::Update(patch)));
        let new = NewTarget {
            id: ids.next(),
            node: PlanNode::Milestone(end),
            priority: 0,
        };
        send(&mut tenant, PlanUpdate::Target(Crud::Add(new)));
    }

    // `PLINKO_BENCH_SAVE=1` also saves the tenant as `data/Bench`, to try the client on a lot of
    // work: run the server with PLINKO_TENANT=Bench and log in as root@localhost
    if std::env::var("PLINKO_BENCH_SAVE").is_ok() {
        let dir = plinko::store::tenant_dir("Bench").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let store = plinko::store::Store::open(&dir.join("tenant.db")).unwrap();
        store.create(&tenant).unwrap();
        store.set_password(root, "password123").unwrap();
    }
    let started = Instant::now();
    let state = scheduler::solve(&mut tenant, &scheduler::Hints::default()).state;
    println!("solve: {:?}", started.elapsed());
    let scheduled: usize = state.plans.values().map(|p| p.tasks.len()).sum();
    let unscheduled: usize = state.plans.values().map(|p| p.unscheduled.len()).sum();
    let worst = state
        .plans
        .values()
        .filter_map(|p| p.plan_end)
        .map(|e| e.delay())
        .max();
    println!(
        "{} plans, {scheduled} tasks scheduled, {unscheduled} not, worst delay {worst:?} days",
        state.plans.len()
    );
    let bytes = plinko::codec::encode(&state).unwrap();
    println!("schedule on the wire: {} KiB", bytes.len() / 1024);
}
// }}}
