<!-- exp=baseline-001 task=macro_course_cards cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.0409835 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford University", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science with R", "provider": "Johns Hopkins University", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "Google Cloud", "progress": 0, "enrolled": False},
    {"id": 7, "title": "UX Design Principles", "provider": "California Institute of the Arts", "progress": 75, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(course):
    return course["progress"] == 100


def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    elif tab == "Completed":
        return is_completed(course)
    return True


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

    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(key=tab,
                     class_name="tab active" if tab == active_tab else "tab",
                     on_click=lambda t=tab: set_active_tab(t),
                     tab)
              for tab in TABS]),
        div(class_name="course-grid",
            *[div(key=c["id"], class_name="course-card",
                h2(class_name="course-card-title", c["title"]),
                p(class_name="course-provider", c["provider"]),
                div(class_name="progress-bar-track",
                    div(class_name="progress-bar-fill",
                        style={"width": f"{c['progress']}%"})),
                p(class_name="progress-label",
                    f"{c['progress']}% complete" + (" \u2713" if is_completed(c) else "")),
                button(class_name="enroll-btn unenroll" if c["enrolled"] else "enroll-btn",
                       on_click=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"))
              for c in visible]) if visible else p(class_name="empty-state", "No courses in this view."),
    )
```
