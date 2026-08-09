<!-- exp=baseline-sonnet task=macro_course_cards cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.046328999999999995 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 45, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 70, "enrolled": True},
    {"id": 5, "title": "UX Design Fundamentals", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Negotiation Skills", "provider": "ESSEC Business School", "progress": 20, "enrolled": True},
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

def CourseCard(course, on_toggle):
    button_label = "Unenroll" if course["enrolled"] else "Enroll"
    status_label = "Completed" if is_completed(course) else ("Enrolled" if course["enrolled"] else "Not enrolled")
    return div(class_name="course-card", key=course["id"],
        h3(class_name="course-title", course["title"]),
        p(class_name="course-provider", course["provider"]),
        div(class_name="progress-track",
            div(class_name="progress-bar", style={"width": f"{course['progress']}%"}),
        ),
        p(class_name="course-status", f"{status_label} - {course['progress']}%"),
        button(class_name="enroll-toggle", on_click=lambda: on_toggle(course["id"]), button_label),
    )

@component
def CourseCatalog():
    courses, set_courses = use_state(INITIAL_COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enrollment(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible_courses = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name="tab-button active" if tab == active_tab else "tab-button",
                on_click=lambda tab=tab: set_active_tab(tab),
                tab,
              ) for tab in TABS],
        ),
        div(class_name="course-list",
            *[CourseCard(c, toggle_enrollment) for c in visible_courses],
        ) if len(visible_courses) > 0 else p(class_name="empty-state", "No courses in this view."),
    )
```
