use crate::types::{
    milestone::{Milestone, MilestoneUuid},
    tag::{Tag, TagUuid},
    task::{Task, TaskUuid},
    user::{User, UserUuid},
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
    Update(TaskUuid, Task),
    Remove(TaskUuid),
}

enum MilestoneUpdate {
    Add(Milestone),
    Update(MilestoneUuid, Milestone),
    Remove(MilestoneUuid),
}

enum UserUpdate {
    Add(User),
    Update(UserUuid, User),
    Remove(UserUuid),
}

enum TagUpdate {
    Add(Tag),
    Update(TagUuid, Tag),
    Remove(TagUuid),
}
