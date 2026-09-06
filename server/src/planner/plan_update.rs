use uuid::Uuid;

use crate::{
    planner::plan::PlanSchedulerData,
    types::{milestone::Milestone, tag::Tag, task::Task, user::User},
};

struct PlanUpdate {
    update_type: PlanUpdateType,
    time: chrono::DateTime<chrono::Utc>,
}

enum PlanUpdateType {
    Task(TaskUpdate),
    User(UserUpdate),
    Milestone(MilestoneUpdate),
    Tag(TagUpdate),
}

enum TaskUpdate {
    Add(Task),
    Update(Uuid, Task),
    Remove(Uuid),
}

enum MilestoneUpdate {
    Add(Milestone),
    Update(Uuid, Milestone),
    Remove(Uuid),
}

enum UserUpdate {
    Add(User),
    Update(Uuid, User),
    Remove(Uuid),
}

enum TagUpdate {
    Add(Tag),
    Update(Uuid, Tag),
    Remove(Uuid),
}

enum SchedulerDataUpdate {
    Update(PlanSchedulerData),
    Reset,
}
