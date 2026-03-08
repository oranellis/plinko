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

    /// Returns the critical path from an end node to the root (PlanStart).
    /// The critical path is the longest path in terms of total duration
    /// (task durations + dependency lags).
    pub fn get_critical_path_to_root(&self) -> NodeChain {
        let end_nodes = self.get_end_nodes();
        if end_nodes.is_empty() {
            return vec![NodeId::PlanStart];
        }

        // Get all paths from all end nodes to root
        let all_paths: Vec<NodeChain> = end_nodes
            .iter()
            .flat_map(|end_node| {
                self.get_all_paths_to_root(vec![*end_node])
                    .unwrap_or_default()
            })
            .collect();

        if all_paths.is_empty() {
            return vec![NodeId::PlanStart];
        }

        // Find the path with maximum duration (the critical path)
        all_paths
            .into_iter()
            .max_by(|a, b| {
                let dur_a = self.calculate_path_duration(a);
                let dur_b = self.calculate_path_duration(b);
                dur_a
                    .partial_cmp(&dur_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap() // Safe because we checked all_paths is not empty
    }

    /// Calculates the total duration of a path in working days.
    /// Sums up task durations (milestones = 0) and dependency lags.
    fn calculate_path_duration(&self, path: &NodeChain) -> f32 {
        let mut total_days = 0.0;

        for i in 0..path.len() {
            let current_node = path[i];

            // Add the duration of the current node
            match current_node {
                NodeId::Task(id) => {
                    if let Some(task) = self.tasks.get(&id) {
                        total_days += task.effective_duration_days();
                    }
                }
                NodeId::Milestone(_) | NodeId::PlanStart => {
                    // Milestones and PlanStart have zero duration
                }
            }

            // If there's a next node in the path, add the lag from the dependency
            if i + 1 < path.len() {
                let next_node = path[i + 1];

                // Find the dependency edge from current to next and add its lag
                let deps = self.get_dependencies(&current_node);
                if let Some(dep) = deps.iter().find(|d| d.id == next_node) {
                    total_days += dep.lag_days;
                }
            }
        }

        total_days
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

    // ── Critical Path Tests ───────────────────────────────────────────────────

    #[test]
    fn critical_path_empty_plan() {
        let p = Plan::new("Empty");
        let path = p.get_critical_path_to_root();
        assert_eq!(path, vec![NodeId::PlanStart]);
    }

    #[test]
    fn critical_path_single_task() {
        let mut p = Plan::new("Single");
        let t = p.add_task(Task::new("T", "").with_duration(5.0));
        p.add_task_dependency(t, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(path, vec![NodeId::Task(t), NodeId::PlanStart]);
    }

    #[test]
    fn critical_path_linear_chain() {
        let mut p = Plan::new("Chain");
        let t1 = p.add_task(Task::new("T1", "").with_duration(3.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0));
        let t3 = p.add_task(Task::new("T3", "").with_duration(2.0));

        // PlanStart -> T1 -> T2 -> T3
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(
            path,
            vec![
                NodeId::Task(t3),
                NodeId::Task(t2),
                NodeId::Task(t1),
                NodeId::PlanStart
            ]
        );

        // Duration should be 3 + 5 + 2 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_lag() {
        let mut p = Plan::new("WithLag");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(3.0));

        // PlanStart -> T1 -> (2 day lag) -> T2
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::with_lag(NodeId::Task(t1), 2.0))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Duration should be 5 + 2 (lag) + 3 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_lead() {
        let mut p = Plan::new("WithLead");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(3.0));

        // PlanStart -> T1 -> (1 day lead/overlap) -> T2
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::with_lead(NodeId::Task(t1), 1.0))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Duration should be 5 + (-1) (lead) + 3 = 7 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 7.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_chooses_longest_path() {
        let mut p = Plan::new("MultiplePaths");
        let t1 = p.add_task(Task::new("T1", "").with_duration(10.0)); // Long path
        let t2 = p.add_task(Task::new("T2", "").with_duration(2.0)); // Short path
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0)); // Convergence point

        // PlanStart -> T1 (10d) -> T3 (1d) = 11 days total
        // PlanStart -> T2 (2d)  -> T3 (1d) = 3 days total
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Should choose the longer path through T1
        assert!(path.contains(&NodeId::Task(t1)));
        assert!(!path.contains(&NodeId::Task(t2)));

        let duration = p.calculate_path_duration(&path);
        assert!((duration - 11.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_milestone() {
        let mut p = Plan::new("WithMilestone");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let m = p.add_milestone(Milestone::new("Launch", ""));

        // PlanStart -> T1 -> Milestone (0 duration)
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(
            path,
            vec![NodeId::Milestone(m), NodeId::Task(t1), NodeId::PlanStart]
        );

        // Duration should be 5 (milestone has 0 duration)
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_diamond_pattern() {
        let mut p = Plan::new("Diamond");
        let t1 = p.add_task(Task::new("T1", "").with_duration(2.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0)); // Longer branch
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0)); // Shorter branch
        let t4 = p.add_task(Task::new("T4", "").with_duration(3.0));

        //       T1 (2d)
        //      /      \
        //   T2 (5d)  T3 (1d)
        //      \      /
        //       T4 (3d)
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t2)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t3)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Should go through T2 (longer): T4 -> T2 -> T1 -> PlanStart
        assert!(path.contains(&NodeId::Task(t2)));
        assert!(!path.contains(&NodeId::Task(t3)));

        // Duration: 3 + 5 + 2 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }
}
