// modules {{{
use std::collections::HashMap;

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
        dependency::PlanNode,
        error::PlinkoResult,
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        short_id::ShortId,
        target::{NewTarget, TargetPatch},
        task::{Assignee, NewTask, TaskPatch, Worker},
        task_group::NewTaskGroup,
        workload::Workload,
    },
};
use uuid::Uuid;
// }}}

// helpers {{{
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn new_plan() -> Plan {
    let new = NewPlan {
        id: ShortId::new(),
        name: "My Plan".to_owned(),
    };
    Plan::from(new)
}
// }}}

// fn add_task {{{
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
// }}}

// fn task_patch {{{
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
        persistent_state: None,
    }
}
// }}}

// fn patch_task {{{
fn patch_task(plan: &mut Plan, patch: TaskPatch) -> PlinkoResult<()> {
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
}
// }}}

// fn allocations_are_hours_per_day_and_may_use_placeholders {{{
#[test]
fn allocations_are_hours_per_day_and_may_use_placeholders() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
// }}}

// fn days {{{
/// Allowed weekdays, Monday as 0
fn days(allowed: &[usize]) -> [bool; 7] {
    std::array::from_fn(|i| allowed.contains(&i))
}
// }}}

// fn add_affinity {{{
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
// }}}

// fn link {{{
fn link(plan: &mut Plan, task: Uuid, affinities: &[Uuid]) -> PlinkoResult<()> {
    let patch = TaskPatch {
        affinities: Some(affinities.iter().copied().collect()),
        ..task_patch(task)
    };
    patch_task(plan, patch)
}
// }}}

// fn tasks_only_work_on_days_every_linked_affinity_allows {{{
#[test]
fn tasks_only_work_on_days_every_linked_affinity_allows() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
// }}}

// fn affinities_must_exist_and_cannot_be_removed_while_linked {{{
#[test]
fn affinities_must_exist_and_cannot_be_removed_while_linked() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
// }}}

// fn add_target {{{
fn add_target(plan: &mut Plan, node: PlanNode, priority: i32) -> PlinkoResult<Uuid> {
    let id = Uuid::now_v7();
    let new = NewTarget { id, node, priority };
    plan.apply_update(PlanUpdate::Target(Crud::Add(new)))
        .map(|()| id)
}
// }}}

// fn targets_must_name_existing_nodes_and_block_their_removal {{{
#[test]
fn targets_must_name_existing_nodes_and_block_their_removal() {
    let mut plan = new_plan();
    assert!(plan.targets().is_empty());
    assert!(add_target(&mut plan, PlanNode::Task(Uuid::now_v7()), 0).is_err());
    assert!(plan.targets().is_empty());

    let task = add_task(&mut plan, Parent::Plan);
    let milestone = Uuid::now_v7();
    let new = NewMilestone {
        id: milestone,
        name: "Done".to_string(),
        parent: Parent::Plan,
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
// }}}

// fn plan_priority_defaults_to_zero_and_is_patched {{{
#[test]
fn plan_priority_defaults_to_zero_and_is_patched() {
    let mut plan = new_plan();
    assert_eq!(plan.scheduler_data().priority(), 0);
    let patch = PlanSchedulerDataPatch {
        priority: Some(-3),
        default_start: None,
    };
    plan.apply_update(PlanUpdate::SchedulerData(patch)).unwrap();
    assert_eq!(plan.scheduler_data().priority(), -3);
}
// }}}

// fn the_default_start_is_an_existing_node_that_stays_while_it_is_set {{{
#[test]
fn the_default_start_is_an_existing_node_that_stays_while_it_is_set() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
    let set = |node| {
        PlanUpdate::SchedulerData(PlanSchedulerDataPatch {
            priority: None,
            default_start: Some(node),
        })
    };
    assert!(plan.scheduler_data().default_start().is_none());
    assert!(
        plan.apply_update(set(Some(PlanNode::Task(Uuid::now_v7()))))
            .is_err()
    );
    plan.apply_update(set(Some(PlanNode::Task(task)))).unwrap();
    assert_eq!(
        plan.scheduler_data().default_start(),
        Some(PlanNode::Task(task))
    );
    assert!(
        plan.apply_update(PlanUpdate::Task(Crud::Remove(task)))
            .is_err()
    );
    plan.apply_update(set(None)).unwrap();
    assert!(plan.scheduler_data().default_start().is_none());
    plan.apply_update(PlanUpdate::Task(Crud::Remove(task)))
        .unwrap();
}
// }}}

// fn todo {{{
fn todo(items: &[(&str, bool)]) -> BlockContent {
    let items = items.iter().map(|&(text, done)| TodoItem {
        text: text.to_string(),
        done,
    });
    BlockContent::TodoList(items.collect())
}
// }}}

// fn tasks_and_milestones_hold_data_blocks {{{
#[test]
fn tasks_and_milestones_hold_data_blocks() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
// }}}

// fn a_new_affinity_allows_every_day {{{
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
    let task = add_task(&mut plan, Parent::Plan);
    link(&mut plan, task, &[affinity]).unwrap();
    for day in d(2024, 1, 1).iter_days().take(14) {
        assert_eq!(plan.task_allows_day(&task, day), Some(true));
    }
}
// }}}

