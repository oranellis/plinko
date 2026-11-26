// modules {{{
use plinko::{
    common::{crud::Crud, error::PlinkoError, short_id::ShortId},
    planner::items::{
        dependency::PlanNodeType,
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        plan_delay::NewPlanDelay,
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
    },
    planner::resources::{tag::NewTag, user::NewPlanUser},
    planner::{plan::*, plan_update::PlanUpdate},
};
use uuid::Uuid;
// }}}

// create_plan {{{
#[test]
fn create_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
    });

    assert_eq!(plan.id().to_owned(), id);
    assert_eq!(plan.name(), "My Plan");
}
// }}}

// add_task_to_plan {{{
#[test]
fn add_task_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
    });
    let task = NewTask {
        id: Uuid::new_v4(),
        name: "Task Name".to_string(),
        parent: Parent::Plan,
    };
    let task_id = task.id;
    let update = PlanUpdate::Task(Crud::Add(task));
    plan.apply_update(update).expect("should not error");
    assert!(plan.tasks().contains_key(&task_id));
}
// }}}

// add_milestone_to_plan {{{
#[test]
fn add_milestone_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
    });
    let milestone = NewMilestone {
        id: Uuid::new_v4(),
        name: "Milestone Name".to_string(),
        parent: Parent::Plan,
    };
    let milestone_id = milestone.id;
    let update = PlanUpdate::Milestone(Crud::Add(milestone));
    plan.apply_update(update).expect("should not error");
    assert!(plan.milestones().contains_key(&milestone_id));
}
// }}}

// add_user_to_plan {{{
#[test]
fn add_user_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
    });
    let user_id = Uuid::new_v4(); // Fake Uuid, would normally match Tenant user Uuid
    let update = PlanUpdate::User(Crud::Add(NewPlanUser { id: user_id }));
    plan.apply_update(update).expect("should not error");
    assert!(plan.users().contains_key(&user_id));
}
// }}}

// add_tag_to_plan {{{
#[test]
fn add_tag_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
    });
    let tag = NewTag {
        id: Uuid::new_v4(),
        name: "John Doe".to_string(),
    };
    let tag_id = tag.id;
    let update = PlanUpdate::Tag(Crud::Add(tag));
    plan.apply_update(update).expect("should not error");
    assert!(plan.tags().contains_key(&tag_id));
}
// }}}

// add_task_group_to_plan {{{
#[test]
fn add_task_group_to_plan() {
    let mut plan = new_plan();
    let group = NewTaskGroup {
        id: Uuid::new_v4(),
        name: "Group".to_string(),
        parent: Parent::Plan,
    };
    let group_id = group.id;
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(group)))
        .expect("should not error");
    assert!(plan.task_groups().contains_key(&group_id));
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Remove(group_id)))
        .expect("should not error");
    assert!(plan.task_groups().is_empty());
}
// }}}

// add_plan_buffer_to_plan {{{
#[test]
fn add_plan_buffer_to_plan() {
    let mut plan = new_plan();
    let buffer = NewPlanBuffer {
        id: Uuid::new_v4(),
        parent: Parent::Plan,
        duration: 5,
    };
    let buffer_id = buffer.id;
    plan.apply_update(PlanUpdate::PlanBuffer(Crud::Add(buffer)))
        .expect("should not error");
    assert_eq!(*plan.plan_buffers()[&buffer_id].duration(), 5);
}
// }}}

// add_plan_delay_to_plan {{{
#[test]
fn add_plan_delay_to_plan() {
    let mut plan = new_plan();
    let delay = NewPlanDelay {
        id: Uuid::new_v4(),
        name: Some("Cure".into()),
        parent: Parent::Plan,
        duration: 5,
    };
    let delay_id = delay.id;
    plan.apply_update(PlanUpdate::PlanDelay(Crud::Add(delay)))
        .expect("should not error");
    assert_eq!(*plan.plan_delays()[&delay_id].duration(), 5);
    assert_eq!(plan.plan_delays()[&delay_id].name(), Some("Cure"));
    plan.apply_update(PlanUpdate::PlanDelay(Crud::Remove(delay_id)))
        .expect("nothing depends on it");
    assert!(plan.plan_delays().is_empty());
}
// }}}

