<!-- exp=baseline-001 task=macro_course_cards cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0826355 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Science Foundations", "provider": "IBM", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Google UX Design", "provider": "Google", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Algorithms Part I", "provider": "Princeton", "progress": 70, "enrolled": True},
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


@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="catalog-tabs",
            *[button(key=t,
                     cn="tab active" if t == tab else "tab",
                     oc=lambda t=t: set_tab(t),
                     t)
              for t in TABS]),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h2(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"})),
                p(cn="progress-label", f"{c['progress']}% complete"),
                is_completed(c) and span(cn="badge-completed", "Completed"),
                button(cn="enroll-btn" if not c["enrolled"] else "unenroll-btn",
                       oc=lambda cid=c["id"]: toggle(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible]) if visible else p(cn="empty-state", "No courses in this view."),
    )
```
