use chrono::NaiveDate;
use plinko::{
    planner::{
        plan::*,
        plan_update::{
            MilestoneUpdate, PlanBufferUpdate, PlanUpdate, TagUpdate, TaskGroupUpdate, TaskUpdate,
            UserUpdate,
        },
    },
    types::{
        error::PlinkoError,
        milestone::NewMilestone,
        plan_buffer::NewPlanBuffer,
        short_id::ShortId,
        tag::NewTag,
        task::NewTask,
        task_group::{GroupMember, NewTaskGroup, TaskGroupPatch},
    },
};
use uuid::Uuid;

#[test]
fn create_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
        start_date,
    });

    assert_eq!(plan.id().to_owned(), id);
    assert_eq!(plan.name(), "My Plan");
}

#[test]
fn add_task_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
        start_date,
    });
    let task = NewTask {
        id: Uuid::now_v7(),
        name: "Task Name".to_string(),
    };
    let task_id = task.id;
    let update = PlanUpdate::Task(TaskUpdate::Add(task));
    plan.apply_update(update).expect("should not error");
    assert!(plan.tasks().contains_key(&task_id));
}

#[test]
fn add_milestone_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
        start_date,
    });
    let milestone = NewMilestone {
        id: Uuid::now_v7(),
        name: "Milestone Name".to_string(),
    };
    let milestone_id = milestone.id;
    let update = PlanUpdate::Milestone(MilestoneUpdate::Add(milestone));
    plan.apply_update(update).expect("should not error");
    assert!(plan.milestones().contains_key(&milestone_id));
}

#[test]
fn add_user_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
        start_date,
    });
    let user_id = Uuid::now_v7(); // Fake Uuid, would normally match Tenant user Uuid
    let update = PlanUpdate::User(UserUpdate::Add(user_id));
    plan.apply_update(update).expect("should not error");
    assert!(plan.users().contains_key(&user_id));
}

#[test]
fn add_tag_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::from(NewPlan {
        id,
        name: "My Plan".to_owned(),
        start_date,
    });
    let tag = NewTag {
        id: Uuid::now_v7(),
        name: "John Doe".to_string(),
    };
    let tag_id = tag.id;
    let update = PlanUpdate::Tag(TagUpdate::Add(tag));
    plan.apply_update(update).expect("should not error");
    assert!(plan.tags().contains_key(&tag_id));
}

#[test]
fn add_task_group_to_plan() {
    let mut plan = new_plan();
    let group = NewTaskGroup {
        id: Uuid::now_v7(),
        name: "Group".to_string(),
    };
    let group_id = group.id;
    plan.apply_update(PlanUpdate::TaskGroup(TaskGroupUpdate::Add(group)))
        .expect("should not error");
    assert!(plan.task_groups().contains_key(&group_id));
    plan.apply_update(PlanUpdate::TaskGroup(TaskGroupUpdate::Remove(group_id)))
        .expect("should not error");
    assert!(plan.task_groups().is_empty());
}

#[test]
fn add_plan_buffer_to_plan() {
    let mut plan = new_plan();
    let buffer = NewPlanBuffer {
        id: Uuid::now_v7(),
        duration: 5,
    };
    let buffer_id = buffer.id;
    plan.apply_update(PlanUpdate::PlanBuffer(PlanBufferUpdate::Add(buffer)))
        .expect("should not error");
    assert_eq!(*plan.plan_buffers()[&buffer_id].duration(), 5);
}

fn new_plan() -> Plan {
    Plan::from(NewPlan {
        id: ShortId::new(),
        name: "My Plan".to_owned(),
        start_date: NaiveDate::from_ymd_opt(2000, 12, 31).unwrap(),
    })
}

fn add_group(plan: &mut Plan) -> Uuid {
    let id = Uuid::now_v7();
    let new = NewTaskGroup {
        id,
        name: "Group".to_string(),
    };
    plan.apply_update(PlanUpdate::TaskGroup(TaskGroupUpdate::Add(new)))
        .expect("should not error");
    id
}

fn set_contents(plan: &mut Plan, id: Uuid, contents: Vec<GroupMember>) -> Result<(), PlinkoError> {
    let patch = TaskGroupPatch {
        id,
        name: None,
        dependencies: None,
        contents: Some(contents),
    };
    plan.apply_update(PlanUpdate::TaskGroup(TaskGroupUpdate::Update(patch)))
}

#[test]
fn nested_groups_reject_cycles_and_double_membership() {
    let mut plan = new_plan();
    let (outer, inner) = (add_group(&mut plan), add_group(&mut plan));
    set_contents(&mut plan, outer, vec![GroupMember::TaskGroup(inner)]).unwrap();
    assert!(set_contents(&mut plan, inner, vec![GroupMember::TaskGroup(outer)]).is_err());
    assert!(set_contents(&mut plan, inner, vec![GroupMember::TaskGroup(inner)]).is_err());

    let other = add_group(&mut plan);
    assert!(set_contents(&mut plan, other, vec![GroupMember::TaskGroup(inner)]).is_err());
    assert!(set_contents(&mut plan, other, vec![GroupMember::Task(Uuid::now_v7())]).is_err());
}
