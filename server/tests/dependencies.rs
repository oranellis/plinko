// modules {{{
use std::collections::HashMap;

use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, error::PlinkoResult, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        parent::Parent,
        task::{NewTask, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroupPatch},
    },
    planner::resources::tag::NewTag,
    planner::{plan::*, plan_update::PlanUpdate},
};
use uuid::Uuid;
// }}}

// helpers {{{
fn new_plan() -> Plan {
    Plan::from(NewPlan {
        id: ShortId::new(),
        name: "My Plan".to_owned(),
    })
}

fn add_task(plan: &mut Plan, parent: Parent) -> Uuid {
    let id = Uuid::new_v4();
    let new = NewTask {
        id,
        name: "Task".to_string(),
        parent,
    };
    plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
    id
}

fn add_group(plan: &mut Plan, parent: Parent) -> Uuid {
    let id = Uuid::new_v4();
    let new = NewTaskGroup {
        id,
        name: "Group".to_string(),
        parent,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
        .unwrap();
    id
}

fn depend(nodes: &[PlanNode]) -> Vec<PlanNode> {
    nodes.to_vec()
}

fn task_patch(id: Uuid) -> TaskPatch {
    TaskPatch {
        id,
        name: None,
        parent: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: None,
    }
}

fn set_task_deps(plan: &mut Plan, id: Uuid, nodes: &[PlanNode]) -> PlinkoResult<()> {
    let patch = TaskPatch {
        dependencies: Some(depend(nodes)),
        ..task_patch(id)
    };
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
}

fn set_task_parent(plan: &mut Plan, id: Uuid, parent: Parent) -> PlinkoResult<()> {
    let patch = TaskPatch {
        parent: Some(parent),
        ..task_patch(id)
    };
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
}

fn task_node(id: Uuid) -> PlanNode {
    PlanNode::Task(id)
}

fn group_node(id: Uuid) -> PlanNode {
    PlanNode::TaskGroup(id)
}

fn remove_task(plan: &mut Plan, id: Uuid) -> PlinkoResult<()> {
    plan.apply_update(PlanUpdate::Task(Crud::Remove(id)))
}
// }}}

// dependency_on_missing_node_is_rejected {{{
#[test]
fn dependency_on_missing_node_is_rejected() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
    let missing = task_node(Uuid::new_v4());
    assert!(set_task_deps(&mut plan, task, &[missing]).is_err());
    assert!(plan.tasks()[&task].dependencies().is_empty());
}
// }}}

// dependency_cycles_are_rejected_and_rolled_back {{{
#[test]
fn dependency_cycles_are_rejected_and_rolled_back() {
    let mut plan = new_plan();
    let (a, b, c) = (
        add_task(&mut plan, Parent::Plan),
        add_task(&mut plan, Parent::Plan),
        add_task(&mut plan, Parent::Plan),
    );
    set_task_deps(&mut plan, b, &[task_node(a)]).unwrap();
    set_task_deps(&mut plan, c, &[task_node(b)]).unwrap();
    assert!(set_task_deps(&mut plan, a, &[task_node(c)]).is_err());
    assert!(set_task_deps(&mut plan, a, &[task_node(a)]).is_err());
    assert!(plan.tasks()[&a].dependencies().is_empty());
}
// }}}

