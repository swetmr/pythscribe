<!-- exp=macrov2-sonnet task=macro_course_cards cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0512556 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 0, "enrolled": False},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Financial Markets", "provider": "Yale University", "progress": 45, "enrolled": True},
    {"id": 4, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
    {"id": 5, "title": "UX Design Fundamentals", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "AWS", "progress": 20, "enrolled": True},
    {"id": 7, "title": "Creative Writing", "provider": "Wesleyan University", "progress": 0, "enrolled": False},
    {"id": 8, "title": "Business Analytics", "provider": "University of Pennsylvania", "progress": 100, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    elif tab == "Completed":
        return course["progress"] == 100
    else:
        return True


@component
def CourseCard(course, on_toggle):
    is_completed = course["progress"] == 100
    return div(class_name="course-card",
        h3(class_name="course-title", course["title"]),
        p(class_name="course-provider", course["provider"]),
        div(class_name="progress-track",
            div(class_name="progress-bar", style={"width": f"{course['progress']}%"}),
        ),
        p(class_name="progress-label", f"{course['progress']}% complete" if not is_completed else "Completed"),
        button(
            class_name="unenroll-btn" if course["enrolled"] else "enroll-btn",
            on_click=lambda: on_toggle(course["id"]),
            "Unenroll" if course["enrolled"] else "Enroll",
        ),
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

    visible = [c for c in courses if matches_tab(c, active_tab)]

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
            *[CourseCard(key=c["id"], course=c, on_toggle=toggle_enrolled) for c in visible],
        ) if visible else p(class_name="empty-state", "No courses in this view."),
    )
```
