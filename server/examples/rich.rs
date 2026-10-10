//! Scores the scheduler on larger made up tenants with part time people, half days, task
//! affinities, tags and user groups, and exports them for an exact solver:
//! `cargo run --release --example rich [seeds] [plans] [tasks per plan] [people]`.
//! `EXPORT=dir` writes each tenant and the scheduler's schedule as JSON, `VALIDATE=dir` reads
//! `solution_<plans>_<seed>.json` (every task's start and person) and checks the scheduler
//! accepts it exactly: every task is fixed on that day with that person and must land there.

use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use chrono::{DateTime, Days, NaiveDate};
use plinko::{
    availability::{
        rule::{NewRule, RuleKind, WeeklySchedule},
        rule_set::RuleSetUpdate,
    },
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role},
        user_group::NewTenantUserGroup,
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    scheduler,
    types::{
        affinity::NewAffinity,
        common::TimeConstraint,
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
        short_id::ShortId,
        tag::NewTag,
        target::NewTarget,
        task::{Assignee, NewTask, TaskPatch, Worker},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch},
        workload::Workload,
    },
};
use uuid::Uuid;

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

/// One task as made, for exporting
struct Info {
    id: Uuid,
    plan: usize,
    days: u32,
    hours: u8,
    worker: Worker,
    /// The people who could do it
    candidates: Vec<usize>,
    /// Monday to Friday, which days the task may be worked on
    allowed: [bool; 5],
    deps: Vec<Uuid>,
}

struct Made {
    tenant: Tenant,
    plans: Vec<ShortId>,
    people: Vec<Uuid>,
    /// Each person's hours Monday to Friday
    caps: Vec<[u8; 5]>,
    tasks: Vec<Info>,
    milestones: Vec<(Uuid, Vec<Uuid>)>,
    ends: Vec<Uuid>,
}

struct Opts {
    seed: u64,
    plans: usize,
    per_plan: usize,
    people: usize,
}