// group_end_waits_on_members {{{
#[test]
fn group_end_waits_on_members() {
    let mut plan = new_plan();
    let group = add_group(&mut plan, Parent::Plan);
    let member = add_task(&mut plan, Parent::Node(group));
    let outside = add_task(&mut plan, Parent::Plan);

    // a member cannot wait on the end of its own group
    assert!(set_task_deps(&mut plan, member, &[group_node(group)]).is_err());
    // the group can wait on outside items, which then cannot wait on it back
    let patch = TaskGroupPatch {
        time_constraint: None,
        id: group,
        name: None,
        dependencies: Some(depend(&[task_node(outside)])),
        parent: None,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Update(patch)))
        .unwrap();
    // a member with no dependencies begins when the group does, so it is behind the group too
    assert!(set_task_deps(&mut plan, outside, &[task_node(member)]).is_err());
    assert!(set_task_deps(&mut plan, outside, &[group_node(group)]).is_err());
    // moving a dependent of the group end into the group is also a cycle
    let dependent = add_task(&mut plan, Parent::Plan);
    set_task_deps(&mut plan, dependent, &[group_node(group)]).unwrap();
    assert!(set_task_parent(&mut plan, dependent, Parent::Node(group)).is_err());
    assert_eq!(plan.tasks()[&dependent].parent(), Parent::Plan);
}
// }}}

// members_anchor_to_group_start {{{
#[test]
fn members_anchor_to_group_start() {
    let mut plan = new_plan();
    let group = add_group(&mut plan, Parent::Plan);
    let member = add_task(&mut plan, Parent::Node(group));

    // the group start cannot wait on a member that anchors to it
    let patch = TaskGroupPatch {
        time_constraint: None,
        id: group,
        name: None,
        dependencies: Some(depend(&[task_node(member)])),
        parent: None,
    };
    let update = PlanUpdate::TaskGroup(Crud::Update(patch));
    assert!(plan.apply_update(update).is_err());
}
// }}}

// referenced_items_cannot_be_removed {{{
#[test]
fn referenced_items_cannot_be_removed() {
    let mut plan = new_plan();
    let (a, b) = (
        add_task(&mut plan, Parent::Plan),
        add_task(&mut plan, Parent::Plan),
    );
    set_task_deps(&mut plan, b, &[task_node(a)]).unwrap();
    assert!(remove_task(&mut plan, a).is_err());
    set_task_deps(&mut plan, b, &[]).unwrap();
    remove_task(&mut plan, a).unwrap();
}
// }}}

// allocated_tags_must_exist_and_cannot_be_removed {{{
#[test]
fn allocated_tags_must_exist_and_cannot_be_removed() {
    let mut plan = new_plan();
    let task = add_task(&mut plan, Parent::Plan);
    let tag = Uuid::new_v4();
    let allocate = |plan: &mut Plan, workers: HashMap<Worker, Workload>| {
        let patch = TaskPatch {
            allocations: Some(workers),
            ..task_patch(task)
        };
        plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
    };
    let tagged = HashMap::from([(Worker::Tag(tag), Workload::new_const(1))]);
    assert!(allocate(&mut plan, tagged.clone()).is_err());

    let new = NewTag {
        id: tag,
        name: "Tag".to_string(),
    };
    plan.apply_update(PlanUpdate::Tag(Crud::Add(new))).unwrap();
    allocate(&mut plan, tagged).unwrap();
    let remove = PlanUpdate::Tag(Crud::Remove(tag));
    assert!(plan.apply_update(remove).is_err());
    allocate(&mut plan, HashMap::new()).unwrap();
    let remove = PlanUpdate::Tag(Crud::Remove(tag));
    plan.apply_update(remove).unwrap();
}
// }}}

// simplification {{{
/// Sets the dependencies, then applies what simplifying leaves to do, returning the update's items
fn set_and_simplify(plan: &mut Plan, id: Uuid, deps: Vec<PlanNode>) -> Vec<PlanNode> {
    let patch = TaskPatch {
        dependencies: Some(deps),
        ..task_patch(id)
    };
    let update = PlanUpdate::Task(Crud::Update(patch));
    let extra = {
        plan.apply_update(update.clone()).unwrap();
        plan.simplifications(&update)
    };
    let changed = extra.iter().filter_map(|u| u.rewires()).collect();
    for u in extra {
        plan.apply_update(u).unwrap();
    }
    changed
}

fn task_deps(plan: &Plan, id: Uuid) -> Vec<PlanNode> {
    plan.tasks()[&id].dependencies().clone()
}

