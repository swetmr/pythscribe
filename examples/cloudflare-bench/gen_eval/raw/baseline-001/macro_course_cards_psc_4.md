<!-- exp=baseline-001 task=macro_course_cards cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.0351485 -->
```python
"use client"
R*

def make_courses():
    return [
        {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 65, "enrolled": True},
        {"id": 3, "title": "Deep Learning", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Data Science", "provider": "IBM", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Algorithms", "provider": "Princeton", "progress": 40, "enrolled": True},
        {"id": 6, "title": "Web Development", "provider": "Meta", "progress": 0, "enrolled": False},
    ]

def is_completed(c):
    return c["progress"] == 100

@c
def CourseCatalog():
    courses, set_courses = us(make_courses())
    tab, set_tab = us("All")

    def toggle(cid):
        set_courses([{**c, "enrolled": not c["enrolled"]} if c["id"] == cid else c for c in courses])

    def visible():
        if tab == "Enrolled":
            return [c for c in courses if c["enrolled"]]
        if tab == "Completed":
            return [c for c in courses if is_completed(c)]
        return courses

    tabs = ["All", "Enrolled", "Completed"]
    shown = visible()

    return div(cn="course-catalog",
        h2("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn=("tab active" if t == tab else "tab"),
                     oc=lambda t=t: set_tab(t), t)
              for t in tabs]),
        div(cn="course-list",
            *[div(key=c["id"], cn=("course-card completed" if is_completed(c) else "course-card"),
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"})),
                span(cn="progress-label", f"{c['progress']}% complete"),
                button(cn=("btn unenroll" if c["enrolled"] else "btn enroll"),
                       oc=lambda cid=c["id"]: toggle(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
              ) for c in shown]),
        len(shown) == 0 and p(cn="empty", "No courses in this view."),
    )
```
