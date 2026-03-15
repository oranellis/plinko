use crate::data::TaskId;

pub struct WorkSegment {}

pub struct FixedTaskAllocation {
    task_id: TaskId,
}

pub struct DynamicTaskAllocation {}

pub enum TaskAllocation {}

pub struct NodeAllocations {}
