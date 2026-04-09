# Development Notes

## Web Fixes/Requests

- [ ] Can you fix the add users flow? I can get to the screen for adding a user but the save button doesn't add a new user it just crashes
- [ ] Can you have the app width be dynamic and fill the browser window rather than currently the fixed max width with the bars at the side?
- [ ] My browser currently interprets scrolling left and right as a browser back and forward actions, is there a way to stop these working for my app?
- [ ] Can you match up the colours between the rust UI and the react ui? The task status colours are wrong, among other colours. Have a general check that the colours match up
- [ ] The drag panning on the allocation screen isn't working, can you fix this?
- [ ] Also on the allocation screen the scroll left right up and down all map to panning left and right, can this be fixed so scrolling up and down actually goes up and down?
- [ ] The allocation screen content is blurry and I can't see any vertical or horizontal seperators for tasks and allocation bars, can you fix this?
- [ ] The gantt chart is a lot less readable than before, can you make the tasks a bit less tall, make the left and right edge not completely fill the gantt grid box, then make the font size across the app slightly bigger
- [ ] On the gantt screen there are no dependency arrows and no hover effect. I am fine having no dependency arrows when there is nothing hovered but I would like to have dependency lines drawn between dependencies and dependents for the task, same as the rust ui, when items are hovered. In particular I would like these dependency arrows to be drawn between the gantt grid centerpoints like the rust ui
- [ ] Like the rust UI, I would like the task packing in the web gantt chart to require two tasks are not right next to each other to allow for dependency arrows to come out the left and right of the task
- [ ] The forward dependents box is missing from the add/edit task/milestone screen. Can you add it?
- [ ] There is no filter on the workers selection box, can you add it?
- [ ] There is no filter/search on the scheduler target box, can you add it?
- [ ] Can you remove the up and down arrows from number entry boxes and allow them to be empty for ease of number entry? empty number boxes should be interpreted as 0
- [ ] The on the overview screen the icon to the left in the task button is missing from my font. Can you make SVG files from the old icons in the app and use those instead of the text for the buttons at the top?
- [ ] Can you make the dependencies and dependents lists be in a scrollable container within the edit task screen to avoid the edit task window being very long?
- [ ] can you have the same border flashing red for the search as before?
- [ ] When jumping to tasks it centers the vertical scroll to the task but the scroll ordinarily only goes as high as the top task at the top of the screen. Can you have the vertical scroll jump respect the limits?
- [ ] Can you add a vertical scroll limit so I cannot scroll down below the bottom task?
- [ ] Can you allow me to scroll left and right about half a screen beyond the ends of the tasks?
- [ ] Can you make sure the milestone names are drawn where possible? Possibly by adding an exclusion for a couple grid boxes to the right of the milestone for the grid packing
- [ ] Can you add back the coloured borders when a task is hovered? Colouring both the dependent, current, and dependency tasks
- [ ] Can you also add back the gold border for the plan target node
