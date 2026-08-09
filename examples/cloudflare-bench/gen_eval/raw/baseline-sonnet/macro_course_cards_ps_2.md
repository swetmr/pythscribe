<!-- exp=baseline-sonnet task=macro_course_cards cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.026557800000000003 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 0, "enrolled": False},
    {"id": 2, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Intro to Cloud Computing", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 4, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 5, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 6, "title": "UX Design Principles", "provider": "Google", "progress": 20, "enrolled": True},
    {"id": 7, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
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


def progress_bar(percent):
    return div(class_name="progress-track",
        div(class_name="progress-fill", style={"width": f"{percent}%"}),
    )


@component
def CourseCatalog():
    courses, set_courses = use_state(COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enrolled(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="tab-bar",
            *[button(
                key=tab,
                class_name=f"tab-button {'active' if tab == active_tab else ''}",
                on_click=lambda tab=tab: set_active_tab(tab),
                tab,
            ) for tab in TABS],
        ),
        div(class_name="course-grid",
            *[div(key=course["id"], class_name="course-card",
                h2(class_name="course-title", course["title"]),
                p(class_name="course-provider", course["provider"]),
                progress_bar(course["progress"]),
                p(class_name="progress-label", f"{course['progress']}% complete"),
                is_completed(course) and span(class_name="completed-badge", "Completed"),
                button(
                    class_name="enroll-button",
                    on_click=lambda cid=course["id"]: toggle_enrolled(cid),
                    "Unenroll" if course["enrolled"] else "Enroll",
                ),
            ) for course in visible],
        ),
        len(visible) == 0 and p(class_name="empty-state", "No courses in this view."),
    )
```
