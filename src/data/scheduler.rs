use crate::data::{Dependency, NodeId, Plan};
use std::{collections::HashSet, fmt};

type NodeChain = Vec<NodeId>;

#[derive(Debug, Clone)]
struct EmptyChainError;

impl fmt::Display for EmptyChainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "expected content in the node chain")
    }
}

impl Plan {
    fn run_scheduler() {}

    fn get_dependencies(&self, node_id: &NodeId) -> &[Dependency] {
        match node_id {
            NodeId::Task(task_id) => {
                &self
                    .tasks
                    .get(task_id)
                    .unwrap_or_else(|| panic!("cannot find expected node {node_id:?}"))
                    .dependencies
            }
            NodeId::Milestone(milestone_id) => {
                &self
                    .milestones
                    .get(milestone_id)
                    .unwrap_or_else(|| panic!("cannot find expected node {node_id:?}"))
                    .dependencies
            }
            NodeId::PlanStart => &[],
        }
    }

    fn get_all_paths_to_root(
        &self,
        current_chain: NodeChain,
    ) -> Result<Vec<NodeChain>, EmptyChainError> {
        let node_id = current_chain.iter().last().ok_or(EmptyChainError)?;

        if matches!(node_id, NodeId::PlanStart) {
            return Ok(vec![current_chain]);
        }

        self.get_dependencies(node_id)
            .iter()
            .try_fold(vec![], |mut acc, dependency| {
                let mut new_chain = current_chain.clone();
                new_chain.push(dependency.id);
                acc.extend(self.get_all_paths_to_root(new_chain)?);
                Ok(acc)
            })
    }

    pub fn get_critical_path_to_root(&self) -> NodeChain {
        self.get_all_paths_to_root(vec![self.get_end_nodes().last().unwrap().clone()])
        todo!("implement getting the critical path to root using the number of days in each task and the lag/lead to build the chain with number of days")
    }

    /// Returns all nodes (tasks and milestones) that have no successors —
    /// i.e., nothing depends on them. These are the "end" or "leaf" nodes
    /// of the dependency graph.
    pub fn get_end_nodes(&self) -> Vec<NodeId> {
        // Collect all existing nodes
        let all_nodes: HashSet<NodeId> = self
            .tasks
            .keys()
            .map(|&id| NodeId::Task(id))
            .chain(self.milestones.keys().map(|&id| NodeId::Milestone(id)))
            .collect();

        // Collect all nodes that appear as dependencies (are depended upon)
        let depended_upon: HashSet<NodeId> = self
            .tasks
            .values()
            .flat_map(|task| &task.dependencies)
            .chain(self.milestones.values().flat_map(|m| &m.dependencies))
            .map(|dep| dep.id)
            .collect();

        // End nodes = all nodes that exist but aren't depended upon by anything
        all_nodes.difference(&depended_upon).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Dependency, Milestone, Task};

    #[test]
    fn get_end_nodes_empty_plan() {
        let p = Plan::new("Empty");
        let end_nodes = p.get_end_nodes();
        assert!(end_nodes.is_empty());
    }

    #[test]
    fn get_end_nodes_single_task_no_dependencies() {
        let mut p = Plan::new("Single");
        let t = p.add_task(Task::new("T", ""));
        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t)));
    }

    #[test]
    fn get_end_nodes_linear_chain() {
        let mut p = Plan::new("Chain");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));

        // T1 -> T2 -> T3 (T3 is the end node)
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t3)));
    }

    #[test]
    fn get_end_nodes_multiple_end_nodes() {
        let mut p = Plan::new("Multiple");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));

        // T1 -> T2, T1 -> T3 (both T2 and T3 are end nodes)
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Task(t2)));
        assert!(end_nodes.contains(&NodeId::Task(t3)));
    }

    #[test]
    fn get_end_nodes_with_milestone() {
        let mut p = Plan::new("WithMilestone");
        let t1 = p.add_task(Task::new("T1", ""));
        let m = p.add_milestone(Milestone::new("Launch", ""));

        // T1 -> Milestone (milestone is the end node)
        p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Milestone(m)));
    }

    #[test]
    fn get_end_nodes_mixed_tasks_and_milestones() {
        let mut p = Plan::new("Mixed");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let m1 = p.add_milestone(Milestone::new("M1", ""));
        let m2 = p.add_milestone(Milestone::new("M2", ""));

        // T1 -> M1, T2 -> M2 (M1 and M2 are end nodes)
        p.add_milestone_dependency(m1, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_milestone_dependency(m2, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Milestone(m1)));
        assert!(end_nodes.contains(&NodeId::Milestone(m2)));
    }

    #[test]
    fn get_end_nodes_plan_start_dependency_doesnt_affect_result() {
        let mut p = Plan::new("PlanStart");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));

        // Both depend on PlanStart, but both are still end nodes
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Task(t1)));
        assert!(end_nodes.contains(&NodeId::Task(t2)));
    }

    #[test]
    fn get_end_nodes_diamond_dependency() {
        let mut p = Plan::new("Diamond");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));
        let t4 = p.add_task(Task::new("T4", ""));

        //     T1
        //    /  \
        //   T2  T3
        //    \  /
        //     T4
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t2)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t3)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t4)));
    }
}
