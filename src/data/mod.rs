//! Domain model for project plans, tasks, milestones, users, and scheduling.

pub mod allocation;
pub mod calendar;
pub mod constraint;
pub mod dates;
pub mod dependency;
pub mod ids;
pub mod milestone;
pub mod plan;
pub mod schedule;
pub mod scheduler;
pub mod storage;
pub mod tag;
pub mod task;
pub mod user;

pub use allocation::{
    ConstraintViolation, NodeAllocations, Status, TaskAllocation, TaskState, WorkSegment,
};
pub use calendar::CalendarOverrides;
pub use constraint::{ConstraintKind, DateConstraint};
pub use dependency::Dependency;
pub use ids::{MilestoneId, NodeId, TagId, TaskId, UserId};
pub use milestone::Milestone;
pub use plan::{DependencyError, Plan};
pub use schedule::{Weekday, WorkSchedule};
pub use storage::{Storage, StorageError};
pub use tag::Tag;
pub use task::{Task, WorkerSlot};
pub use user::{User, UserData};
