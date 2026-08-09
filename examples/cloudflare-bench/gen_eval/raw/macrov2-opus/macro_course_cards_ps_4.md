<!-- exp=macrov2-opus task=macro_course_cards cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.0381705 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 100, "enrolled": True},
    {"id": 5, "title": "SQL for Data Science", "provider": "UC Davis", "progress": 20, "enrolled": False},
    {"id": 6, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 70, "enrolled": True},
    {"id": 7, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
]

TABS = ["All", "Enrolled", "Completed"]


def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    if tab == "Completed":
        return course["progress"] == 100
    return True


@component
def CourseCatalog():
    courses, set_courses = use_state(COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name="tab active" if tab == active_tab else "tab",
                on_click=lambda: set_active_tab(tab),
                tab,
            ) for tab in TABS]),
        div(class_name="course-list",
            *[div(key=c["id"], class_name="course-card",
                h2(class_name="course-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-bar",
                    div(class_name="progress-fill",
                        style={"width": f"{c['progress']}%"})),
                p(class_name="progress-label", f"{c['progress']}% complete"),
                c["progress"] == 100 and span(class_name="completed-badge", "Completed"),
                button(
                    class_name="enroll-btn enrolled" if c["enrolled"] else "enroll-btn",
                    on_click=lambda: toggle_enroll(c["id"]),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
            ) for c in visible]),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view."),
    )
```
