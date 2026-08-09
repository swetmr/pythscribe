<!-- exp=baseline-001 task=macro_course_cards cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.0401085 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford University", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Financial Markets", "provider": "Yale University", "progress": 20, "enrolled": True},
    {"id": 6, "title": "The Science of Well-Being", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Google Data Analytics", "provider": "Google", "progress": 75, "enrolled": True},
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

    def render_card(course):
        completed = is_completed(course)
        return div(class_name="course-card", key=course["id"],
            div(class_name="course-header",
                h3(class_name="course-title", course["title"]),
                completed and span(class_name="badge-completed", "Completed"),
            ),
            p(class_name="course-provider", course["provider"]),
            div(class_name="progress-track",
                div(class_name="progress-fill",
                    style={"width": f"{course['progress']}%"}),
            ),
            p(class_name="progress-label", f"{course['progress']}% complete"),
            button(
                class_name="unenroll-btn" if course["enrolled"] else "enroll-btn",
                on_click=lambda: toggle_enroll(course["id"]),
                "Unenroll" if course["enrolled"] else "Enroll",
            ),
        )

    return div(class_name="course-catalog",
        h2(class_name="catalog-heading", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name="tab active" if tab == active_tab else "tab",
                on_click=lambda t=tab: set_active_tab(t),
                tab,
            ) for tab in TABS]),
        div(class_name="course-grid",
            *[render_card(c) for c in visible])
            if len(visible) > 0
            else p(class_name="empty-state", "No courses in this view."),
    )
```
