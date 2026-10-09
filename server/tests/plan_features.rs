use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use plinko::{
    availability::{
        rule::{NewRule, RuleKind, WeeklySchedule},
        rule_set::RuleSetUpdate,
    },
    planner::{plan::*, plan_update::PlanUpdate},
    types::{
        affinity::{AffinityPatch, NewAffinity},
        crud::Crud,
        data_block::{BlockContent, DataBlockPatch, NewDataBlock, TodoItem},
        dependency::{Dependency, DependencyNode, PlanNode},
        error::PlinkoResult,
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        short_id::ShortId,
        target::{NewTarget, TargetPatch},
        task::{Assignee, NewTask, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroupPatch},
        workload::Workload,
    },
};
use uuid::Uuid;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn new_plan() -> Plan {
    let new = NewPlan {
        id: ShortId::new(),
        name: "My Plan".to_owned(),
        start_date: d(2024, 1, 1),
    };
    Plan::from(new)
}

fn add_task(plan: &mut Plan, parent: Parent) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewTask {
        id,
        name: "Task".to_string(),
        parent,
    };
    plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
    id
}

fn task_patch(id: Uuid) -> TaskPatch {
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
        description: None,
        persistent_state: None,
    }
}

fn patch_task(plan: &mut Plan, patch: TaskPatch) -> PlinkoResult<()> {
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
}

fn depends_on(plan: &mut Plan, id: Uuid, nodes: &[DependencyNode]) {
    let dependencies = nodes
        .iter()
        .map(|&node| Dependency { node, lag: 0 })
        .collect();
    let patch = TaskPatch {
        dependencies: Some(dependencies),
        ..task_patch(id)
    };
    patch_task(plan, patch).unwrap();
}

fn task_node(id: Uuid) -> DependencyNode {
    DependencyNode::Node(PlanNode::Task(id))
}

#[test]
fn allocations_are_hours_per_day_and_may_use_placeholders() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    let workers = HashMap::from([(Worker::Placeholder, Workload::new(4).unwrap())]);
    let patch = TaskPatch {
        allocations: Some(workers),
        ..task_patch(task)
    };
    patch_task(&mut plan, patch).unwrap();
    assert_eq!(
        plan.tasks()[&task].allocations()[&Worker::Placeholder].get(),
        4
    );
}

#[test]
fn unscheduled_items_do_not_trace_to_the_plan_start() {
    let mut plan = new_plan();
    let (anchored, floating, dependent, mixed) = (
        add_task(&mut plan, Parent::PlanStart),
        add_task(&mut plan, Parent::PlanStart),
        add_task(&mut plan, Parent::PlanStart),
        add_task(&mut plan, Parent::PlanStart),
    );
    depends_on(&mut plan, anchored, &[DependencyNode::ParentStart]);
    depends_on(&mut plan, dependent, &[task_node(floating)]);
    // any anchored path is enough
    depends_on(
        &mut plan,
        mixed,
        &[task_node(floating), task_node(anchored)],
    );

    let expected = HashSet::from([PlanNode::Task(floating), PlanNode::Task(dependent)]);
    assert_eq!(plan.unscheduled(), expected);
}

#[test]
fn group_members_anchor_through_the_group_start() {
    let mut plan = new_plan();
    let group = Uuid::now_v7();
    let new = NewTaskGroup {
        id: group,
        name: "Group".to_string(),
        parent: Parent::PlanStart,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
        .unwrap();
    let member = add_task(&mut plan, Parent::Node(group));
    depends_on(&mut plan, member, &[DependencyNode::ParentStart]);
    let after = add_task(&mut plan, Parent::PlanStart);
    depends_on(
        &mut plan,
        after,
        &[DependencyNode::Node(PlanNode::TaskGroup(group))],
    );

    // a group with no dependencies never starts, so nothing in or after it is scheduled
    let all = HashSet::from([
        PlanNode::TaskGroup(group),
        PlanNode::Task(member),
        PlanNode::Task(after),
    ]);
    assert_eq!(plan.unscheduled(), all);

    let patch = TaskGroupPatch {
        id: group,
        name: None,
        dependencies: Some(vec![Dependency {
            node: DependencyNode::ParentStart,
            lag: 0,
        }]),
        parent: None,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Update(patch)))
        .unwrap();
    assert!(plan.unscheduled().is_empty());
}

