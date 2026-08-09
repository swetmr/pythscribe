<!-- exp=macrov2-opus task=macro_course_cards cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.0362315 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Python for Everybody", "provider": "Univ. of Michigan", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Machine Learning", "provider": "Stanford", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Google Data Analytics", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Algorithms, Part I", "provider": "Princeton", "progress": 75, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]

def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    if tab == "Completed":
        return course["progress"] == 100
    return True

@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(course_id):
        set_courses([
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ])

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(cn="course-catalog",
        h1(cn="catalog-title", "Course Catalog"),
        div(cn="catalog-tabs",
            *[button(key=t, cn="tab active" if t == tab else "tab",
                     oc=lambda: set_tab(t), t)
              for t in TABS]),
        div(cn="course-grid",
            *[div(key=c["id"], cn="course-card",
                h2(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                c["progress"] == 100 and span(cn="badge-completed", "Completed"),
                button(cn="enroll-btn unenroll" if c["enrolled"] else "enroll-btn",
                       oc=lambda: toggle(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible]),
        len(visible) == 0 and p(cn="empty-state", "No courses in this view."),
    )
```
