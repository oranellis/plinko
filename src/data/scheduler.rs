use crate::data::{Dependency, NodeId, Plan, constraint};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

type NodeChain = Vec<NodeId>;

#[derive(Debug, Clone)]
pub enum SchedulerError {
    EmptyChain,
    MissingTaskAffinity {
        task_name: String,
        required_tags: HashSet<String>,
    },
    NoPathsToNode(NodeId),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SchedulerError::EmptyChain => write!(f, "expected content in the node chain"),
            SchedulerError::MissingTaskAffinity {
                task_name,
                required_tags,
            } => {
                let mut tags: Vec<&str> = required_tags.iter().map(String::as_str).collect();
                tags.sort_unstable();
                write!(
                    f,
                    "task \"{task_name}\" is not satisfied, needs the following tags: {}",
                    tags.join(", ")
                )
            }
            SchedulerError::NoPathsToNode(node_id) => {
                write!(f, "no path from plan start to node {node_id:?}")
            }
        }
    }
}

impl Plan {
    pub fn compute_time_optimised_plan(&self) -> Result<(), SchedulerError> {
        // Ok the procedure for creating the plan is as follows
        //
        // Create a structure which can store tasks across person schedules. This should flexibly
        // allow a task to be broken up by lack of time available in a person's schedule. I.e. a
        // long running task could be broken up by weekends, mid-week holidays, or quarter days.
        // This structure should be stored in plan.rs so need to make changes there.
        //
        // Check that all the nodes are somehow connected to the plan start, if not throw an error
        // for the node that violates this
        //
        // for every time constrained node, starting with the soonest first, insert all the nodes
        // from all the possible node chains (not inserting duplicate nodes) from the plan start
        // onwards, in descending length of chain, into the plan. If it is not possible to meet a
        // date constraint, return a SchedulerError.
        //
        // Once all the time constrained nodes have been inserted, insert all the nodes from all
        // the node chains from the start to the scheduler_target node. When inserting nodes for
        // this step and the previous one, nodes with the start before constraint can be pushed
        // back up to their target date.
        //
        // Once all the scheduler_target dependent nodes have been inserted, insert all the
        // remaining end nodes, in order of longest node chain to shortest (as with all the
        // previous steps). Insert these nodes such that they do not impact the date of the
        // scheduler_target node.
        //
        // Task insertion logic must work like this
        // Insert the task day by day to the assigned person in the next available workdays which
        // have workload capacity remaining. So for a task with 0.5 workload per calendar day, fill the days
        // in the person's schedule with 0.5 workload remaining from as early as possible onwards.
        // In instances where multiple people can complete a worker slot, assign it to the person
        // who can finish it first. If several people can finish it at the same time then assign it to the
        // first person in the people list.
        // If it is not possible to insert the task before a dependent already in the plan then
        // push the dependent back and add the task at the end (overlapping as much as possible
        // with existing tasks) until the task fits. When shifting tasks, propogate the effects
        // forward. There shouldn't be any changes which impact fixed date tasks but if there are
        // then return a SchedulerError. Note that 'latest' constrained tasks can be pushed back
        // but only up to their ascribed date.

        let node_reverse_adj_map = self.build_dependents_map();

        let time_constrained_nodes = self.get_time_constrained_nodes();

        Ok(())
    }

    /// Returns all nodes with `Fixed` or `Latest` constraints, sorted soonest-first.
    fn get_time_constrained_nodes(&self) -> Vec<NodeId> {
        let mut v: Vec<(NodeId, constraint::DateConstraint)> = self
            .tasks
            .iter()
            .filter_map(|(&id, task)| {
                task.constraint
                    .filter(|c| {
                        matches!(
                            c.kind,
                            constraint::ConstraintKind::Fixed | constraint::ConstraintKind::Latest
                        )
                    })
                    .map(|c| (NodeId::Task(id), c))
            })
            .chain(self.milestones.iter().filter_map(|(&id, milestone)| {
                milestone
                    .constraint
                    .filter(|c| {
                        matches!(
                            c.kind,
                            constraint::ConstraintKind::Fixed | constraint::ConstraintKind::Latest
                        )
                    })
                    .map(|c| (NodeId::Milestone(id), c))
            }))
            .collect();
        v.sort_by_key(|(_, c)| c.date);
        v.into_iter().map(|(id, _)| id).collect()
    }