#[test]
fn milestones_and_buffers_count_as_nodes_too() {
    let mut plan = new_plan();
    let new = NewMilestone {
        id: Uuid::now_v7(),
        name: "Milestone".to_string(),
        parent: Parent::PlanStart,
    };
    let milestone = PlanNode::Milestone(new.id);
    plan.apply_update(PlanUpdate::Milestone(Crud::Add(new)))
        .unwrap();
    let new = NewPlanBuffer {
        id: Uuid::now_v7(),
        parent: Parent::PlanStart,
        duration: 3,
    };
    let buffer = PlanNode::PlanBuffer(new.id);
    plan.apply_update(PlanUpdate::PlanBuffer(Crud::Add(new)))
        .unwrap();
    assert_eq!(plan.unscheduled(), HashSet::from([milestone, buffer]));
}

/// Allowed weekdays, Monday as 0
fn days(allowed: &[usize]) -> [bool; 7] {
    std::array::from_fn(|i| allowed.contains(&i))
}

fn add_affinity(plan: &mut Plan, weekly: [bool; 7]) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewAffinity {
        id,
        name: "Affinity".to_string(),
    };
    plan.apply_update(PlanUpdate::Affinity(Crud::Add(new)))
        .unwrap();
    let rule = NewRule {
        name: String::new(),
        stop_day: None,
        id: Uuid::now_v7(),
        start_day: d(2000, 1, 1),
        kind: RuleKind::Weekly(WeeklySchedule::new(weekly)),
    };
    let update = RuleSetUpdate::Rule(Crud::Add(rule));
    plan.apply_update(PlanUpdate::AffinityRules(id, update))
        .unwrap();
    id
}

fn link(plan: &mut Plan, task: Uuid, affinities: &[Uuid]) -> PlinkoResult<()> {
    let patch = TaskPatch {
        affinities: Some(affinities.iter().copied().collect()),
        ..task_patch(task)
    };
    patch_task(plan, patch)
}

#[test]
fn tasks_only_work_on_days_every_linked_affinity_allows() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    let thursdays = add_affinity(&mut plan, days(&[3]));
    let late_week = add_affinity(&mut plan, days(&[3, 4]));
    let weekend = add_affinity(&mut plan, days(&[5, 6]));

    let (thursday, friday) = (d(2024, 1, 4), d(2024, 1, 5));
    assert_eq!(plan.task_allows_day(&task, friday), Some(true));
    link(&mut plan, task, &[thursdays]).unwrap();
    assert_eq!(plan.task_allows_day(&task, thursday), Some(true));
    assert_eq!(plan.task_allows_day(&task, friday), Some(false));

    link(&mut plan, task, &[late_week, thursdays]).unwrap();
    assert_eq!(plan.task_allows_day(&task, friday), Some(false));
    assert_eq!(plan.task_allows_day(&task, thursday), Some(true));
    link(&mut plan, task, &[weekend, thursdays]).unwrap();
    assert_eq!(plan.task_allows_day(&task, thursday), Some(false));
    assert_eq!(plan.task_allows_day(&Uuid::now_v7(), thursday), None);
}

#[test]
fn affinities_must_exist_and_cannot_be_removed_while_linked() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    assert!(link(&mut plan, task, &[Uuid::now_v7()]).is_err());
    assert!(plan.tasks()[&task].affinities().is_empty());

    let affinity = add_affinity(&mut plan, [true; 7]);
    link(&mut plan, task, &[affinity]).unwrap();
    let remove = || PlanUpdate::Affinity(Crud::Remove(affinity));
    assert!(plan.apply_update(remove()).is_err());
    link(&mut plan, task, &[]).unwrap();

    let rename = AffinityPatch {
        id: affinity,
        name: Some("Renamed".to_string()),
    };
    plan.apply_update(PlanUpdate::Affinity(Crud::Update(rename)))
        .unwrap();
    assert_eq!(plan.affinities()[&affinity].name(), "Renamed");
    plan.apply_update(remove()).unwrap();
    let rules = RuleSetUpdate::Rule(Crud::Remove(Uuid::now_v7()));
    assert!(
        plan.apply_update(PlanUpdate::AffinityRules(affinity, rules))
            .is_err()
    );
}

