<!-- exp=macrov2-sonnet task=macro_course_cards cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.08122410000000001 -->
```python
"use client"
R*

def is_completed(c):
    return c["progress"] == 100

INITIAL_COURSES = [
    {"title": "Intro to Python", "provider": "Coursera", "progress": 45, "enrolled": True},
    {"title": "Machine Learning", "provider": "Stanford Online", "progress": 100, "enrolled": True},
    {"title": "Web Development Bootcamp", "provider": "Udemy", "progress": 0, "enrolled": False},
    {"title": "Data Structures & Algorithms", "provider": "MIT OpenCourseWare", "progress": 100, "enrolled": True},
    {"title": "Deep Learning Specialization", "provider": "deeplearning.ai", "progress": 20, "enrolled": False},
    {"title": "Cloud Computing Basics", "provider": "Google Cloud", "progress": 0, "enrolled": False},
]

TABS = ["All", "Enrolled", "Completed"]

@c
def CourseCatalog():
    tab, set_tab = us("All")
    courses, set_courses = us(INITIAL_COURSES)

    def toggle_enroll(i):
        updated = [c for c in courses]
        updated[i] = {**updated[i], "enrolled": not updated[i]["enrolled"]}
        set_courses(updated)

    indexed = list(enumerate(courses))
    if tab == "Enrolled":
        filtered = [(i, c) for i, c in indexed if c["enrolled"]]
    elif tab == "Completed":
        filtered = [(i, c) for i, c in indexed if is_completed(c)]
    else:
        filtered = indexed

    return div(cn="course-catalog",
        div(cn="tabs",
            *[button(key=t, cn=f"tab {'active' if t == tab else ''}", oc=lambda: set_tab(t), t)
              for t in TABS]
        ),
        div(cn="course-list",
            *[div(key=c["title"], cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                span(cn="progress-label", f"{c['progress']}%"),
                span(cn="completed-badge", "Completed") if is_completed(c) else None,
                button(cn="enroll-toggle", oc=lambda: toggle_enroll(i),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for i, c in filtered]
        ),
    )
```
