<!-- exp=macrov2-sonnet task=macro_course_cards cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.0560526 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning Foundations", "provider": "Stanford Online", "progress": 45, "enrolled": True},
    {"id": 2, "title": "Intro to Python Programming", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 100, "enrolled": True},
    {"id": 5, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 20, "enrolled": True},
    {"id": 6, "title": "UX Design Principles", "provider": "Google", "progress": 0, "enrolled": False},
    {"id": 7, "title": "Business Analytics", "provider": "Wharton", "progress": 100, "enrolled": True},
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
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    def mk_toggle(course_id):
        return lambda: toggle_enroll(course_id)

    visible = [c for c in courses if matches_tab(c, active_tab)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn=f"tab {'active' if t == active_tab else ''}",
                     oc=lambda t=t: set_active_tab(t), t)
              for t in TABS],
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-bar-fill", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="enroll-toggle", oc=mk_toggle(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible],
        ),
        len(visible) == 0 and p(cn="empty-state", "No courses found."),
    )
```
