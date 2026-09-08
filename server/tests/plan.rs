use chrono::{DateTime, NaiveDate};
use plinko::{
    planner::{
        plan::*,
        plan_update::{
            MilestoneUpdate, PlanUpdate, PlanUpdateType, TagUpdate, TaskUpdate, UserUpdate,
        },
    },
    types::{milestone::Milestone, short_id::ShortId, tag::Tag, task::Task, user::User},
};

#[test]
fn create_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let plan = Plan::new(id, "My Plan".to_owned(), start_date);

    assert_eq!(plan.id().to_owned(), id);
    assert_eq!(plan.name(), "My Plan");
}

#[test]
fn add_task_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::new(id, "My Plan".to_owned(), start_date);
    let task = Task::new("Task Name".to_string()).expect("error not expected");
    let task_id = *task.id();
    let update = PlanUpdate {
        update_type: PlanUpdateType::Task(TaskUpdate::Add(task)),
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
    };
    plan.apply_update(update).expect("should not error");
    assert!(plan.tasks().contains_key(&task_id));
}

#[test]
fn add_milestone_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::new(id, "My Plan".to_owned(), start_date);
    let milestone = Milestone::new("Milestone Name".to_string());
    let milestone_id = *milestone.id();
    let update = PlanUpdate {
        update_type: PlanUpdateType::Milestone(MilestoneUpdate::Add(milestone)),
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
    };
    plan.apply_update(update).expect("should not error");
    assert!(plan.milestones().contains_key(&milestone_id));
}

#[test]
fn add_user_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::new(id, "My Plan".to_owned(), start_date);
    let user = User::new("John Doe".to_string());
    let user_id = *user.id();
    let update = PlanUpdate {
        update_type: PlanUpdateType::User(UserUpdate::Add(user)),
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
    };
    plan.apply_update(update).expect("should not error");
    assert!(plan.users().contains_key(&user_id));
}

#[test]
fn add_tag_to_plan() {
    let id = ShortId::from_str_checked("ABCDEFGH").expect("should always be valid");
    let start_date = NaiveDate::from_ymd_opt(2000, 12, 31).unwrap();
    let mut plan = Plan::new(id, "My Plan".to_owned(), start_date);
    let tag = Tag::new("John Doe".to_string());
    let tag_id = *tag.id();
    let update = PlanUpdate {
        update_type: PlanUpdateType::Tag(TagUpdate::Add(tag)),
        time: DateTime::from_timestamp(978307200, 0).expect("time should be fine"),
    };
    plan.apply_update(update).expect("should not error");
    assert!(plan.tags().contains_key(&tag_id));
}