// nodes_are_found_by_id_whatever_their_kind {{{
#[test]
fn nodes_are_found_by_id_whatever_their_kind() {
    let mut plan = new_plan();
    let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    let group = ids[4];
    let updates = [
        PlanUpdate::TaskGroup(Crud::Add(NewTaskGroup {
            id: group,
            name: "Group".to_string(),
            parent: Parent::Plan,
        })),
        PlanUpdate::Task(Crud::Add(NewTask {
            id: ids[0],
            name: "Task".to_string(),
            parent: Parent::Node(group),
        })),
        PlanUpdate::Milestone(Crud::Add(NewMilestone {
            id: ids[1],
            name: "Milestone".to_string(),
            parent: Parent::Plan,
        })),
        PlanUpdate::PlanBuffer(Crud::Add(NewPlanBuffer {
            id: ids[2],
            parent: Parent::Plan,
            duration: 2,
        })),
        PlanUpdate::PlanDelay(Crud::Add(NewPlanDelay {
            id: ids[3],
            name: None,
            parent: Parent::Plan,
            duration: 2,
        })),
    ];
    for update in updates {
        plan.apply_update(update).unwrap();
    }

    let kinds = [
        PlanNodeType::Task,
        PlanNodeType::Milestone,
        PlanNodeType::PlanBuffer,
        PlanNodeType::PlanDelay,
        PlanNodeType::TaskGroup,
    ];
    for (id, kind) in ids.iter().zip(kinds) {
        let node = plan.node(id).expect("every added item is found");
        assert_eq!(node.id(), *id);
        assert_eq!(node.node_type(), kind);
        assert!(node.dependencies().is_empty());
    }
    assert_eq!(plan.node(&ids[0]).unwrap().parent(), Parent::Node(group));
    assert_eq!(plan.node(&ids[1]).unwrap().parent(), Parent::Plan);
    assert!(plan.node(&Uuid::new_v4()).is_none());
}
// }}}

// helpers {{{
fn new_plan() -> Plan {
    Plan::from(NewPlan {
        id: ShortId::new(),
        name: "My Plan".to_owned(),
    })
}

fn add_group(plan: &mut Plan, parent: Parent) -> Result<Uuid, PlinkoError> {
    let id = Uuid::new_v4();
    let new = NewTaskGroup {
        id,
        name: "Group".to_string(),
        parent,
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))?;
    Ok(id)
}

fn set_group_parent(plan: &mut Plan, id: Uuid, parent: Parent) -> Result<(), PlinkoError> {
    let patch = TaskGroupPatch {
        id,
        name: None,
        dependencies: None,
        parent: Some(parent),
    };
    plan.apply_update(PlanUpdate::TaskGroup(Crud::Update(patch)))
}
// }}}

// group_parents_reject_cycles_and_missing_groups {{{
#[test]
fn group_parents_reject_cycles_and_missing_groups() {
    let mut plan = new_plan();
    let outer = add_group(&mut plan, Parent::Plan).unwrap();
    let inner = add_group(&mut plan, Parent::Node(outer)).unwrap();
    let leaf = add_group(&mut plan, Parent::Node(inner)).unwrap();
    assert_eq!(plan.task_groups()[&inner].parent(), Parent::Node(outer));

    assert!(set_group_parent(&mut plan, outer, Parent::Node(leaf)).is_err());
    assert!(set_group_parent(&mut plan, inner, Parent::Node(inner)).is_err());
    assert!(add_group(&mut plan, Parent::Node(Uuid::new_v4())).is_err());

    // moving is a single parent change
    set_group_parent(&mut plan, leaf, Parent::Node(outer)).unwrap();
    assert_eq!(plan.task_groups()[&leaf].parent(), Parent::Node(outer));
}
// }}}

// tasks_move_between_groups_and_block_group_removal {{{
#[test]
fn tasks_move_between_groups_and_block_group_removal() {
    let mut plan = new_plan();
    let (a, b) = (
        add_group(&mut plan, Parent::Plan).unwrap(),
        add_group(&mut plan, Parent::Plan).unwrap(),
    );
    let task = NewTask {
        id: Uuid::new_v4(),
        name: "Task".to_string(),
        parent: Parent::Node(a),
    };
    let task_id = task.id;
    plan.apply_update(PlanUpdate::Task(Crud::Add(task)))
        .unwrap();

    let remove = |plan: &mut Plan, id| plan.apply_update(PlanUpdate::TaskGroup(Crud::Remove(id)));
    assert!(remove(&mut plan, a).is_err());

    let patch = TaskPatch {
        id: task_id,
        name: None,
        parent: Some(Parent::Node(b)),
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: None,
    };
    plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
        .unwrap();
    assert_eq!(plan.tasks()[&task_id].parent(), Parent::Node(b));
    remove(&mut plan, a).unwrap();
    assert!(remove(&mut plan, b).is_err());
}
// }}}
