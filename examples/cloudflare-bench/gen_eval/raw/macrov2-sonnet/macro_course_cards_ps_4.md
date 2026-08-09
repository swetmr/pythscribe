<!-- exp=macrov2-sonnet task=macro_course_cards cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0334725 -->
```python
"use client"

from pyths.react import component, use_state

def make_courses():
    return [
        {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Modern JavaScript", "provider": "Meta", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
        {"id": 4, "title": "UX Design Fundamentals", "provider": "Google", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Financial Markets", "provider": "Yale University", "progress": 20, "enrolled": True},
        {"id": 6, "title": "Cloud Computing Basics", "provider": "AWS", "progress": 0, "enrolled": False},
        {"id": 7, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
        {"id": 8, "title": "Marketing Analytics", "provider": "IBM", "progress": 60, "enrolled": True},
    ]


@component
def CourseCard(course, on_toggle):
    is_completed = course["progress"] == 100
    button_label = "Unenroll" if course["enrolled"] else "Enroll"
    status_label = "Completed" if is_completed else ("In Progress" if course["enrolled"] else "Not Enrolled")

    return div(class_name="course-card",
        h3(class_name="course-title", course["title"]),
        p(class_name="course-provider", course["provider"]),
        div(class_name="progress-track",
            div(class_name="progress-bar", style={"width": f"{course['progress']}%"}),
        ),
        div(class_name="course-meta",
            span(class_name="course-status", status_label),
            span(class_name="course-percent", f"{course['progress']}%"),
        ),
        button(class_name="enroll-btn", on_click=lambda: on_toggle(course["id"]), button_label),
    )


@component
def CourseCatalog():
    courses, set_courses = use_state(make_courses())
    active_tab, set_active_tab = use_state("All")

    def toggle_enrolled(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    def matches_tab(c):
        if active_tab == "Enrolled":
            return c["enrolled"] and c["progress"] < 100
        elif active_tab == "Completed":
            return c["progress"] == 100
        else:
            return True

    visible_courses = [c for c in courses if matches_tab(c)]
    tabs = ["All", "Enrolled", "Completed"]

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=tab,
                class_name=f"tab-btn tab-btn-active" if tab == active_tab else "tab-btn",
                on_click=lambda: set_active_tab(tab),
                tab,
              ) for tab in tabs],
        ),
        div(class_name="course-grid",
            *[CourseCard(key=c["id"], course=c, on_toggle=toggle_enrolled) for c in visible_courses],
        ) if len(visible_courses) > 0 else p(class_name="empty-state", "No courses found."),
    )
```
