<!-- exp=baseline-001 task=macro_course_cards cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0752105 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford University", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Google Data Analytics", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 5, "title": "The Science of Well-Being", "provider": "Yale University", "progress": 70, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Algorithms, Part I", "provider": "Princeton University", "progress": 20, "enrolled": False},
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

    def tab_count(tab):
        return len([c for c in courses if matches_tab(c, tab)])

    return div(class_name="course-catalog",
        header(class_name="catalog-header",
            h1("Course Catalog"),
            p(class_name="catalog-subtitle", "Explore and track your learning"),
        ),
        nav(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name="tab active" if tab == active_tab else "tab",
                on_click=lambda t=tab: set_active_tab(t),
                f"{tab} ({tab_count(tab)})",
            ) for tab in TABS],
        ),
        ul(class_name="course-grid",
            *[li(key=course["id"], class_name="course-card",
                div(class_name="course-body",
                    h2(class_name="course-title", course["title"]),
                    p(class_name="course-provider", course["provider"]),
                    div(class_name="progress-bar",
                        div(class_name="progress-fill",
                            style={"width": f"{course['progress']}%"}),
                    ),
                    div(class_name="progress-meta",
                        span(class_name="progress-label", f"{course['progress']}% complete"),
                        span(class_name="course-badge", "Completed")
                            if is_completed(course) else None,
                    ),
                ),
                button(
                    class_name="enroll-btn unenroll" if course["enrolled"] else "enroll-btn",
                    on_click=lambda cid=course["id"]: toggle_enroll(cid),
                    "Unenroll" if course["enrolled"] else "Enroll",
                ),
            ) for course in visible],
        ) if visible else p(class_name="empty-state", "No courses in this view."),
    )
```
