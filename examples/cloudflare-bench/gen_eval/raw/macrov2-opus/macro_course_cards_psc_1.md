<!-- exp=macrov2-opus task=macro_course_cards cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.08187900000000001 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Algorithms", "provider": "Princeton", "progress": 70, "enrolled": True},
    {"id": 6, "title": "Web Development", "provider": "Meta", "progress": 0, "enrolled": False},
]

def is_completed(c):
    return c["progress"] == 100

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
            return is_completed(c)
        return True

    visible = [c for c in courses if matches(c)]
    tabs = ["All", "Enrolled", "Completed"]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t,
                     cn="tab active" if t == tab else "tab",
                     oc=lambda t=t: set_tab(t), t)
              for t in tabs]),
        div(cn="course-grid",
            *[div(key=c["id"], cn="course-card",
                is_completed(c) and span(cn="badge", "Completed"),
                h2(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill",
                        style={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="enroll-btn enrolled" if c["enrolled"] else "enroll-btn",
                       oc=lambda cid=c["id"]: toggle(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
              )
              for c in visible]),
    )
```
