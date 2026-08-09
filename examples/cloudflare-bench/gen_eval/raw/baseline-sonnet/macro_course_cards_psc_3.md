<!-- exp=baseline-sonnet task=macro_course_cards cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0245568 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 45, "enrolled": True},
    {"id": 2, "title": "Intro to Python Programming", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Data Structures & Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Business Analytics", "provider": "Wharton", "progress": 100, "enrolled": True},
    {"id": 5, "title": "UX Design Principles", "provider": "Google", "progress": 20, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Basics", "provider": "AWS", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Financial Markets", "provider": "Yale University", "progress": 100, "enrolled": True},
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
    active_tab, set_active_tab = us("All")

    def toggle_enroll(course_id):
        updated = [c for c in courses]
        for i, c in enumerate(updated):
            if c["id"] == course_id:
                updated[i] = {**c, "enrolled": not c["enrolled"]}
        set_courses(updated)

    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn="tab active" if t == active_tab else "tab",
                     oc=lambda t=t: set_active_tab(t), t) for t in TABS]
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                span(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="unenroll-btn" if c["enrolled"] else "enroll-btn",
                       oc=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible]
        ),
        len(visible) == 0 and p(cn="empty-state", "No courses in this view."),
    )
```
