//! Scores how well hard dates are kept on made up plans: `cargo run --release --example score [seeds] [plans]`.
//! 500 tasks in layers of dense dependencies share twelve people, a tenth of them have a Fixed or Latest
//! date. A reference schedule without the dates is made first and each date is set on or after where
//! the reference started the task, so a schedule that meets them all always exists. Lower is better.

// modules {{{
use std::{collections::HashMap, time::Instant};

use chrono::{DateTime, Days, NaiveDate};
use plinko::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler,
    types::{
        common::TimeConstraint,
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
        short_id::ShortId,
        tag::NewTag,
        target::NewTarget,
        task::{NewTask, TaskPatch, Worker},
        user::{NewPlanUser, PlanUserDataPatch},
        workload::Workload,
    },
};
use uuid::Uuid;
// }}}

struct Random(u64);

// impl Random {{{
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

// fn apply {{{
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

// Random supporting types {{{
/// What a made up tenant holds: the tasks of each plan in order, to put dates on
struct Made {
    tenant: Tenant,
    plans: Vec<(ShortId, Vec<Uuid>)>,
    people: Vec<Uuid>,
    /// The four tags of each plan, a person is in the tag that is their position modulo four
    tags: Vec<Vec<Uuid>>,
}
// }}}

// fn make {{{
/// A tenant of busy people with `plans` plans of about `total` tasks between them in layers,
/// each task waiting on one to three of the previous two layers, and a milestone now and then
/// that gathers several. `dates` says what hard date each task gets, if any.
fn make(seed: u64, plans: usize, total: usize, dates: &HashMap<Uuid, TimeConstraint>) -> Made {
    let today = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut tenant = Tenant::new(ShortId::new(), "Score".to_string(), today);
    let root = *tenant.users().keys().next().unwrap();
    let mut random = Random(seed * 7919 + 13);
    let mut next = 0u128;
    let mut id = move || {
        next += 1;
        Uuid::from_u128(next + seed as u128 * 1_000_000)
    };
    let people: Vec<Uuid> = (0..12)
        .map(|i| {
            let user = id();
            let new = NewTenantLoginUser {
                id: user,
                name: format!("User {i}"),
                email: format!("user{i}-{seed}@example.com"),
                role: Role::Member,
            };
            apply(&mut tenant, root, TenantUpdateType::User(Crud::Add(new)));
            user
        })
        .collect();
    let mut made = Vec::new();
    let mut all_tags: Vec<Vec<Uuid>> = Vec::new();
    for p in 0..plans {
        let plan = ShortId::from_str_checked(&format!("s{seed:02}p{p:04}")).unwrap();
        let new = NewPlan {
            id: plan,
            name: format!("Plan {p}"),
        };
        apply(
            &mut tenant,
            root,
            TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
        );
        let send = |tenant: &mut Tenant, update| {
            apply(
                tenant,
                root,
                TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update))),
            );
        };
        for person in &people {
            send(
                &mut tenant,
                PlanUpdate::User(Crud::Add(NewPlanUser { id: *person })),
            );
        }
        let tags: Vec<Uuid> = (0..4)
            .map(|_| {
                let tag = id();
                let new = NewTag {
                    id: tag,
                    name: "Tag".to_string(),
                };
                send(&mut tenant, PlanUpdate::Tag(Crud::Add(new)));
                tag
            })
            .collect();
        for (i, person) in people.iter().enumerate() {
            let patch = PlanUserDataPatch {
                id: *person,
                tags: Some([tags[i % 4]].into()),
            };
            send(&mut tenant, PlanUpdate::User(Crud::Update(patch)));
        }
        // the nodes so far, layer by layer, for what comes next to wait on
        let mut nodes: Vec<PlanNode> = Vec::new();
        let mut tasks: Vec<Uuid> = Vec::new();
        let count = total / plans;
        const LAYER: usize = 12;
        for i in 0..count {
            let task = id();
            send(
                &mut tenant,
                PlanUpdate::Task(Crud::Add(NewTask {
                    id: task,
                    name: format!("Task {i}"),
                    parent: Parent::Plan,
                })),
            );
            let layer = i / LAYER;
            let pool: &[PlanNode] = if layer == 0 {
                &[]
            } else {
                &nodes[(layer.saturating_sub(2) * LAYER).min(nodes.len())
                    ..(layer * LAYER).min(nodes.len())]
            };
            let mut on: Vec<Dependency> = Vec::new();
            if pool.is_empty() {
                on.push(Dependency::default());
            } else {
                for _ in 0..1 + random.below(3) {
                    let pick = pool[random.below(pool.len() as u64) as usize];
                    if !on.iter().any(|d| d.node == DependencyNode::Node(pick)) {
                        on.push(Dependency {
                            node: DependencyNode::Node(pick),
                            lag: 0,
                        });
                    }
                }
            }
            let worker = match random.below(4) {
                0 => Worker::Tag(tags[random.below(4) as usize]),
                _ => Worker::User(people[random.below(12) as usize]),
            };
            let hours = Workload::new([4, 8][random.below(2) as usize]).unwrap();
            let patch = TaskPatch {
                id: task,
                name: None,
                parent: None,
                mode: None,
                duration: Some(1 + random.below(6) as u32),
                time_constraint: dates.get(&task).copied(),
                dependencies: Some(on),
                allocations: Some(HashMap::from([(worker, hours)])),
                affinities: None,
                actual_workers: None,
                persistent_state: None,
            };
            send(&mut tenant, PlanUpdate::Task(Crud::Update(patch)));
            nodes.push(PlanNode::Task(task));
            tasks.push(task);
            if i % 40 == 39 {
                let milestone = id();
                let new = NewMilestone {
                    id: milestone,
                    name: "Gate".to_string(),
                    parent: Parent::Plan,
                };
                send(&mut tenant, PlanUpdate::Milestone(Crud::Add(new)));
                let gather = nodes.iter().rev().take(4).map(|n| Dependency {
                    node: DependencyNode::Node(*n),
                    lag: 0,
                });
                let patch = MilestonePatch {
                    id: milestone,
                    name: None,
                    parent: None,
                    time_constraint: None,
                    dependencies: Some(gather.collect()),
                    persistent_state: None,
                };
                send(&mut tenant, PlanUpdate::Milestone(Crud::Update(patch)));
                nodes.push(PlanNode::Milestone(milestone));
            }
        }
        let end = id();
        let new = NewMilestone {
            id: end,
            name: "End".to_string(),
            parent: Parent::Plan,
        };
        send(&mut tenant, PlanUpdate::Milestone(Crud::Add(new)));
        let after = nodes.iter().rev().take(6).map(|n| Dependency {
            node: DependencyNode::Node(*n),
            lag: 0,
        });
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
            id: id(),
            node: PlanNode::Milestone(end),
            priority: 0,
        };
        send(&mut tenant, PlanUpdate::Target(Crud::Add(new)));
        made.push((plan, tasks));
        all_tags.push(tags);
    }
    Made {
        tenant,
        plans: made,
        people,
        tags: all_tags,
    }
}
// }}}