fn add_target(plan: &mut Plan, node: PlanNode, priority: i32) -> PlinkoResult<Uuid> {
    let id = Uuid::now_v7();
    let new = NewTarget { id, node, priority };
    plan.apply_update(PlanUpdate::Target(Crud::Add(new)))
        .map(|()| id)
}

#[test]
fn targets_must_name_existing_nodes_and_block_their_removal() {
    let mut plan = new_plan();
    assert!(plan.targets().is_empty());
    assert!(add_target(&mut plan, PlanNode::Task(Uuid::now_v7()), 0).is_err());
    assert!(plan.targets().is_empty());

    let task = add_task(&mut plan, Parent::PlanStart);
    let milestone = Uuid::now_v7();
    let new = NewMilestone {
        id: milestone,
        name: "Done".to_string(),
        parent: Parent::PlanStart,
    };
    plan.apply_update(PlanUpdate::Milestone(Crud::Add(new)))
        .unwrap();
    let target = add_target(&mut plan, PlanNode::Task(task), -2).unwrap();
    assert_eq!(plan.targets()[&target].priority(), -2);

    let remove_task = || PlanUpdate::Task(Crud::Remove(task));
    assert!(plan.apply_update(remove_task()).is_err());

    // moving the target to another node frees the task, and the node must exist
    let patch = |node, priority| TargetPatch {
        id: target,
        node,
        priority,
    };
    let bad = patch(Some(PlanNode::Task(Uuid::now_v7())), None);
    assert!(
        plan.apply_update(PlanUpdate::Target(Crud::Update(bad)))
            .is_err()
    );
    assert_eq!(plan.targets()[&target].node(), PlanNode::Task(task));
    let moved = patch(Some(PlanNode::Milestone(milestone)), Some(5));
    plan.apply_update(PlanUpdate::Target(Crud::Update(moved)))
        .unwrap();
    assert_eq!(plan.targets()[&target].priority(), 5);
    plan.apply_update(remove_task()).unwrap();

    plan.apply_update(PlanUpdate::Target(Crud::Remove(target)))
        .unwrap();
    assert!(plan.targets().is_empty());
}

#[test]
fn plan_priority_defaults_to_zero_and_is_patched() {
    let mut plan = new_plan();
    assert_eq!(plan.scheduler_data().priority(), 0);
    let patch = PlanSchedulerDataPatch {
        start_date: None,
        priority: Some(-3),
    };
    plan.apply_update(PlanUpdate::SchedulerData(patch)).unwrap();
    assert_eq!(plan.scheduler_data().priority(), -3);
    assert_eq!(*plan.scheduler_data().start_date(), d(2024, 1, 1));
}

fn todo(items: &[(&str, bool)]) -> BlockContent {
    let items = items.iter().map(|&(text, done)| TodoItem {
        text: text.to_string(),
        done,
    });
    BlockContent::TodoList(items.collect())
}

#[test]
fn tasks_and_milestones_hold_data_blocks() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    let node = PlanNode::Task(task);
    let block = Uuid::now_v7();
    let new = NewDataBlock {
        id: block,
        title: "Checklist".to_string(),
        content: todo(&[("write", false)]),
    };
    plan.apply_update(PlanUpdate::Data(node, Crud::Add(new)))
        .unwrap();
    assert_eq!(plan.tasks()[&task].data().blocks().len(), 1);

    let patch = DataBlockPatch {
        id: block,
        title: None,
        content: Some(todo(&[("write", true)])),
    };
    plan.apply_update(PlanUpdate::Data(node, Crud::Update(patch)))
        .unwrap();
    let content = plan.tasks()[&task].data().blocks()[&block]
        .content()
        .clone();
    assert_eq!(content, todo(&[("write", true)]));
    plan.apply_update(PlanUpdate::Data(node, Crud::Remove(block)))
        .unwrap();
    assert!(plan.tasks()[&task].data().blocks().is_empty());

    let buffer = PlanNode::PlanBuffer(Uuid::now_v7());
    let new = NewDataBlock {
        id: Uuid::now_v7(),
        title: "Nope".to_string(),
        content: todo(&[]),
    };
    assert!(
        plan.apply_update(PlanUpdate::Data(buffer, Crud::Add(new)))
            .is_err()
    );
    let missing = PlanNode::Task(Uuid::now_v7());
    assert!(
        plan.apply_update(PlanUpdate::Data(missing, Crud::Remove(block)))
            .is_err()
    );
}

