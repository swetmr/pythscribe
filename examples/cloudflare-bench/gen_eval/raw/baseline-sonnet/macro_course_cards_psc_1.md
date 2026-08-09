<!-- exp=baseline-sonnet task=macro_course_cards cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.06939899999999999 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Full-Stack Web Development", "provider": "Meta", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Financial Markets", "provider": "Yale", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Python for Everybody", "provider": "Michigan", "progress": 70, "enrolled": True},
    {"id": 6, "title": "UX Design Fundamentals", "provider": "Google", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 30, "enrolled": True},
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

    def toggle_enroll(course_id):
        updated = [{**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c for c in courses]
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn="tab" + (" active" if t == tab else ""),
                     oc=lambda t=t: set_tab(t), t) for t in TABS],
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="enroll-btn",
                       oc=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible],
        ),
    )
```
