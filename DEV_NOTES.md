# Development Notes

## In Progress

I am rewriting the planning and task allocation to be structured better, exploiting more of the rust type system to encode logic.

- [ ] In particular I am working on the `allocation.rs`, rebuilding the types to represent how tasks are represented. `NodeAllocations` contains all of the dynamic data associated with the tasks and milestones, including the dates and hour allocations from the scheduling process, the state of tasks, and any additional dates such as extended in-progress tasks.

- [ ] In addition I am rewriting `plan.rs` rebuilding the functions required for the scheduling process from the ground up as a learning experience.

- [ ] Once complete I need to move on and rewrite `scheduler.rs` to compute the time optimised plan in the way I want, mostly maintaining the existing behaviour with easier to understand rust logic and structure. Also through rewriting it I will learn how Claude originally implemented it.

- [ ] Once all that is done I will review the queue logic and begin seperating out the UI application from the data side, eventually splitting the application into a server and client program.
