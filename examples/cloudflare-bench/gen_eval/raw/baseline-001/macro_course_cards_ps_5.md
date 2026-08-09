<!-- exp=baseline-001 task=macro_course_cards cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.041133499999999996 -->
```python
"use client"

from pyths.react import component, use_state


def initial_courses():
    return [
        {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Algorithms, Part I", "provider": "Princeton", "progress": 20, "enrolled": True},
        {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
        {"id": 7, "title": "The Science of Well-Being", "provider": "Yale", "progress": 100, "enrolled": True},
    ]


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
    courses, set_courses = use_state(initial_courses())
    active_tab, set_active_tab = use_state("All")

    tabs = ["All", "Enrolled", "Completed"]

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
                key=t,
                class_name="tab active" if t == active_tab else "tab",
                on_click=lambda t=t: set_active_tab(t),
                t,
            ) for t in tabs]
        ),
        div(class_name="course-grid",
            *[div(key=c["id"], class_name="course-card" + (" completed" if is_completed(c) else ""),
                h3(class_name="course-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                p(class_name="progress-label", f"{c['progress']}% complete"),
                is_completed(c) and span(class_name="badge", "Completed") or None,
                button(
                    class_name="enroll-btn enrolled" if c["enrolled"] else "enroll-btn",
                    on_click=lambda cid=c["id"]: toggle_enroll(cid),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
            ) for c in visible]
        ),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view.") or None,
    )
```