/// Plans with layers of dense dependencies sharing people some of whom work part time or half
/// days. `dates` sets hard dates on tasks and `pins` who does the tag and group work of a task.
fn make(opts: &Opts, dates: &HashMap<Uuid, TimeConstraint>, pins: &HashMap<Uuid, usize>) -> Made {
    let today = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut tenant = Tenant::new(ShortId::new(), "Rich".to_string(), today);
    let root = *tenant.users().keys().next().unwrap();
    let mut random = Random(opts.seed * 7919 + 13);
    let mut next = 0u128;
    let seed = opts.seed;
    let mut id = move || {
        next += 1;
        Uuid::from_u128(next + seed as u128 * 10_000_000)
    };
    let mut caps = Vec::new();
    let people: Vec<Uuid> = (0..opts.people)
        .map(|i| {
            let user = id();
            let new = NewTenantLoginUser {
                id: user,
                name: format!("User {i}"),
                email: format!("user{i}-{seed}@example.com"),
                role: Role::Member,
            };
            apply(&mut tenant, root, TenantUpdateType::User(Crud::Add(new)));
            // some work part time, some have a half day on Friday
            let week: [u8; 5] = match random.below(100) {
                0..12 => [4; 5],
                12..30 => [8, 8, 8, 8, 4],
                _ => [8; 5],
            };
            let hours = |h: u8| Workload::new(h).unwrap();
            let kind = RuleKind::Weekly(WeeklySchedule::new([
                hours(week[0]),
                hours(week[1]),
                hours(week[2]),
                hours(week[3]),
                hours(week[4]),
                hours(0),
                hours(0),
            ]));
            let rule = NewRule {
                name: String::new(),
                stop_day: None,
                id: id(),
                start_day: NaiveDate::from_ymd_opt(2000, 1, 3).unwrap(),
                kind,
            };
            apply(
                &mut tenant,
                root,
                TenantUpdateType::Availability(user, RuleSetUpdate::Rule(Crud::Add(rule))),
            );
            caps.push(week);
            user
        })
        .collect();
    // groups of people, the same in every plan
    let group_of = |i: usize| match i % 10 {
        0 | 1 => 0,
        2 | 3 => 1,
        4..=6 => 2,
        _ => 3,
    };
    let groups: Vec<Uuid> = (0..4)
        .map(|g| {
            let group = id();
            let members: HashSet<Uuid> = (0..opts.people)
                .filter(|i| group_of(*i) == g)
                .map(|i| people[i])
                .collect();
            let new = NewTenantUserGroup {
                id: group,
                name: format!("Group {g}"),
                parent: None,
                users: members,
            };
            apply(
                &mut tenant,
                root,
                TenantUpdateType::UserGroup(Crud::Add(new)),
            );
            group
        })
        .collect();
    const TAGS: usize = 8;
    let tags_of = |i: usize| -> Vec<usize> {
        let mut tags = vec![i % TAGS];
        if i.is_multiple_of(3) {
            tags.push((i * 3 + 1) % TAGS);
        }
        tags
    };
    // monday to friday masks of what an affinity allows
    let masks: [[bool; 5]; 3] = [
        [true, true, true, true, false],
        [false, true, true, true, false],
        [false, true, true, true, true],
    ];
    let mut plan_ids = Vec::new();
    let mut tasks = Vec::new();
    let mut milestones = Vec::new();
    let mut ends = Vec::new();
    for p in 0..opts.plans {
        let plan = ShortId::from_str_checked(&format!("r{seed:02}p{p:04}")).unwrap();
        let new = NewPlan {
            id: plan,
            name: format!("Plan {p}"),
            start_date: today,
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
        for group in &groups {
            send(
                &mut tenant,
                PlanUpdate::UserGroup(Crud::Add(NewPlanUserGroup { id: *group })),
            );
        }
        let tags: Vec<Uuid> = (0..TAGS)
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
                tags: Some(tags_of(i).into_iter().map(|t| tags[t]).collect()),
            };
            send(&mut tenant, PlanUpdate::User(Crud::Update(patch)));
        }
        let affinities: Vec<Uuid> = masks
            .iter()
            .map(|mask| {
                let affinity = id();
                send(
                    &mut tenant,
                    PlanUpdate::Affinity(Crud::Add(NewAffinity {
                        id: affinity,
                        name: "Days".to_string(),
                    })),
                );
                let allowed = |d: usize| mask.get(d).copied().unwrap_or(false);
                let kind = RuleKind::Weekly(WeeklySchedule::new([
                    allowed(0),
                    allowed(1),
                    allowed(2),
                    allowed(3),
                    allowed(4),
                    false,
                    false,
                ]));
                let rule = NewRule {
                    name: String::new(),
                    stop_day: None,
                    id: id(),
                    start_day: NaiveDate::from_ymd_opt(2000, 1, 3).unwrap(),
                    kind,
                };
                send(
                    &mut tenant,
                    PlanUpdate::AffinityRules(affinity, RuleSetUpdate::Rule(Crud::Add(rule))),
                );
                affinity
            })
            .collect();
        let mut nodes: Vec<PlanNode> = Vec::new();
        const LAYER: usize = 20;
        for i in 0..opts.per_plan {
            let task = id();
            send(
                &mut tenant,
                PlanUpdate::Task(Crud::Add(NewTask {
                    id: task,
                    name: format!("Task {i}"),
                    parent: Parent::PlanStart,
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
            let (worker, candidates, part_time) = match random.below(100) {
                0..50 => {
                    let who = random.below(opts.people as u64) as usize;
                    (Worker::User(people[who]), vec![who], caps[who] == [4; 5])
                }
                50..75 => {
                    let tag = random.below(TAGS as u64) as usize;
                    let members = (0..opts.people)
                        .filter(|i| tags_of(*i).contains(&tag))
                        .collect();
                    (Worker::Tag(tags[tag]), members, false)
                }
                _ => {
                    let g = random.below(4) as usize;
                    let members = (0..opts.people).filter(|i| group_of(*i) == g).collect();
                    (Worker::UserGroup(groups[g]), members, false)
                }
            };
            let hours = if part_time {
                4
            } else {
                [4, 8][random.below(2) as usize]
            };
            let affinity = (random.below(100) < 12).then(|| random.below(3) as usize);
            let days = 1 + random.below(6) as u32;
            let allowed = affinity.map_or([true; 5], |a| masks[a]);
            let mut allocations = HashMap::from([(worker, Workload::new(hours).unwrap())]);
            let mut recorded = HashMap::new();
            if let Some(person) = pins.get(&task) {
                recorded.insert(worker, Assignee::User(people[*person]));
            }
            if matches!(worker, Worker::User(_)) {
                recorded.clear();
            }
            let patch = TaskPatch {
                id: task,
                name: None,
                parent: None,
                mode: None,
                duration: Some(days),
                time_constraint: dates.get(&task).copied(),
                dependencies: Some(on.clone()),
                allocations: Some(std::mem::take(&mut allocations)),
                affinities: affinity.map(|a| HashSet::from([affinities[a]])),
                actual_workers: (!recorded.is_empty()).then_some(recorded),
                persistent_state: None,
            };
            send(&mut tenant, PlanUpdate::Task(Crud::Update(patch)));
            let deps = on
                .iter()
                .filter_map(|d| match d.node {
                    DependencyNode::Node(PlanNode::Task(t) | PlanNode::Milestone(t)) => Some(t),
                    _ => None,
                })
                .collect();
            tasks.push(Info {
                id: task,
                plan: p,
                days,
                hours,
                worker,
                candidates,
                allowed,
                deps,
            });
            nodes.push(PlanNode::Task(task));
            if i % 40 == 39 {
                let milestone = id();
                let new = NewMilestone {
                    id: milestone,
                    name: "Gate".to_string(),
                    parent: Parent::PlanStart,
                };
                send(&mut tenant, PlanUpdate::Milestone(Crud::Add(new)));
                let gather: Vec<Dependency> = nodes
                    .iter()
                    .rev()
                    .take(4)
                    .map(|n| Dependency {
                        node: DependencyNode::Node(*n),
                        lag: 0,
                    })
                    .collect();
                let patch = MilestonePatch {
                    id: milestone,
                    name: None,
                    parent: None,
                    time_constraint: None,
                    dependencies: Some(gather.clone()),
                    persistent_state: None,
                };
                send(&mut tenant, PlanUpdate::Milestone(Crud::Update(patch)));
                milestones.push((milestone, node_ids(&gather)));
                nodes.push(PlanNode::Milestone(milestone));
            }
        }
        let end = id();
        let new = NewMilestone {
            id: end,
            name: "End".to_string(),
            parent: Parent::PlanStart,
        };
        send(&mut tenant, PlanUpdate::Milestone(Crud::Add(new)));
        let after: Vec<Dependency> = nodes
            .iter()
            .rev()
            .take(6)
            .map(|n| Dependency {
                node: DependencyNode::Node(*n),
                lag: 0,
            })
            .collect();
        let patch = MilestonePatch {
            id: end,
            name: None,
            parent: None,
            time_constraint: None,
            dependencies: Some(after.clone()),
            persistent_state: None,
        };
        send(&mut tenant, PlanUpdate::Milestone(Crud::Update(patch)));
        milestones.push((end, node_ids(&after)));
        ends.push(end);
        let new = NewTarget {
            id: id(),
            node: PlanNode::Milestone(end),
            priority: 0,
        };
        send(&mut tenant, PlanUpdate::Target(Crud::Add(new)));
        plan_ids.push(plan);
    }
    Made {
        tenant,
        plans: plan_ids,
        people,
        caps,
        tasks,
        milestones,
        ends,
    }
}

fn node_ids(deps: &[Dependency]) -> Vec<Uuid> {
    deps.iter()
        .filter_map(|d| match d.node {
            DependencyNode::Node(PlanNode::Task(t) | PlanNode::Milestone(t)) => Some(t),
            _ => None,
        })
        .collect()
}

fn export(
    world: &Made,
    state: &scheduler::state::TenantVolatileState,
    dates: &HashMap<Uuid, TimeConstraint>,
    path: &str,
) {
    let list = |ids: &[Uuid]| {
        ids.iter()
            .map(|i| format!("\"{i}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut tasks = Vec::new();
    let mut starts = Vec::new();
    let mut finishes = Vec::new();
    let mut persons = Vec::new();
    for t in &world.tasks {
        let constraint = match dates.get(&t.id) {
            Some(TimeConstraint::Fixed(d)) => format!("[\"fixed\",\"{d}\"]"),
            Some(TimeConstraint::Latest(d)) => format!("[\"latest\",\"{d}\"]"),
            _ => "null".to_string(),
        };
        let candidates = t
            .candidates
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let allowed = t
            .allowed
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",");
        tasks.push(format!(
            "{{\"id\":\"{}\",\"plan\":{},\"days\":{},\"hours\":{},\"candidates\":[{candidates}],\"allowed\":[{allowed}],\"deps\":[{}],\"constraint\":{constraint}}}",
            t.id,
            t.plan,
            t.days,
            t.hours,
            list(&t.deps)
        ));
        let plan = &state.plans[&world.plans[t.plan]];
        if let Some(v) = plan.tasks.get(&t.id) {
            starts.push(format!("\"{}\":\"{}\"", t.id, v.date_range.start()));
            finishes.push(format!("\"{}\":\"{}\"", t.id, v.date_range.end()));
            if let Some(Assignee::User(u)) = v.workers.get(&t.worker) {
                let who = world.people.iter().position(|p| p == u).unwrap();
                persons.push(format!("\"{}\":{who}", t.id));
            }
        }
    }
    let milestones: Vec<String> = world
        .milestones
        .iter()
        .map(|(id, deps)| format!("{{\"id\":\"{id}\",\"deps\":[{}]}}", list(deps)))
        .collect();
    let caps: Vec<String> = world
        .caps
        .iter()
        .map(|c| format!("[{}]", c.map(|h| h.to_string()).join(",")))
        .collect();
    let json = format!(
        "{{\"start\":\"2024-01-01\",\"caps\":[{}],\"tasks\":[{}],\"milestones\":[{}],\"ends\":[{}],\"heuristic\":{{\"starts\":{{{}}},\"finishes\":{{{}}},\"people\":{{{}}}}}}}",
        caps.join(","),
        tasks.join(",\n"),
        milestones.join(",\n"),
        list(&world.ends),
        starts.join(","),
        finishes.join(","),
        persons.join(",")
    );
    std::fs::write(path, json).unwrap();
}

#[derive(Default)]
struct Score {
    dated: f64,
    violated: f64,
    late: f64,
    worst: f64,
    end: f64,
    delay: f64,
    seconds: f64,
}

fn score(
    state: &scheduler::state::TenantVolatileState,
    since: NaiveDate,
) -> (f64, f64, f64, f64, f64) {
    let (mut violated, mut late, mut worst, mut end, mut delay) = (0.0, 0.0, 0.0f64, 0.0f64, 0.0);
    for plan in state.plans.values() {
        for v in plan.constraint_violations.values() {
            if let TimeConstraint::Fixed(date) | TimeConstraint::Latest(date) = v.constraint {
                let days = (v.scheduled - date).num_days() as f64;
                violated += 1.0;
                late += days;
                worst = worst.max(days);
            }
        }
        if let Some(e) = plan.plan_end {
            end = end.max((e.forecast - since).num_days() as f64);
            delay += e.delay() as f64;
        }
    }
    (violated, late, worst, end, delay)
}

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let seeds = args.first().copied().unwrap_or(2);
    let plans = args.get(1).copied().unwrap_or(4);
    let per_plan = args.get(2).copied().unwrap_or(1000);
    let people = args.get(3).copied().unwrap_or(40);
    let since = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut total = Score::default();
    for seed in 1..=seeds as u64 {
        let opts = Opts {
            seed,
            plans,
            per_plan,
            people,
        };
        // the reference schedule has no hard dates
        let mut plain = make(&opts, &HashMap::new(), &HashMap::new());
        scheduler::prepare(&mut plain.tenant);
        let reference = scheduler::solve_tenant(&plain.tenant);
        let mut random = Random(seed * 31 + 5);
        let mut dates: HashMap<Uuid, TimeConstraint> = HashMap::new();
        for t in &plain.tasks {
            if random.below(10) != 0 {
                continue;
            }
            let plan = &reference.plans[&plain.plans[t.plan]];
            let Some(first) = plan.tasks.get(&t.id).map(|v| v.date_range.start()) else {
                continue;
            };
            let constraint = if random.below(100) < 35 {
                TimeConstraint::Fixed(first)
            } else {
                TimeConstraint::Latest(first + Days::new(random.below(9)))
            };
            dates.insert(t.id, constraint);
        }
        let mut world = make(&opts, &dates, &HashMap::new());
        scheduler::prepare(&mut world.tenant);
        let started = Instant::now();
        let state = scheduler::solve_tenant(&world.tenant);
        let seconds = started.elapsed().as_secs_f64();
        let unscheduled: usize = state.plans.values().map(|p| p.unscheduled.len()).sum();
        let (violated, late, worst, end, delay) = score(&state, since);
        println!(
            "seed {seed}: {} dated, {violated} missed, {late} days late, worst {worst}, plan end {end} days, {unscheduled} unscheduled, {seconds:.1}s",
            dates.len()
        );
        total.dated += dates.len() as f64;
        total.violated += violated;
        total.late += late;
        total.worst = total.worst.max(worst);
        total.end += end;
        total.delay += delay;
        total.seconds += seconds;
        if let Ok(dir) = std::env::var("EXPORT") {
            export(
                &world,
                &state,
                &dates,
                &format!("{dir}/rich_{plans}_{seed}.json"),
            );
        }
        if let Ok(dir) = std::env::var("VALIDATE") {
            validate(&opts, &dir, &world, &dates, since);
        }
    }
    let n = seeds as f64;
    println!(
        "average: {:.0} dated, {:.1} missed, {:.1} days late, plan end {:.1} days, {:.1}s",
        total.dated / n,
        total.violated / n,
        total.late / n,
        total.end / n,
        total.seconds / n
    );
}

/// Fixes every task on the day and with the person a solver chose and checks the scheduler puts
/// each exactly there without missing a date or leaving anything out
fn validate(
    opts: &Opts,
    dir: &str,
    world: &Made,
    dates: &HashMap<Uuid, TimeConstraint>,
    since: NaiveDate,
) {
    let path = format!("{dir}/solution_{}_{}.json", opts.plans, opts.seed);
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("  no {path}");
        return;
    };
    let solution: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut fixed: HashMap<Uuid, TimeConstraint> = HashMap::new();
    let mut pins: HashMap<Uuid, usize> = HashMap::new();
    let mut wanted: HashMap<Uuid, NaiveDate> = HashMap::new();
    for (id, day) in solution["starts"].as_object().unwrap() {
        let id: Uuid = id.parse().unwrap();
        let day: NaiveDate = day.as_str().unwrap().parse().unwrap();
        fixed.insert(id, TimeConstraint::Fixed(day));
        wanted.insert(id, day);
    }
    for (id, person) in solution["people"].as_object().unwrap() {
        pins.insert(id.parse().unwrap(), person.as_u64().unwrap() as usize);
    }
    // how well the solver's days keep the dates the plans really have
    let (mut kept_missed, mut kept_late) = (0, 0);
    for (id, constraint) in dates {
        if let TimeConstraint::Fixed(date) | TimeConstraint::Latest(date) = constraint {
            let late = (wanted[id] - *date).num_days();
            if late > 0 {
                kept_missed += 1;
                kept_late += late;
            }
        }
    }
    println!("  the solver's days miss {kept_missed} real dates, {kept_late} days late in all");
    let mut checked = make(opts, &fixed, &pins);
    scheduler::prepare(&mut checked.tenant);
    let state = scheduler::solve_tenant(&checked.tenant);
    let mut moved = 0;
    let mut missing = 0;
    let mut worst = 0;
    for t in &world.tasks {
        let plan = &state.plans[&checked.plans[t.plan]];
        match plan.tasks.get(&t.id) {
            Some(v) if v.date_range.start() == wanted[&t.id] => {}
            Some(v) => {
                moved += 1;
                worst = worst.max((v.date_range.start() - wanted[&t.id]).num_days());
            }
            None => missing += 1,
        }
    }
    let (violated, _, _, end, _) = score(&state, since);
    let last = state
        .plans
        .values()
        .flat_map(|p| p.tasks.values())
        .map(|t| (t.date_range.end() - since).num_days())
        .max()
        .unwrap_or(0);
    println!("  last task ends day {last}");
    println!(
        "  checked {}: {moved} tasks not on the solver's day (worst {worst} days late), {missing} unscheduled, {violated} dates missed, plan end {end} days",
        path
    );
}