#[test]
fn a_new_affinity_allows_every_day() {
    let mut plan = new_plan();
    let affinity = Uuid::now_v7();
    let new = NewAffinity {
        id: affinity,
        name: "Anything".to_string(),
    };
    plan.apply_update(PlanUpdate::Affinity(Crud::Add(new)))
        .unwrap();
    let task = add_task(&mut plan, Parent::PlanStart);
    link(&mut plan, task, &[affinity]).unwrap();
    for day in d(2024, 1, 1).iter_days().take(14) {
        assert_eq!(plan.task_allows_day(&task, day), Some(true));
    }
}

#[test]
fn plan_users_hold_tags_by_id() {
    use plinko::types::{
        tag::NewTag,
        user::{NewPlanUser, PlanUserDataPatch},
    };
    let mut plan = new_plan();
    let (user, tag) = (Uuid::now_v7(), Uuid::now_v7());
    let new = NewPlanUser { id: user };
    plan.apply_update(PlanUpdate::User(Crud::Add(new))).unwrap();
    let hold = |tags: &[Uuid]| {
        PlanUpdate::User(Crud::Update(PlanUserDataPatch {
            id: user,
            tags: Some(tags.iter().copied().collect()),
        }))
    };
    assert!(
        plan.apply_update(hold(&[tag])).is_err(),
        "the tag must exist"
    );

    let new = NewTag {
        id: tag,
        name: "Designer".to_string(),
    };
    plan.apply_update(PlanUpdate::Tag(Crud::Add(new))).unwrap();
    plan.apply_update(hold(&[tag])).unwrap();
    assert!(plan.users()[&user].tags().contains(&tag));

    // a tag someone holds cannot be removed
    let remove = || PlanUpdate::Tag(Crud::Remove(tag));
    assert!(plan.apply_update(remove()).is_err());
    plan.apply_update(hold(&[])).unwrap();
    plan.apply_update(remove()).unwrap();
}

fn record(plan: &mut Plan, task: Uuid, workers: &[(Worker, Assignee)]) -> PlinkoResult<()> {
    let patch = TaskPatch {
        actual_workers: Some(workers.iter().copied().collect()),
        ..task_patch(task)
    };
    patch_task(plan, patch)
}

#[test]
fn the_person_who_did_the_work_is_recorded_per_requested_worker() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    let allocate = |plan: &mut Plan, slots: &[Worker]| {
        let allocations = slots
            .iter()
            .map(|w| (*w, Workload::new(4).unwrap()))
            .collect();
        let patch = TaskPatch {
            allocations: Some(allocations),
            ..task_patch(task)
        };
        patch_task(plan, patch)
    };
    let (slot, other) = (Worker::Placeholder, Worker::UserGroup(Uuid::now_v7()));
    let person = Assignee::User(Uuid::now_v7());

    // only workers the task asks for can have someone recorded
    assert!(record(&mut plan, task, &[(slot, person)]).is_err());
    allocate(&mut plan, &[slot]).unwrap();
    record(&mut plan, task, &[(slot, person)]).unwrap();
    assert_eq!(plan.tasks()[&task].actual_workers()[&slot], person);

    // the worker cannot be dropped from the task while someone is recorded for it
    assert!(allocate(&mut plan, &[]).is_err());
    assert!(record(&mut plan, task, &[(other, person)]).is_err());

    // and the record can be cleared
    record(&mut plan, task, &[]).unwrap();
    assert!(plan.tasks()[&task].actual_workers().is_empty());
    allocate(&mut plan, &[]).unwrap();
}

#[test]
fn task_and_group_durations_start_at_one_and_cannot_be_zero() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::PlanStart);
    assert_eq!(plan.tasks()[&task].duration(), 1);
    let zero = TaskPatch {
        duration: Some(0),
        ..task_patch(task)
    };
    assert!(patch_task(&mut plan, zero).is_err());
    let three = TaskPatch {
        duration: Some(3),
        ..task_patch(task)
    };
    patch_task(&mut plan, three).unwrap();
    assert_eq!(plan.tasks()[&task].duration(), 3);

    let group = Uuid::now_v7();
    let new = NewTaskGroup {
        id: group,
        name: "Group".to_string(),
        parent: Parent::PlanStart,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
        .unwrap();
}
