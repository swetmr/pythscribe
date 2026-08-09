<!-- exp=macrov2-haiku task=macro_course_cards cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.06411850000000001 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 75, "enrolled": True},
        {"id": 2, "title": "Web Development Basics", "provider": "edX", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Machine Learning Fundamentals", "provider": "Udacity", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Advanced JavaScript", "provider": "Coursera", "progress": 50, "enrolled": True},
        {"id": 5, "title": "Cloud Computing Essentials", "provider": "AWS", "progress": 100, "enrolled": True},
        {"id": 6, "title": "Mobile App Development", "provider": "Pluralsight", "progress": 25, "enrolled": False},
    ]
    
    active_tab, set_active_tab = use_state("All")
    course_states, set_course_states = use_state({c["id"]: c["enrolled"] for c in courses})
    
    def toggle_enrollment(course_id):
        updated = {**course_states}
        updated[course_id] = not updated[course_id]
        set_course_states(updated)
    
    def filter_courses():
        if active_tab == "All":
            return courses
        elif active_tab == "Enrolled":
            return [c for c in courses if course_states[c["id"]]]
        elif active_tab == "Completed":
            return [c for c in courses if c["progress"] == 100]
        return courses
    
    filtered = filter_courses()
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            button(class_name=f"tab-button {'active' if active_tab == 'All' else ''}",
                   on_click=lambda: set_active_tab("All"),
                   "All"),
            button(class_name=f"tab-button {'active' if active_tab == 'Enrolled' else ''}",
                   on_click=lambda: set_active_tab("Enrolled"),
                   "Enrolled"),
            button(class_name=f"tab-button {'active' if active_tab == 'Completed' else ''}",
                   on_click=lambda: set_active_tab("Completed"),
                   "Completed"),
        ),
        div(class_name="courses-grid",
            *[div(key=str(c["id"]), class_name="course-card",
                  div(class_name="course-title", c["title"]),
                  div(class_name="course-provider", c["provider"]),
                  div(class_name="progress-bar-container",
                      div(class_name="progress-bar",
                          style={"width": f"{c['progress']}%"}),
                  ),
                  div(class_name="progress-text", f"{c['progress']}%"),
                  button(class_name="enroll-button",
                         on_click=lambda: toggle_enrollment(c["id"]),
                         "Unenroll" if course_states[c["id"]] else "Enroll"),
              )
              for c in filtered]
        ),
    )
```