// fn export {{{
/// Writes the plans and how they were scheduled as JSON for comparing with an exact solver
fn export(world: &Made, state: &plinko::scheduler::state::TenantVolatileState, path: &str) {
    let person = |u: &Uuid| world.people.iter().position(|p| p == u).unwrap();
    let node = |n: &PlanNode| match n {
        PlanNode::Task(id) | PlanNode::Milestone(id) => format!("\"{id}\""),
        _ => unreachable!(),
    };
    let mut tasks = Vec::new();
    let mut milestones = Vec::new();
    let mut ends = Vec::new();
    let mut starts = Vec::new();
    for (p, (plan_id, _)) in world.plans.iter().enumerate() {
        let plan = &world.tenant.plans()[plan_id];
        let deps = |list: &[Dependency]| {
            let nodes: Vec<String> = list
                .iter()
                .filter_map(|d| match &d.node {
                    DependencyNode::Node(n) => Some(node(n)),
                    _ => None,
                })
                .collect();
            nodes.join(",")
        };
        for (id, t) in plan.tasks() {
            let (worker, hours) = t.allocations().iter().next().unwrap();
            let who = match worker {
                Worker::User(u) => format!("\"user\":{}", person(u)),
                Worker::Tag(tag) => {
                    let index = world.tags[p].iter().position(|g| g == tag).unwrap();
                    format!("\"tag\":{index},\"plan\":{p}")
                }
                _ => unreachable!(),
            };
            let constraint = match t.time_constraint() {
                TimeConstraint::Fixed(d) => format!("[\"fixed\",\"{d}\"]"),
                TimeConstraint::Latest(d) => format!("[\"latest\",\"{d}\"]"),
                _ => "null".to_string(),
            };
            tasks.push(format!(
                "{{\"id\":\"{id}\",\"days\":{},\"hours\":{},{who},\"deps\":[{}],\"constraint\":{constraint}}}",
                t.duration(),
                hours.get(),
                deps(t.dependencies())
            ));
            if let Some(v) = state.plans[plan_id].tasks.get(id) {
                starts.push(format!("\"{id}\":\"{}\"", v.date_range.start()));
            }
        }
        for (id, m) in plan.milestones() {
            milestones.push(format!(
                "{{\"id\":\"{id}\",\"deps\":[{}]}}",
                deps(m.dependencies())
            ));
        }
        for t in plan.targets().values() {
            ends.push(node(&t.node()));
        }
    }
    let json = format!(
        "{{\"start\":\"2024-01-01\",\"tasks\":[{}],\"milestones\":[{}],\"ends\":[{}],\"heuristic\":{{{}}}}}",
        tasks.join(",\n"),
        milestones.join(",\n"),
        ends.join(","),
        starts.join(",")
    );
    std::fs::write(path, json).unwrap();
}
// }}}

