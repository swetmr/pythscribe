<!-- exp=baseline-haiku task=macro_course_cards cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0353913 -->
```python
"use client"
from pyths.react import component, use_state

@component
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 75},
        {"id": 2, "title": "Web Development Bootcamp", "provider": "Udemy", "progress": 100},
        {"id": 3, "title": "Machine Learning Basics", "provider": "edX", "progress": 50},
        {"id": 4, "title": "Advanced JavaScript", "provider": "Coursera", "progress": 100},
        {"id": 5, "title": "Cloud Architecture", "provider": "Pluralsight", "progress": 25},
        {"id": 6, "title": "React Deep Dive", "provider": "Frontend Masters", "progress": 85},
    ]
    
    enrolled_ids, set_enrolled_ids = use_state([1, 2, 4])
    active_tab, set_active_tab = use_state("All")
    
    def toggle_enroll(course_id):
        if course_id in enrolled_ids:
            new_enrolled = [cid for cid in enrolled_ids if cid != course_id]
        else:
            new_enrolled = [*enrolled_ids, course_id]
        set_enrolled_ids(new_enrolled)
    
    if active_tab == "Enrolled":
        filtered_courses = [c for c in courses if c["id"] in enrolled_ids]
    elif active_tab == "Completed":
        filtered_courses = [c for c in courses if c["id"] in enrolled_ids and c["progress"] == 100]
    else:
        filtered_courses = courses
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            button(class_name=f"tab {'active' if active_tab == 'All' else ''}",
                   on_click=lambda: set_active_tab("All"),
                   "All"),
            button(class_name=f"tab {'active' if active_tab == 'Enrolled' else ''}",
                   on_click=lambda: set_active_tab("Enrolled"),
                   "Enrolled"),
            button(class_name=f"tab {'active' if active_tab == 'Completed' else ''}",
                   on_click=lambda: set_active_tab("Completed"),
                   "Completed"),
        ),
        div(class_name="courses-list",
            *[div(class_name="course-card", key=c["id"],
                div(class_name="course-header",
                    h3(c["title"]),
                    span(class_name="provider", c["provider"]),
                ),
                div(class_name="progress-section",
                    div(class_name="progress-bar",
                        div(class_name="progress-fill",
                            style={"width": f"{c['progress']}%"}
                        ),
                    ),
                    span(class_name="progress-text", f"{c['progress']}%"),
                ),
                button(class_name="enroll-btn",
                       on_click=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["id"] in enrolled_ids else "Enroll"),
            ) for c in filtered_courses]
        ),
    )
```
