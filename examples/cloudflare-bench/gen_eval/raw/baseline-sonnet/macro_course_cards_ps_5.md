<!-- exp=baseline-sonnet task=macro_course_cards cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0269628 -->
```python
"use client"
from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 45},
    {"id": 2, "title": "Full-Stack Web Development", "provider": "Meta", "progress": 100},
    {"id": 3, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 0},
    {"id": 4, "title": "Financial Markets", "provider": "Yale University", "progress": 100},
    {"id": 5, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 70},
    {"id": 6, "title": "Google UX Design", "provider": "Google", "progress": 20},
    {"id": 7, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(course):
    return course["progress"] == 100


def filter_courses(courses, tab):
    if tab == "Enrolled":
        return [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        return [c for c in courses if is_completed(c)]
    else:
        return courses


@component
def CourseCatalog():
    courses, set_courses = use_state([{**c, "enrolled": False} for c in COURSES])
    active_tab, set_active_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ]
        set_courses(updated)

    visible = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="tabs",
            *[button(
                key=tab,
                class_name=f"tab-button{' active' if tab == active_tab else ''}",
                on_click=lambda t=tab: set_active_tab(t),
                tab,
            ) for tab in TABS],
        ),
        div(class_name="course-list",
            *[div(key=course["id"], class_name="course-card",
                h3(class_name="course-title", course["title"]),
                p(class_name="course-provider", course["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-bar",
                        style={"width": f"{course['progress']}%"}),
                ),
                p(class_name="progress-label", f"{course['progress']}% complete"),
                is_completed(course) and span(class_name="completed-badge", "Completed"),
                button(
                    class_name="enroll-button unenroll" if course["enrolled"] else "enroll-button",
                    on_click=lambda cid=course["id"]: toggle_enroll(cid),
                    "Unenroll" if course["enrolled"] else "Enroll",
                ),
            ) for course in visible],
        ),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view."),
    )
```
