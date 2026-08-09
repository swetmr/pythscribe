<!-- exp=baseline-sonnet task=macro_course_cards cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0284478 -->
```python
"use client"

from pyths.react import component, use_state

TABS = ["All", "Enrolled", "Completed"]

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 45, "enrolled": True},
    {"id": 2, "title": "Introduction to Python", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Cloud Computing Basics", "provider": "Google Cloud", "progress": 70, "enrolled": True},
    {"id": 5, "title": "UX Design Principles", "provider": "CalArts", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Deep Learning Specialization", "provider": "deeplearning.ai", "progress": 30, "enrolled": True},
]


def is_completed(course):
    return course["progress"] == 100


def filter_courses(courses, tab):
    if tab == "Enrolled":
        return [c for c in courses if c["enrolled"] and not is_completed(c)]
    elif tab == "Completed":
        return [c for c in courses if is_completed(c)]
    else:
        return courses


@component
def CourseCatalog():
    courses, set_courses = use_state(INITIAL_COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ]
        set_courses(updated)

    visible_courses = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name=f"tab-button tab-button-active" if tab == active_tab else "tab-button",
                on_click=lambda tab=tab: set_active_tab(tab),
                tab,
            ) for tab in TABS]
        ),
        div(class_name="course-list",
            *[div(key=course["id"], class_name="course-card",
                h3(class_name="course-title", course["title"]),
                p(class_name="course-provider", course["provider"]),
                div(class_name="progress-bar-track",
                    div(class_name="progress-bar-fill",
                        style={"width": f"{course['progress']}%"}),
                ),
                p(class_name="progress-label", f"{course['progress']}% complete"),
                button(
                    class_name="unenroll-button" if course["enrolled"] else "enroll-button",
                    on_click=lambda cid=course["id"]: toggle_enroll(cid),
                    "Unenroll" if course["enrolled"] else "Enroll",
                ),
            ) for course in visible_courses]
        ) if len(visible_courses) > 0 else p(class_name="empty-state", "No courses found."),
    )
```
