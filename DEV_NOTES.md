# Development Notes

## In Progress

I am rewriting the planning and task allocation to be structured better, exploiting more of the rust type system to encode logic.

- [ ] In particular I am working on the `allocation.rs`, rebuilding the types to represent how tasks are represented. `NodeAllocations` contains all of the dynamic data associated with the tasks and milestones, including the dates and hour allocations from the scheduling process, the state of tasks, and any additional dates such as extended in-progress tasks.

- [ ] In addition I am rewriting `plan.rs` rebuilding the functions required for the scheduling process from the ground up as a learning experience.

- [ ] Once complete I need to move on and rewrite `scheduler.rs` to compute the time optimised plan in the way I want, mostly maintaining the existing behaviour with easier to understand rust logic and structure. Also through rewriting it I will learn how Claude originally implemented it.

- [ ] Once all that is done I will review the queue logic and begin seperating out the UI application from the data side, eventually splitting the application into a server and client program.


## Current bugs/requests

- cannot create a task with a state other than not started
- people are not sorted by tag order in the users list
- edit task checking does not error and ask the user to enter the start (and end) dates as required when status is not 'not-started'
- workers plus bar hover lights up under behind the calendar select for end date
- Add a forward dependents list on the task and milestone create/edit screen
- dependency arrows clip over items
- milestones never schedule before today, even if all their dependencies are completed in the past
- milestone colours are wrong, they should only be based on the status of the dependencies
- add task/milestone information hover in the bottom left corner for status information, including who is assigned to the task
- restructure how items are placed on the gantt chart, I want them to be grouped more by dependency chain, items that are close on the dependency node graph should be close on the gantt chart. Milestones should be close to the top if possible
- add a strict mode selector for task workload allocations; when the allocation is strict the task must have the exact hours per day dictated by workload/duration per person.
- When rebuilting the plan some tasks are placed in a different order if all the permuatations do not affect the plan, can you fix this so the tasks are always scheduled in the same order if permutations do not affect any key dates?
