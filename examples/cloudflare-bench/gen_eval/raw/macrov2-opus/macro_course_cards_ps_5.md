<!-- exp=macrov2-opus task=macro_course_cards cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.0384455 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford University", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Financial Markets", "provider": "Yale University", "progress": 20, "enrolled": False},
    {"id": 6, "title": "The Science of Well-Being", "provider": "Yale University", "progress": 75, "enrolled": True},
    {"id": 7, "title": "Google Data Analytics", "provider": "Google", "progress": 100, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(course):
    return course["progress"] == 100


def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    if tab == "Completed":
        return is_completed(course)
    return True


@component
def CourseCatalog():
    courses, set_courses = use_state(INITIAL_COURSES)
    active_tab, set_active_tab = use_state("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(class_name="course-catalog",
        h1("Course Catalog"),
        div(class_name="catalog-tabs",
            *[button(key=tab,
                     class_name="tab active" if tab == active_tab else "tab",
                     on_click=lambda: set_active_tab(tab),
                     tab)
              for tab in TABS]),
        div(class_name="course-grid",
            *[div(key=c["id"],
                  class_name="course-card completed" if is_completed(c) else "course-card",
                  h2(class_name="course-title", c["title"]),
                  p(class_name="course-provider", c["provider"]),
                  div(class_name="progress-bar",
                      div(class_name="progress-fill",
                          style={"width": f"{c['progress']}%"})),
                  p(class_name="progress-label", f"{c['progress']}% complete"),
                  button(class_name="enroll-btn",
                         on_click=lambda: toggle_enroll(c["id"]),
                         "Unenroll" if c["enrolled"] else "Enroll"))
              for c in visible]) if visible else p(class_name="empty-state", "No courses in this tab."),
        p(class_name="catalog-summary", f"Showing {len(visible)} of {len(courses)} courses"),
    )
```
