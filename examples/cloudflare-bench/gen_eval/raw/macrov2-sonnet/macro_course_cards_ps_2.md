<!-- exp=macrov2-sonnet task=macro_course_cards cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0267375 -->
```python
"use client"

from pyths.react import component, use_state

def make_courses():
    return [
        {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Data Structures and Algorithms", "provider": "Coursera", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
        {"id": 5, "title": "UX Design Fundamentals", "provider": "Google", "progress": 20, "enrolled": True},
        {"id": 6, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0, "enrolled": False},
        {"id": 7, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 70, "enrolled": True},
    ]

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
    courses, set_courses = use_state(make_courses())
    active_tab, set_active_tab = use_state("All")

    def toggle_enrolled(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ]
        set_courses(updated)

    def filter_courses():
        if active_tab == "Enrolled":
            return [c for c in courses if c["enrolled"] and c["progress"] < 100]
        elif active_tab == "Completed":
            return [c for c in courses if c["progress"] == 100]
        else:
            return courses

    visible = filter_courses()
    tabs = ["All", "Enrolled", "Completed"]

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="tab-bar",
            *[button(
                key=t,
                class_name="tab-active" if t == active_tab else "tab",
                on_click=lambda t=t: set_active_tab(t),
                t,
              ) for t in tabs],
        ),
        div(class_name="course-grid",
            *[CourseCard(key=c["id"], course=c, on_toggle=toggle_enrolled) for c in visible],
        ) if len(visible) > 0 else p(class_name="empty-state", "No courses found."),
    )
```
