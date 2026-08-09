<!-- exp=macrov2-sonnet task=macro_course_cards cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.026977499999999998 -->
```python
"use client"
from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 0, "enrolled": False},
    {"id": 2, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Full-Stack Web Development", "provider": "Meta", "progress": 100, "enrolled": True},
    {"id": 4, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Cloud Computing Fundamentals", "provider": "Google Cloud", "progress": 20, "enrolled": True},
    {"id": 6, "title": "UX Design Principles", "provider": "IBM", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Financial Markets", "provider": "Yale University", "progress": 60, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def filter_courses(courses, tab):
    if tab == "Enrolled":
        return [c for c in courses if c["enrolled"]]
    if tab == "Completed":
        return [c for c in courses if c["progress"] == 100]
    return courses


@component
def CourseCard(course, on_toggle):
    return div(class_name="course-card",
        h3(class_name="course-title", course["title"]),
        p(class_name="course-provider", course["provider"]),
        div(class_name="progress-track",
            div(class_name="progress-fill", style={"width": f"{course['progress']}%"}),
        ),
        p(class_name="progress-label", f"{course['progress']}% complete"),
        button(
            class_name="unenroll-btn" if course["enrolled"] else "enroll-btn",
            on_click=lambda: on_toggle(course["id"]),
            "Unenroll" if course["enrolled"] else "Enroll",
        ),
    )


@component
def CourseCatalog():
    courses, set_courses = use_state(INITIAL_COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enrolled(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible_courses = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1(class_name="catalog-heading", "Course Catalog"),
        div(class_name="tab-bar",
            *[button(
                key=tab,
                class_name="tab active" if tab == active_tab else "tab",
                on_click=lambda: set_active_tab(tab),
                tab,
            ) for tab in TABS],
        ),
        div(class_name="course-grid",
            *[CourseCard(key=c["id"], course=c, on_toggle=toggle_enrolled) for c in visible_courses],
        ) if len(visible_courses) > 0 else p(class_name="empty-state", "No courses found."),
    )
```
