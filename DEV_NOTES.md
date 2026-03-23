# Development Notes

## Fixes/Requests

- [ ] When the plan allocates the workload for a task where there are more than one people working on the task, allocations between users must be on the same day, for exmaple, if person a has 0.5 days per day and person b has 0.25 days per day on a task over 5 days, the scheduler must find the first 5 days where both person a and person b have the required number of hours free. This applies to only strict mode tasks as it does not really make sense in relaxed mode.
- [ ] Can the allocation screen task names be stuck to the left edge rather than overlapping the tasks, such that they stay in the same position on the screen even when the calendar scrolls? since there is only one task per row.
- [ ] On the allocation screen can you use the same box style as on the gantt screen and have it be in the bottom left corner of the screen.
- [ ] The hitbox for clicking on the calendar screen is off for selecting the user, and my font does not have the symbol in the other box, can you change the hitbox to be correct and change the symbols to something in most default fonts?
- [ ] The plan settings screen has visual issues where text is overlapping (the saved plans) and the UI currently cannot select any other plans, only the save plan and new plan buttons are usable. The names for the identity selection are too low as well in the box and not alligned with the radio button. Can you fix these layout issues on the plan screen?
- [ ] The tags on the Team Members screen clip into the user name when there are more than one or two tags, can you make the Team Members window a bit wider and have long lists of tags have just the start of the text then an elipse to indicate more tags?
