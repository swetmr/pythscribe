<!-- exp=macrov2-opus task=macro_course_cards cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.0362955 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 100},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0},
    {"id": 4, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 100},
    {"id": 5, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 70},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "Google Cloud", "progress": 20},
    {"id": 7, "title": "Financial Markets", "provider": "Yale University", "progress": 0},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(course):
    return course["progress"] == 100


@component
def CourseCatalog():
    courses, set_courses = use_state([{**c, "enrolled": is_completed(c)} for c in COURSES])
    tab, set_tab = use_state("All")

    def toggle_enroll(cid):
        set_courses([
            {**c, "enrolled": not c["enrolled"]} if c["id"] == cid else c
            for c in courses
        ])

    def matches(course):
        if tab == "Enrolled":
            return course["enrolled"]
        if tab == "Completed":
            return is_completed(course)
        return True

    visible = [c for c in courses if matches(c)]

    return div(class_name="course-catalog",
        h2(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=name,
                class_name="tab active" if tab == name else "tab",
                on_click=lambda: set_tab(name),
                name,
            ) for name in TABS],
        ),
        div(class_name="course-grid",
            *[div(key=c["id"], class_name="course-card",
                h3(class_name="course-card-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-fill", style={"width": f"{c['progress']}%"}),
                ),
                p(class_name="progress-label",
                    "Completed" if is_completed(c) else f"{c['progress']}% complete"),
                button(
                    class_name="enroll-btn unenroll" if c["enrolled"] else "enroll-btn enroll",
                    on_click=lambda: toggle_enroll(c["id"]),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
            ) for c in visible],
        ),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view."),
    )
```
