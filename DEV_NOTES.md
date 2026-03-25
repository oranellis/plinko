# Development Notes

## Fixes/Requests

- [ ] The title text for the name of the task on the task hover widget is positioned too high and clips out of the box, as well as looking like it's not centered. Can you fix the text positioning for the task name?
- [ ] I would like to change the behaviour of the scheduler. Rather than having tasks scheduled across long gaps interupted by other tasks, I would like tasks to stay as consecutive as possible. There are several tasks in the plan now that span several weeks even though there are only a couple days of allocation. Can you have it so tasks are inserted only in gaps where they fit consecutively, only interupted by calendar gaps rather than other tasks? Basically just enforce that tasks run as consecutively as possible.
- [ ] The background colour on the home screen, calendar screen, and really all the screens is still much too light, can you make it a darker gray?
- [ ] The dependency arrows are not drawn in very good positions right now, can you instead of drawing them freely between points, just have them traverse between the center points of the calendar/task grid? So for example when a dependency arrow leaves the left (end) of a task it should come from the center of the last day rather than the edge, then go into the center of the start of the next task/milestone. To make it easier to see what's happening in hover mode, when the user hovers can you have all vertical lines be slightly to the left for dependencies and be slightly to the right for dependents?