// fn main {{{
fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let (seeds, plans) = (
        args.first().copied().unwrap_or(10),
        args.get(1).copied().unwrap_or(1),
    );
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let (mut dated, mut violated, mut late, mut worst, mut makespan, mut delay, mut seconds) =
        (0.0, 0.0, 0.0, 0.0f64, 0.0, 0.0, 0.0);
    for seed in 1..=seeds as u64 {
        // the reference schedule has no hard dates
        let mut plain = make(seed, plans, 500, &HashMap::new());
        scheduler::prepare(&mut plain.tenant);
        let reference = scheduler::solve_tenant(&plain.tenant);
        let mut random = Random(seed * 31 + 5);
        let mut dates: HashMap<Uuid, TimeConstraint> = HashMap::new();
        for (plan, tasks) in &plain.plans {
            for task in tasks {
                if random.below(10) != 0 {
                    continue;
                }
                let Some(first) = reference.plans[plan]
                    .tasks
                    .get(task)
                    .map(|t| t.date_range.start())
                else {
                    continue;
                };
                let constraint = if random.below(100) < 35 {
                    TimeConstraint::Fixed(first)
                } else {
                    TimeConstraint::Latest(first + Days::new(random.below(9)))
                };
                dates.insert(*task, constraint);
            }
        }
        dated += dates.len() as f64;
        let mut world = make(seed, plans, 500, &dates);
        scheduler::prepare(&mut world.tenant);
        let started = Instant::now();
        let state = scheduler::solve_tenant(&world.tenant);
        seconds += started.elapsed().as_secs_f64();
        if let Ok(dir) = std::env::var("EXPORT") {
            export(
                &world,
                &state,
                &format!("{dir}/instance_{plans}_{seed}.json"),
            );
        }
        let mut last = 0.0f64;
        for plan in state.plans.values() {
            for v in plan.constraint_violations.values() {
                if let TimeConstraint::Fixed(date) | TimeConstraint::Latest(date) = v.constraint {
                    let days = (v.scheduled - date).num_days() as f64;
                    violated += 1.0;
                    late += days;
                    worst = worst.max(days);
                }
            }
            if let Some(end) = plan.plan_end {
                last = last.max((end.forecast - start).num_days() as f64);
                delay += end.delay() as f64;
            }
        }
        makespan += last;
    }
    let n = seeds as f64;
    println!(
        "{seeds} seeds of {plans} plan(s), about {:.0} dated tasks of 500 in each",
        dated / n
    );
    println!("hard dates missed   {:.1}", violated / n);
    println!("days late in all    {:.1}", late / n);
    println!("worst miss (days)   {worst:.0}");
    println!("plan end (days)     {:.1}", makespan / n);
    println!("plan delay in all   {:.1}", delay / n);
    println!("seconds to schedule {:.2}", seconds / n);
}
// }}}
