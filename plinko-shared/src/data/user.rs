use crate::data::{
    WorkSchedule,
    ids::{TagId, UserId},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub tags: HashSet<TagId>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl User {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: UserId::new(),
            name: name.into(),
            tags: HashSet::new(),
        }
    }

    pub fn with_tag(mut self, tag_id: TagId) -> Self {
        self.tags.insert(tag_id);
        self
    }

    pub fn add_tag(&mut self, tag_id: TagId) {
        self.tags.insert(tag_id);
    }

    pub fn remove_tag(&mut self, tag_id: &TagId) {
        self.tags.remove(tag_id);
    }

    pub fn has_tag(&self, tag_id: &TagId) -> bool {
        self.tags.contains(tag_id)
    }
}
// }}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    pub user: User,
    pub schedule: Option<WorkSchedule>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl UserData {
    pub fn new(user: User) -> Self {
        Self {
            user,
            schedule: None,
        }
    }

    pub fn new_with_schedule(user: User, schedule: WorkSchedule) -> Self {
        Self {
            user,
            schedule: Some(schedule),
        }
    }

    pub fn with_schedule(mut self, work_schedule: WorkSchedule) -> UserData {
        self.schedule = Some(work_schedule);
        self
    }

    pub fn user_mut(&mut self) -> &mut User {
        &mut self.user
    }

    pub fn schedule_mut(&mut self) -> &mut WorkSchedule {
        self.schedule.get_or_insert_with(WorkSchedule::weekdays)
    }
}
// }}}
