<!-- exp=macrov2-sonnet task=macro_course_cards cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0256041 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford Online", "progress": 0, "enrolled": False},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 45, "enrolled": True},
    {"id": 4, "title": "Financial Markets", "provider": "Yale University", "progress": 0, "enrolled": False},
    {"id": 5, "title": "UX Design Fundamentals", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 20, "enrolled": True},
    {"id": 7, "title": "Cloud Computing Basics", "provider": "IBM", "progress": 0, "enrolled": False},
]

def matches_tab(course, tab):
    if tab == "Enrolled":
        return course["enrolled"] and course["progress"] < 100
    if tab == "Completed":
        return course["progress"] == 100
    return True

@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle_enroll(course_id):
        updated = [c for c in courses]
        for i, c in enumerate(updated):
            if c["id"] == course_id:
                updated[i] = {**c, "enrolled": not c["enrolled"]}
        set_courses(updated)

    def mk_toggle(course_id):
        return lambda: toggle_enroll(course_id)

    visible = [c for c in courses if matches_tab(c, tab)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn="tab-active" if tab == t else "tab", oc=lambda t=t: set_tab(t), t)
              for t in ["All", "Enrolled", "Completed"]],
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="unenroll-btn" if c["enrolled"] else "enroll-btn",
                       oc=mk_toggle(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in visible],
        ),
    )
```
