<!-- exp=macrov2-opus task=macro_course_cards cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.0376315 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Web Development Bootcamp", "provider": "Johns Hopkins", "progress": 70, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Algorithms Part I", "provider": "Princeton", "progress": 30, "enrolled": False},
]

TABS = ["All", "Enrolled", "Completed"]

def matches(course, tab):
    if tab == "Enrolled":
        return course["enrolled"]
    if tab == "Completed":
        return course["progress"] == 100
    return True

@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(cid):
        set_courses([
            {**c, "enrolled": not c["enrolled"]} if c["id"] == cid else c
            for c in courses
        ])

    visible = [c for c in courses if matches(c, tab)]

    return div(cn="course-catalog",
        h2(cn="catalog-title", "Course Catalog"),
        div(cn="catalog-tabs",
            *[button(key=t, cn=("tab active" if t == tab else "tab"),
                     oc=lambda: set_tab(t), t)
              for t in TABS]),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-card-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                (span(cn="badge-completed", "Completed") if c["progress"] == 100 else None),
                button(cn=("enroll-btn unenroll" if c["enrolled"] else "enroll-btn"),
                       oc=lambda: toggle(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible]),
        (p(cn="empty-state", "No courses in this view.") if not visible else None),
    )
```
