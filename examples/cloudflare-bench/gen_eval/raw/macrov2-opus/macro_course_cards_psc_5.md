<!-- exp=macrov2-opus task=macro_course_cards cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0355565 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Data Science", "provider": "Johns Hopkins", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Deep Learning", "provider": "DeepLearning.AI", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Web Design", "provider": "UC Davis", "progress": 20, "enrolled": False},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 70, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(cid):
        set_courses([{**c, "enrolled": not c["enrolled"]} if c["id"] == cid else c
                     for c in courses])

    def matches(c):
        if tab == "Enrolled":
            return c["enrolled"]
        if tab == "Completed":
            return c["progress"] == 100
        return True

    visible = [c for c in courses if matches(c)]

    return div(cn="course-catalog",
        h1(cn="catalog-title", "Course Catalog"),
        div(cn="catalog-tabs",
            *[button(key=t,
                     cn="tab active" if t == tab else "tab",
                     oc=lambda t=t: set_tab(t),
                     t)
              for t in TABS]),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-card-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                (span(cn="badge completed", "Completed") if c["progress"] == 100 else None),
                button(cn="enroll-btn" if not c["enrolled"] else "enroll-btn unenroll",
                       oc=lambda cid=c["id"]: toggle(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"))
              for c in visible]),
        (p(cn="empty-state", "No courses in this view.") if len(visible) == 0 else None),
    )
```
