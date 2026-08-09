<!-- exp=macrov2-opus task=macro_course_cards cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.0361205 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Algorithms, Part I", "provider": "Princeton", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 70, "enrolled": True},
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
    active, set_active = use_state("All")

    def toggle(cid):
        updated = []
        for c in courses:
            if c["id"] == cid:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, active)]

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="tabs",
            *[button(key=t,
                     class_name="tab active" if t == active else "tab",
                     on_click=lambda: set_active(t),
                     t)
              for t in TABS]),
        div(class_name="course-list",
            *[div(key=c["id"], class_name="course-card",
                h2(class_name="course-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-bar",
                    div(class_name="progress-fill",
                        style={"width": f"{c['progress']}%"})),
                p(class_name="progress-label", f"{c['progress']}% complete"),
                c["progress"] == 100 and span(class_name="badge", "Completed"),
                button(class_name="enroll-btn",
                       on_click=lambda: toggle(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"))
              for c in visible]),
        len(visible) == 0 and p(class_name="empty", "No courses in this view."),
    )
```
