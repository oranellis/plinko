use std::collections::{HashMap, HashSet};

use chrono::{NaiveDate, Weekday};
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
        task::{NewTask, TaskPatch, Worker},
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
fn today_starts_at_the_plan_start_and_moves_by_update() {
    let mut plan = new_plan();
    assert_eq!(*plan.scheduler_data().today(), d(2024, 1, 1));
    let patch = PlanSchedulerDataPatch {
        start_date: None,
        today: Some(d(2024, 1, 9)),
        scheduler_target: None,
    };
    plan.apply_update(PlanUpdate::SchedulerData(patch)).unwrap();
    assert_eq!(*plan.scheduler_data().today(), d(2024, 1, 9));
    assert_eq!(*plan.scheduler_data().start_date(), d(2024, 1, 1));
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

fn add_affinity(plan: &mut Plan, weekly: [u8; 7]) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewAffinity {
        id,
        name: "Affinity".to_string(),
    };
    plan.apply_update(PlanUpdate::Affinity(Crud::Add(new)))
        .unwrap();
    let rule = NewRule {
        id: Uuid::now_v7(),
        start_day: d(2000, 1, 1),
        kind: RuleKind::Weekly(WeeklySchedule::new(weekly).unwrap()),
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
    let thursdays = add_affinity(&mut plan, [0, 0, 0, 8, 0, 0, 0]);
    let late_week = add_affinity(&mut plan, [0, 0, 0, 8, 8, 0, 0]);
    let weekend = add_affinity(&mut plan, [0, 0, 0, 0, 0, 8, 8]);

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

    let affinity = add_affinity(&mut plan, [8; 7]);
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
    let rules = RuleSetUpdate::WeekStart(Weekday::Sun);
    assert!(
        plan.apply_update(PlanUpdate::AffinityRules(affinity, rules))
            .is_err()
    );
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
