<!-- exp=macrov2-opus task=macro_course_cards cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.07325899999999999 -->
```python
"use client"

from pyths.react import component, use_state


def make_courses():
    return [
        {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Cloud Computing Basics", "provider": "Google Cloud", "progress": 20, "enrolled": False},
        {"id": 6, "title": "UX Design Fundamentals", "provider": "CalArts", "progress": 70, "enrolled": True},
        {"id": 7, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    ]


def is_completed(course):
    return course["progress"] == 100


@component
def CourseCatalog():
    courses, set_courses = use_state(make_courses())
    tab, set_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    def matches(course):
        if tab == "Enrolled":
            return course["enrolled"]
        if tab == "Completed":
            return is_completed(course)
        return True

    visible = [c for c in courses if matches(c)]
    tabs = ["All", "Enrolled", "Completed"]

    return div(class_name="course-catalog",
        h1(class_name="catalog-title", "Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(
                key=name,
                class_name="tab active" if tab == name else "tab",
                on_click=lambda: set_tab(name),
                name,
            ) for name in tabs]
        ),
        p(class_name="catalog-count", f"{len(visible)} course(s)"),
        div(class_name="course-grid",
            *[div(key=c["id"], class_name="course-card",
                h2(class_name="course-card-title", c["title"]),
                p(class_name="course-card-provider", c["provider"]),
                div(class_name="progress-track",
                    div(class_name="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                span(class_name="progress-label",
                     "Completed" if is_completed(c) else f"{c['progress']}% complete"),
                button(
                    class_name="enroll-btn enrolled" if c["enrolled"] else "enroll-btn",
                    on_click=lambda: toggle_enroll(c["id"]),
                    "Unenroll" if c["enrolled"] else "Enroll",
                ),
            ) for c in visible]
        ),
    )
```
