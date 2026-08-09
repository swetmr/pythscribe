<!-- exp=baseline-001 task=macro_course_cards cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.035498499999999995 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Algorithms, Part I", "provider": "Princeton", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "The Science of Well-Being", "provider": "Yale", "progress": 70, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]


def is_completed(c):
    return c["progress"] == 100


def matches_tab(c, tab):
    if tab == "Enrolled":
        return c["enrolled"]
    if tab == "Completed":
        return is_completed(c)
    return True


@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(cid):
        set_courses([{**c, "enrolled": not c["enrolled"]} if c["id"] == cid else c
                     for c in courses])

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(cn="course-catalog",
        h2(cn="catalog-title", "Course Catalog"),
        div(cn="catalog-tabs",
            *[button(key=t, cn=f"tab{' active' if t == tab else ''}",
                     oc=lambda t=t: set_tab(t), t)
              for t in TABS]),
        div(cn="course-grid",
            *[div(key=c["id"], cn=f"course-card{' completed' if is_completed(c) else ''}",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="enroll-btn",
                       oc=lambda cid=c["id"]: toggle(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible]),
        len(visible) == 0 and p(cn="empty", "No courses in this view."),
    )
```