#[test]
fn a_dependency_a_chain_implies_is_dropped() {
    let mut plan = new_plan();
    let (a, b, c) = (
        add_task(&mut plan, Parent::Plan),
        add_task(&mut plan, Parent::Plan),
        add_task(&mut plan, Parent::Plan),
    );
    let on = |id| PlanNode::Task(id);
    set_and_simplify(&mut plan, b, vec![on(a)]);
    // a to c is implied by a to b to c
    let changed = set_and_simplify(&mut plan, c, vec![on(a), on(b)]);
    assert_eq!(changed, [PlanNode::Task(c)]);
    assert_eq!(task_deps(&plan, c), [on(b)]);
}

#[test]
fn a_new_link_makes_dependents_redundant_too() {
    let mut plan = new_plan();
    let ids: Vec<Uuid> = (0..3).map(|_| add_task(&mut plan, Parent::Plan)).collect();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let on = |id| PlanNode::Task(id);
    set_and_simplify(&mut plan, c, vec![on(a), on(b)]);
    assert_eq!(task_deps(&plan, c).len(), 2);
    // b now waits on a, so c waiting on a as well says nothing new
    set_and_simplify(&mut plan, b, vec![on(a)]);
    assert_eq!(task_deps(&plan, c), [on(b)]);
}

#[test]
fn what_the_start_of_the_parent_already_waits_for_is_dropped() {
    let mut plan = new_plan();
    let before = add_task(&mut plan, Parent::Plan);
    let group = add_group(&mut plan, Parent::Plan);
    let on = |id| PlanNode::Task(id);
    plan.apply_update(PlanUpdate::set_dependencies(
        PlanNode::TaskGroup(group),
        vec![on(before)],
    ))
    .unwrap();
    // a member waits for its group to start, and the group for `before`
    let member = add_task(&mut plan, Parent::Node(group));
    set_and_simplify(&mut plan, member, vec![on(before)]);
    assert!(task_deps(&plan, member).is_empty());
}
// }}}

// removal {{{
fn remove_through(plan: &mut Plan, node: PlanNode) {
    for update in plan.removal(node) {
        plan.apply_update(update).unwrap();
    }
}

#[test]
fn removing_the_default_start_clears_it() {
    let mut plan = new_plan();
    let a = PlanNode::Task(add_task(&mut plan, Parent::Plan));
    let patch = plinko::planner::plan::PlanSchedulerDataPatch {
        priority: None,
        default_start: Some(Some(a)),
    };
    plan.apply_update(PlanUpdate::SchedulerData(patch)).unwrap();
    remove_through(&mut plan, a);
    assert_eq!(plan.scheduler_data().default_start(), None);
}

#[test]
fn removing_an_item_joins_what_was_before_to_what_was_after() {
    let mut plan = new_plan();
    let ids: Vec<Uuid> = (0..4).map(|_| add_task(&mut plan, Parent::Plan)).collect();
    let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
    let on = task_node;
    plan.apply_update(PlanUpdate::set_dependencies(PlanNode::Task(b), vec![on(a)]))
        .unwrap();
    plan.apply_update(PlanUpdate::set_dependencies(PlanNode::Task(c), vec![on(b)]))
        .unwrap();
    plan.apply_update(PlanUpdate::set_dependencies(PlanNode::Task(d), vec![on(b)]))
        .unwrap();
    remove_through(&mut plan, PlanNode::Task(b));
    assert!(!plan.tasks().contains_key(&b));
    assert_eq!(plan.tasks()[&c].dependencies(), &[on(a)]);
    assert_eq!(plan.tasks()[&d].dependencies(), &[on(a)]);
}

