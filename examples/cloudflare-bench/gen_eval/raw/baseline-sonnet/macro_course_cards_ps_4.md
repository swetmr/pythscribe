<!-- exp=baseline-sonnet task=macro_course_cards cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0260328 -->
```python
"use client"

from pyths.react import component, use_state

def is_completed(course):
    return course["progress"] == 100

def matches_tab(course, tab):
    if tab == "All":
        return True
    elif tab == "Completed":
        return is_completed(course)
    elif tab == "Enrolled":
        return course["enrolled"] and not is_completed(course)
    return True

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning Fundamentals", "provider": "Stanford Online", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Introduction to Python", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Data Science Specialization", "provider": "Johns Hopkins", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 100, "enrolled": True},
    {"id": 5, "title": "UX Design Principles", "provider": "Google", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Business Analytics", "provider": "Wharton", "progress": 70, "enrolled": True},
]

@component
def CourseCatalog():
    courses, set_courses = use_state(INITIAL_COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    tabs = ["All", "Enrolled", "Completed"]
    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="tabs",
            *[button(
                key=tab,
                class_name="tab active" if tab == active_tab else "tab",
                on_click=lambda tab=tab: set_active_tab(tab),
                tab,
              ) for tab in tabs]
        ),
        div(class_name="course-list",
            *[div(key=c["id"], class_name="course-card",
                h3(c["title"]),
                p(class_name="provider", c["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                p(class_name="progress-label", f"{c['progress']}% complete"),
                button(
                    class_name="unenroll-btn" if c["enrolled"] else "enroll-btn",
                    on_click=lambda cid=c["id"]: toggle_enroll(cid),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
              ) for c in visible]
        ) if visible else p(class_name="empty-state", "No courses in this category."),
    )
```
