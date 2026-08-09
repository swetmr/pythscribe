<!-- exp=macrov2-sonnet task=macro_course_cards cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0292275 -->
```python
"use client"

from pyths.react import component, use_state

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 45, "enrolled": True},
    {"id": 2, "title": "Intro to Python Programming", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 72, "enrolled": True},
    {"id": 5, "title": "UX Design Principles", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Business Analytics", "provider": "University of Pennsylvania", "progress": 30, "enrolled": True},
]

def filter_courses(courses, tab):
    if tab == "Enrolled":
        return [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        return [c for c in courses if c["progress"] == 100]
    else:
        return courses

@component
def CourseCard(course, on_toggle):
    return div(class_name="course-card",
        h3(class_name="course-title", course["title"]),
        p(class_name="course-provider", course["provider"]),
        div(class_name="progress-bar-track",
            div(class_name="progress-bar-fill",
                style={"width": f"{course['progress']}%"}),
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
    courses, set_courses = use_state(COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enrollment(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ]
        set_courses(updated)

    tabs = ["All", "Enrolled", "Completed"]
    visible_courses = filter_courses(courses, active_tab)

    return div(class_name="course-catalog",
        h1(class_name="catalog-header", "Course Catalog"),
        div(class_name="tab-bar",
            *[button(
                key=tab,
                class_name="tab-active" if tab == active_tab else "tab",
                on_click=lambda: set_active_tab(tab),
                tab,
              ) for tab in tabs],
        ),
        div(class_name="course-grid",
            *[CourseCard(course=c, on_toggle=toggle_enrollment, key=c["id"])
              for c in visible_courses],
        ) if len(visible_courses) > 0 else p(class_name="empty-state", "No courses in this category."),
    )
```
