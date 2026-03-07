//! Domain model for project plans, tasks, milestones, users, and scheduling.
//!
//! The central entity is [`Plan`], which owns collections of [`Task`]s,
//! [`Milestone`]s, and [`User`]s together with their work schedules and
//! calendar exceptions.  Dependency edges are stored on each task/milestone
//! and validated for cycles on every mutation.  Computed start dates live
//! separately in [`StartDates`] so they can be recomputed without touching
//! task definitions.
//!
//! Persistence is handled by [`Storage`], which saves versioned JSON snapshots
//! under `$XDG_DATA_HOME/<binary>/plans/<plan-uuid>/`.

pub mod calendar;
pub mod constraint;
pub mod dates;
pub mod dependency;
pub mod ids;
pub mod milestone;
pub mod plan;
pub mod schedule;
pub mod storage;
pub mod task;
pub mod user;

pub use calendar::CalendarOverrides;
pub use constraint::{ConstraintKind, DateConstraint};
pub use dates::StartDates;
pub use dependency::Dependency;
pub use ids::{DependencyId, MilestoneId, TaskId, UserId};
pub use milestone::Milestone;
pub use plan::{DependencyError, Plan};
pub use storage::{Storage, StorageError};
pub use schedule::{Weekday, WorkSchedule};
pub use task::{Task, TaskStatus};
pub use user::User;
