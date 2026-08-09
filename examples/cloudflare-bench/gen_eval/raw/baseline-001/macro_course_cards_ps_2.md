<!-- exp=baseline-001 task=macro_course_cards cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.0377835 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 4, "title": "The Science of Well-Being", "provider": "Yale", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Deep Learning", "provider": "DeepLearning.AI", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Google Data Analytics", "provider": "Google", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Algorithms", "provider": "Princeton", "progress": 70, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(course):
    return course["progress"] == 100


def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    if tab == "Completed":
        return is_completed(course)
    return True


@component
def CourseCatalog():
    courses, set_courses = use_state(COURSES)
    tab, set_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=t,
                class_name="tab active" if t == tab else "tab",
                on_click=lambda t=t: set_tab(t),
                t,
            ) for t in TABS],
        ),
        div(class_name="course-grid",
            *[div(key=c["id"], class_name="course-card",
                h2(class_name="course-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                p(class_name="progress-label", f"{c['progress']}% complete"),
                is_completed(c) and span(class_name="badge-completed", "Completed") or None,
                button(
                    class_name="enroll-btn enrolled" if c["enrolled"] else "enroll-btn",
                    on_click=lambda cid=c["id"]: toggle_enroll(cid),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
            ) for c in visible],
        ),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view.") or None,
    )
```
