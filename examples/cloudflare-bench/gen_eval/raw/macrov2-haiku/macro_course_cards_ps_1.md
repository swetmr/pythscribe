<!-- exp=macrov2-haiku task=macro_course_cards cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0370956 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    # Fixture data
    initial_courses = [
        {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 85},
        {"id": 2, "title": "Web Development Bootcamp", "provider": "Udemy", "progress": 100},
        {"id": 3, "title": "Machine Learning Fundamentals", "provider": "Coursera", "progress": 45},
        {"id": 4, "title": "Advanced JavaScript", "provider": "Pluralsight", "progress": 0},
        {"id": 5, "title": "Cloud Computing with AWS", "provider": "Linux Academy", "progress": 70},
        {"id": 6, "title": "Introduction to Databases", "provider": "Codecademy", "progress": 100},
    ]
    
    # State
    tab, set_tab = use_state("All")
    enrolled, set_enrolled = use_state({1: True, 2: True, 3: True, 4: False, 5: False, 6: True})
    
    # Filter courses by tab
    def get_filtered_courses():
        if tab == "All":
            return initial_courses
        elif tab == "Enrolled":
            return [c for c in initial_courses if enrolled[c["id"]]]
        else:  # Completed
            return [c for c in initial_courses if c["progress"] == 100 and enrolled[c["id"]]]
    
    # Toggle enrollment
    def toggle_enrollment(course_id):
        set_enrolled({**enrolled, course_id: not enrolled[course_id]})
    
    filtered = get_filtered_courses()
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            button(class_name="tab-button" + (" active" if tab == "All" else ""),
                   on_click=lambda: set_tab("All"), "All"),
            button(class_name="tab-button" + (" active" if tab == "Enrolled" else ""),
                   on_click=lambda: set_tab("Enrolled"), "Enrolled"),
            button(class_name="tab-button" + (" active" if tab == "Completed" else ""),
                   on_click=lambda: set_tab("Completed"), "Completed"),
        ),
        div(class_name="courses-grid",
            *[div(class_name="course-card", key=c["id"],
                h3(c["title"]),
                p(class_name="provider", c["provider"]),
                div(class_name="progress-container",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                p(class_name="progress-text", f"{c['progress']}% Complete"),
                button(class_name="enroll-button",
                       on_click=lambda: toggle_enrollment(c["id"]),
                       "Unenroll" if enrolled[c["id"]] else "Enroll"),
              ) for c in filtered],
        ),
    )
```