// fn plan_users_hold_tags_by_id {{{
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
// }}}

// fn record {{{
fn record(plan: &mut Plan, task: Uuid, workers: &[(Worker, Assignee)]) -> PlinkoResult<()> {
    let patch = TaskPatch {
        actual_workers: Some(workers.iter().copied().collect()),
        ..task_patch(task)
    };
    patch_task(plan, patch)
}
// }}}

// fn the_person_who_did_the_work_is_recorded_per_requested_worker {{{
#[test]
fn the_person_who_did_the_work_is_recorded_per_requested_worker() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
// }}}

// fn task_and_group_durations_start_at_one_and_cannot_be_zero {{{
#[test]
fn task_and_group_durations_start_at_one_and_cannot_be_zero() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
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
        parent: Parent::Plan,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
        .unwrap();
}
// }}}

// fn tasks_milestones_and_groups_have_a_description_and_data_buffers_have_neither {{{
#[test]
fn tasks_milestones_and_groups_have_a_description_and_data_buffers_have_neither() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
    let milestone = Uuid::now_v7();
    let new = NewMilestone {
        id: milestone,
        name: "M".to_string(),
        parent: Parent::Plan,
    };
    plan.apply_update(PlanUpdate::Milestone(Crud::Add(new)))
        .unwrap();
    let group = Uuid::now_v7();
    let new = NewTaskGroup {
        id: group,
        name: "G".to_string(),
        parent: Parent::Plan,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
        .unwrap();
    let buffer = Uuid::now_v7();
    let new = NewPlanBuffer {
        id: buffer,
        parent: Parent::Plan,
        duration: 2,
    };
    plan.apply_update(PlanUpdate::PlanBuffer(Crud::Add(new)))
        .unwrap();

    let describe = |plan: &mut Plan, node, text: Option<&str>| {
        plan.apply_update(PlanUpdate::Describe(node, text.map(str::to_string)))
    };
    describe(&mut plan, PlanNode::Task(task), Some("a task")).unwrap();
    describe(
        &mut plan,
        PlanNode::Milestone(milestone),
        Some("a milestone"),
    )
    .unwrap();
    describe(&mut plan, PlanNode::TaskGroup(group), Some("a group")).unwrap();
    assert_eq!(plan.tasks()[&task].description(), Some("a task"));
    assert_eq!(
        plan.milestones()[&milestone].description(),
        Some("a milestone")
    );
    assert_eq!(plan.task_groups()[&group].description(), Some("a group"));
    // an empty description is no description
    describe(&mut plan, PlanNode::Task(task), Some("")).unwrap();
    assert_eq!(plan.tasks()[&task].description(), None);
    assert!(describe(&mut plan, PlanNode::PlanBuffer(buffer), Some("no")).is_err());
    assert!(describe(&mut plan, PlanNode::Task(Uuid::now_v7()), Some("no")).is_err());

    // data blocks go on groups as well, and not on buffers
    let block = || NewDataBlock {
        id: Uuid::now_v7(),
        title: "Todo".to_string(),
        content: BlockContent::TodoList(Vec::new()),
    };
    let on = |node| PlanUpdate::Data(node, Crud::Add(block()));
    plan.apply_update(on(PlanNode::TaskGroup(group))).unwrap();
    assert_eq!(plan.task_groups()[&group].data().blocks().len(), 1);
    assert!(plan.apply_update(on(PlanNode::PlanBuffer(buffer))).is_err());
}
// }}}