#[test]
fn removing_a_group_moves_its_members_up_and_what_waited_on_it_waits_on_them() {
    let mut plan = new_plan();
    let before = add_task(&mut plan, Parent::Plan);
    let group = add_group(&mut plan, Parent::Plan);
    plan.apply_update(PlanUpdate::set_dependencies(
        PlanNode::TaskGroup(group),
        vec![task_node(before)],
    ))
    .unwrap();
    let (m1, m2) = (
        add_task(&mut plan, Parent::Node(group)),
        add_task(&mut plan, Parent::Node(group)),
    );
    set_task_deps(&mut plan, m2, &[task_node(m1)]).unwrap();
    let after = add_task(&mut plan, Parent::Plan);
    set_task_deps(&mut plan, after, &[group_node(group)]).unwrap();

    remove_through(&mut plan, PlanNode::TaskGroup(group));
    assert!(plan.task_groups().is_empty());
    assert_eq!(plan.tasks()[&m1].parent(), Parent::Plan);
    // the start of the group was the end of the first task
    assert_eq!(plan.tasks()[&m1].dependencies(), &[task_node(before)]);
    assert_eq!(plan.tasks()[&m2].dependencies(), &[task_node(m1)]);
    let mut waits: Vec<_> = plan.tasks()[&after].dependencies().to_vec();
    waits.sort_by_key(|n| format!("{n:?}"));
    let mut members = vec![task_node(m1), task_node(m2)];
    members.sort_by_key(|n| format!("{n:?}"));
    assert_eq!(waits, members);
}
// }}}

// moving {{{
mod moving {
    use plinko::planner::plan_update::PlanUpdate;
    use plinko::{
        common::{crud::Crud, short_id::ShortId},
        planner::items::{
            dependency::PlanNode, parent::Parent, task::NewTask, task_group::NewTaskGroup,
        },
        planner::plan::*,
    };
    use uuid::Uuid;

    fn plan() -> Plan {
        Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        })
    }

    fn task(plan: &mut Plan) -> Uuid {
        let id = Uuid::new_v4();
        let new = NewTask {
            id,
            name: "T".to_string(),
            parent: Parent::Plan,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        id
    }

    fn on(id: Uuid) -> PlanNode {
        PlanNode::Task(id)
    }

    fn apply(plan: &mut Plan, updates: Vec<PlanUpdate>) {
        for u in updates {
            plan.apply_update(u).unwrap();
        }
    }

    #[test]
    fn into_and_out_of_a_group_carry_the_dependencies() {
        let mut plan = plan();
        let (before, x, after) = (task(&mut plan), task(&mut plan), task(&mut plan));
        let group = Uuid::new_v4();
        let new = NewTaskGroup {
            id: group,
            name: "G".to_string(),
            parent: Parent::Plan,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
            .unwrap();
        let set = |plan: &mut Plan, id, deps| {
            plan.apply_update(PlanUpdate::set_dependencies(PlanNode::Task(id), deps))
                .unwrap();
        };
        set(&mut plan, x, vec![on(before)]);
        set(&mut plan, after, vec![on(x)]);

        let into = plan.moving(PlanNode::Task(x), Parent::Node(group)).unwrap();
        apply(&mut plan, into);
        let group_node = PlanNode::TaskGroup(group);
        assert_eq!(plan.task_groups()[&group].dependencies(), &vec![on(before)]);
        assert!(plan.tasks()[&x].dependencies().is_empty());
        assert_eq!(plan.tasks()[&after].dependencies(), &vec![group_node]);

        let out = plan.moving(PlanNode::Task(x), Parent::Plan).unwrap();
        apply(&mut plan, out);
        assert_eq!(plan.tasks()[&x].parent(), Parent::Plan);
        assert_eq!(plan.tasks()[&x].dependencies(), &vec![on(before)]);
    }

    #[test]
    fn a_group_cannot_move_into_itself() {
        let mut plan = plan();
        let group = Uuid::new_v4();
        let new = NewTaskGroup {
            id: group,
            name: "G".to_string(),
            parent: Parent::Plan,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
            .unwrap();
        assert!(
            plan.moving(PlanNode::TaskGroup(group), Parent::Node(group))
                .is_err()
        );
    }
}
// }}}
