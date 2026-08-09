<!-- exp=macrov2-haiku task=macro_course_cards cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0334635 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python for Data Science", "provider": "DataCamp", "progress": 75},
        {"id": 2, "title": "Web Development Fundamentals", "provider": "Coursera", "progress": 100},
        {"id": 3, "title": "Machine Learning Basics", "provider": "edX", "progress": 45},
        {"id": 4, "title": "Advanced React Patterns", "provider": "Frontend Masters", "progress": 0},
        {"id": 5, "title": "Cloud Computing with AWS", "provider": "Udemy", "progress": 100},
        {"id": 6, "title": "Mobile App Development", "provider": "Coursera", "progress": 60},
    ]
    
    enrolled, set_enrolled = use_state({1, 2, 5})
    tab, set_tab = use_state("All")
    
    def toggle_enroll(course_id):
        if course_id in enrolled:
            set_enrolled(enrolled - {course_id})
        else:
            set_enrolled(enrolled | {course_id})
    
    if tab == "All":
        filtered = courses
    elif tab == "Enrolled":
        filtered = [c for c in courses if c["id"] in enrolled]
    else:
        filtered = [c for c in courses if c["progress"] == 100]
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            button(class_name=f"tab-button {'active' if tab == 'All' else ''}", on_click=lambda: set_tab("All"), "All"),
            button(class_name=f"tab-button {'active' if tab == 'Enrolled' else ''}", on_click=lambda: set_tab("Enrolled"), "Enrolled"),
            button(class_name=f"tab-button {'active' if tab == 'Completed' else ''}", on_click=lambda: set_tab("Completed"), "Completed"),
        ),
        div(class_name="courses-grid",
            *[div(class_name="course-card", key=course["id"],
                h3(course["title"]),
                p(class_name="provider", course["provider"]),
                div(class_name="progress-container",
                    div(class_name="progress-bar", style={"width": f"{course['progress']}%"}),
                ),
                p(class_name="progress-text", f"{course['progress']}%"),
                button(
                    class_name="enroll-button",
                    on_click=lambda: toggle_enroll(course["id"]),
                    "Unenroll" if course["id"] in enrolled else "Enroll"
                ),
            ) for course in filtered]
        ),
    )
```
