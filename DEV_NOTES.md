# Development Notes

## Fixes/Requests

- [x] Can you change the application to dark mode?
- [x] I am still having trouble with in-progress tasks, right now in-progress tasks that have their start date in the past on my plan are all starting yesterday, even when I change the 'Actual Start' for the task. Can you have in-progress tasks be partially scheduled, where the scheduler places the start of the task in the past, fills in the allocation for the tasks as if they were being scheduled normally (but before any not started tasks are scheduled). For this, the actual start date acts as the earliest the allocation can begin for the task for the scheduler. I think basically I am asking for in-progress tasks to go back to being dynamically allocated tasks with a fixed start date. For rendering the gantt bar I still want the left of the bar to be on the actual start date (which takes priority over the first day of allocation) in the event that the allocation of the tasks means the first day of allocation is after the start date.
- [x] On the constraint violation it has the actual scheduled date, but the scheduled date is wrong, likely because after the constraint violation was calculated the task was shifted by some propogation event. Can you just remove the scheduled indication because the plan shows when it was scheduled for anyway
