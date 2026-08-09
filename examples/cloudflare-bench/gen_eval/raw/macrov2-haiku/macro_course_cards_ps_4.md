<!-- exp=macrov2-haiku task=macro_course_cards cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.028083499999999997 -->
```python
"use client"
from pyths.react import component, use_state

@component
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 75},
        {"id": 2, "title": "Web Development Bootcamp", "provider": "Udemy", "progress": 100},
        {"id": 3, "title": "Machine Learning A-Z", "provider": "Coursera", "progress": 45},
        {"id": 4, "title": "React Mastery", "provider": "Udacity", "progress": 88},
        {"id": 5, "title": "Cloud Computing Fundamentals", "provider": "edX", "progress": 100},
        {"id": 6, "title": "Advanced SQL", "provider": "DataCamp", "progress": 30},
    ]
    
    current_tab, set_current_tab = use_state("All")
    enrolled_ids, set_enrolled_ids = use_state({1, 2, 4})
    
    def toggle_enrolled(course_id):
        if course_id in enrolled_ids:
            set_enrolled_ids(enrolled_ids - {course_id})
        else:
            set_enrolled_ids(enrolled_ids | {course_id})
    
    def get_filtered_courses():
        if current_tab == "All":
            return courses
        elif current_tab == "Enrolled":
            return [c for c in courses if c["id"] in enrolled_ids]
        elif current_tab == "Completed":
            return [c for c in courses if c["id"] in enrolled_ids and c["progress"] == 100]
        return courses
    
    filtered_courses = get_filtered_courses()
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            *[button(
                class_name=f"tab {'active' if current_tab == tab else ''}",
                on_click=lambda: set_current_tab(tab),
                tab
            ) for tab in ["All", "Enrolled", "Completed"]]
        ),
        div(class_name="course-list",
            *[div(class_name="course-card",
                h3(c["title"]),
                p(class_name="provider", c["provider"]),
                div(class_name="progress-container",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"})
                ),
                p(class_name="progress-text", f"{c['progress']}%"),
                button(
                    class_name="enroll-btn",
                    on_click=lambda: toggle_enrolled(c["id"]),
                    "Unenroll" if c["id"] in enrolled_ids else "Enroll"
                )
            ) for c in filtered_courses]
        )
    )
```