    fn build_dependents_map(&self) -> HashMap<NodeId, Vec<NodeId>> {
        let mut map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (&task_id, task) in &self.tasks {
            let node = NodeId::Task(task_id);
            for dep in &task.dependencies {
                map.entry(dep.id).or_default().push(node);
            }
        }
        for (&milestone_id, milestone) in &self.milestones {
            let node = NodeId::Milestone(milestone_id);
            for dep in &milestone.dependencies {
                map.entry(dep.id).or_default().push(node);
            }
        }
        map
    }

    // fn get_priority_sorted_task_list_to_node(
    //     &self,
    //     node_id: NodeId,
    // ) -> Result<Vec<NodeId>, SchedulerError> {
    //     // check if there are any nodes not connected back to the root (make a helper function for
    //     // this and make a new error type to capture which node is not connected to the root) and
    //     // exit early
    //     //
    //     // get the list of tasks from the scheduler_target to the root, reverse all the paths so
    //     // they are from the root to the target
    //     //
    //     // then get the list of all the end nodes to the root, sort in order of longest calendar
    //     // path first then reverse so it is from the root to all the node ends
    //
    //     let sorted_paths = self.get_paths_to_node_sorted(node_id)?;
    //     let mut seen = HashSet::new();
    //     let sorted_task_list = sorted_paths
    //         .into_iter()
    //         .flatten()
    //         .filter(|node_id| seen.insert(*node_id))
    //         .collect();
    //
    //     Ok(sorted_task_list)
    // }
    //
    // fn get_priority_sorted_task_list_to_ends(&self) -> Result<Vec<NodeId>, SchedulerError> {
    //     let end_nodes = self.get_end_nodes();
    //     let mut all_paths_with_dur: Vec<(f32, NodeChain)> = end_nodes
    //         .iter()
    //         .map(|&node| self.get_all_paths_to_node(node))
    //         .collect::<Result<Vec<_>, _>>()?
    //         .into_iter()
    //         .flatten()
    //         .map(|p| (self.calculate_path_duration(&p), p))
    //         .collect();
    //
    //     all_paths_with_dur
    //         .sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    //
    //     let mut seen = HashSet::new();
    //     let sorted_task_list = all_paths_with_dur
    //         .into_iter()
    //         .flat_map(|(_, p)| p)
    //         .filter(|node_id| seen.insert(*node_id))
    //         .collect();
    //
    //     Ok(sorted_task_list)
    // }
    //
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
    ) -> Result<Vec<NodeChain>, SchedulerError> {
        let node_id = current_chain
            .iter()
            .last()
            .ok_or(SchedulerError::EmptyChain)?;

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
    fn get_critical_path_to_root(&self) -> NodeChain {
        let end_nodes = self.get_end_nodes();
        if end_nodes.is_empty() {
            return vec![NodeId::PlanStart];
        }

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

            if i + 1 < path.len() {
                let next_node = path[i + 1];

                let deps = self.get_dependencies(&current_node);
                if let Some(dep) = deps.iter().find(|d| d.id == next_node) {
                    total_days += dep.lag_days;
                }
            }
        }

        total_days
    }

    /// Returns all paths from PlanStart to `target` in arbitrary order.
    /// If `target` is `PlanStart`, returns a single chain `[PlanStart]`.
    /// Returns `Err(NoPathsToNode)` if the target has no path to `PlanStart`.
    fn get_all_paths_to_node(&self, target: NodeId) -> Result<Vec<NodeChain>, SchedulerError> {
        if matches!(target, NodeId::PlanStart) {
            return Ok(vec![vec![NodeId::PlanStart]]);
        }

        let paths: Vec<NodeChain> = self
            .get_all_paths_to_root(vec![target])
            .unwrap_or_default()
            .into_iter()
            .map(|mut p| {
                p.reverse();
                p
            })
            .collect();

        if paths.is_empty() {
            return Err(SchedulerError::NoPathsToNode(target));
        }

        Ok(paths)
    }

    /// Returns all paths from PlanStart to `target`, sorted by total duration
    /// (longest first). If `target` is `PlanStart`, returns a single chain
    /// containing only `PlanStart`. Returns `Err(NoPathsToNode)` if the target
    /// has no path back to `PlanStart`.
    fn get_paths_to_node_sorted(&self, target: NodeId) -> Result<Vec<NodeChain>, SchedulerError> {
        let mut paths_with_dur: Vec<(f32, NodeChain)> = self
            .get_all_paths_to_node(target)?
            .into_iter()
            .map(|p| (self.calculate_path_duration(&p), p))
            .collect();

        paths_with_dur
            .sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        Ok(paths_with_dur.into_iter().map(|(_, p)| p).collect())
    }

    /// Returns all nodes (tasks and milestones) that have no successors —
    /// i.e., nothing depends on them. These are the "end" or "leaf" nodes
    /// of the dependency graph.
    fn get_end_nodes(&self) -> Vec<NodeId> {
        let all_nodes: HashSet<NodeId> = self
            .tasks
            .keys()
            .map(|&id| NodeId::Task(id))
            .chain(self.milestones.keys().map(|&id| NodeId::Milestone(id)))
            .collect();

        let depended_upon: HashSet<NodeId> = self
            .tasks
            .values()
            .flat_map(|task| &task.dependencies)
            .chain(self.milestones.values().flat_map(|m| &m.dependencies))
            .map(|dep| dep.id)
            .collect();

        all_nodes.difference(&depended_upon).copied().collect()
    }

    /// Returns `Ok(())` if every placeholder worker slot on every task can be
    /// satisfied by at least one user in the plan.
    /// Specific slots are skipped — the user is already named.
    /// Returns the first unsatisfied placeholder's error otherwise.
    fn all_tasks_completable(&self) -> Result<(), SchedulerError> {
        use crate::data::task::WorkerSlot;
        let users: Vec<_> = self.users.values().collect();
        for task in self.tasks.values() {
            for slot in &task.workers {
                if let WorkerSlot::Placeholder { required_tags, .. } = slot
                    && !users.iter().any(|u| slot.is_satisfied_by(u))
                {
                    return Err(SchedulerError::MissingTaskAffinity {
                        task_name: task.name.clone(),
                        required_tags: required_tags.clone(),
                    });
                }
            }
        }
        Ok(())
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

    // ── get_paths_to_node_sorted Tests ───────────────────────────────────────

    // ── all_tasks_completable Tests ───────────────────────────────────────────

    #[test]
    fn all_completable_empty_plan() {
        let p = Plan::new("Empty");
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_task_with_no_workers() {
        let mut p = Plan::new("NoWorkers");
        p.add_task(Task::new("T", ""));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_task_with_only_specific_workers() {
        use crate::data::User;
        let mut p = Plan::new("Specific");
        let uid = p.add_user(User::new("Alice").with_tag("rust"));
        let mut task = Task::new("T", "");
        task.add_specific_worker(uid, 3.0);
        p.add_task(task);
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_placeholder_satisfied_by_one_user() {
        use crate::data::User;
        let mut p = Plan::new("Satisfied");
        let mut task = Task::new("T", "");
        task.add_placeholder_worker(["rust"], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag("rust"));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_fails_when_no_user_satisfies_placeholder() {
        use crate::data::User;
        let mut p = Plan::new("Unsatisfied");
        let mut task = Task::new("T", "");
        task.add_placeholder_worker(["rust"], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag("python"));
        let err = p.all_tasks_completable().unwrap_err();
        assert!(matches!(err, SchedulerError::MissingTaskAffinity { .. }));
        assert!(err.to_string().contains("rust"));
        assert!(err.to_string().contains("\"T\""));
    }

    #[test]
    fn all_completable_no_users_but_placeholder_required() {
        let mut p = Plan::new("NoUsers");
        let mut task = Task::new("T", "");
        task.add_placeholder_worker(["rust"], 3.0);
        p.add_task(task);
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn all_completable_partial_match_is_insufficient() {
        use crate::data::User;
        let mut p = Plan::new("Partial");
        let mut task = Task::new("T", "");
        task.add_placeholder_worker(["rust", "skia"], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag("rust")); // missing "skia"
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn all_completable_second_user_satisfies_placeholder() {
        use crate::data::User;
        let mut p = Plan::new("SecondUser");
        let mut task = Task::new("T", "");
        task.add_placeholder_worker(["rust"], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag("python"));
        p.add_user(User::new("Bob").with_tag("rust"));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_display_lists_tags_sorted() {
        use crate::data::User;
        let mut p = Plan::new("Display");
        let mut task = Task::new("Frontend", "");
        task.add_placeholder_worker(["typescript", "react"], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice")); // no tags
        let msg = p.all_tasks_completable().unwrap_err().to_string();
        assert!(msg.contains("\"Frontend\""));
        // Tags should appear sorted
        let react_pos = msg.find("react").unwrap();
        let ts_pos = msg.find("typescript").unwrap();
        assert!(react_pos < ts_pos);
    }

    #[test]
    fn all_completable_mixed_slots_both_must_pass() {
        use crate::data::User;
        let mut p = Plan::new("Mixed");
        let uid = p.add_user(User::new("Alice").with_tag("design"));
        let mut task = Task::new("T", "");
        task.add_specific_worker(uid, 2.0);
        task.add_placeholder_worker(["rust"], 3.0); // no rust user
        p.add_task(task);
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn paths_to_node_plan_start_returns_single_root_chain() {
        let p = Plan::new("Empty");
        let paths = p.get_paths_to_node_sorted(NodeId::PlanStart).unwrap();
        assert_eq!(paths, vec![vec![NodeId::PlanStart]]);
    }

    #[test]
    fn paths_to_node_disconnected_returns_error() {
        let mut p = Plan::new("Disconnected");
        let t = p.add_task(Task::new("T", "").with_duration(3.0));
        // T has no dependency on PlanStart — no path to root
        let err = p.get_paths_to_node_sorted(NodeId::Task(t)).unwrap_err();
        assert!(matches!(err, SchedulerError::NoPathsToNode(_)));
    }

    #[test]
    fn paths_to_node_single_path() {
        let mut p = Plan::new("Linear");
        let t1 = p.add_task(Task::new("T1", "").with_duration(3.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0));
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let paths = p.get_paths_to_node_sorted(NodeId::Task(t2)).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0],
            vec![NodeId::PlanStart, NodeId::Task(t1), NodeId::Task(t2)]
        );
    }

    #[test]
    fn paths_to_node_sorted_by_duration() {
        let mut p = Plan::new("MultiplePaths");
        let t1 = p.add_task(Task::new("T1", "").with_duration(10.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(2.0));
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0));

        // PlanStart -> T1 (10d) -> T3 = 11 days
        // PlanStart -> T2  (2d) -> T3 =  3 days
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let paths = p.get_paths_to_node_sorted(NodeId::Task(t3)).unwrap();
        assert_eq!(paths.len(), 2);

        // Longest first
        let dur0 = p.calculate_path_duration(&paths[0]);
        let dur1 = p.calculate_path_duration(&paths[1]);
        assert!((dur0 - 11.0).abs() < f32::EPSILON);
        assert!((dur1 - 3.0).abs() < f32::EPSILON);
        assert!(paths[0].contains(&NodeId::Task(t1)));
        assert!(paths[1].contains(&NodeId::Task(t2)));
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
