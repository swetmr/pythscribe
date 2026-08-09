<!-- exp=baseline-sonnet task=macro_course_cards cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.049921799999999995 -->
```python
"use client"
R*

COURSES = [
    {"title": "Intro to Python", "provider": "University of Michigan", "progress": 100, "enrolled": True},
    {"title": "Machine Learning", "provider": "Stanford", "progress": 45, "enrolled": True},
    {"title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"title": "Data Structures", "provider": "UC San Diego", "progress": 100, "enrolled": True},
    {"title": "Cloud Computing", "provider": "Google", "progress": 20, "enrolled": False},
    {"title": "Business Analytics", "provider": "Wharton", "progress": 0, "enrolled": False},
    {"title": "UX Design Fundamentals", "provider": "Google", "progress": 70, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]

@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")

    def toggle(i):
        updated = [c for c in courses]
        updated[i] = {**updated[i], "enrolled": not updated[i]["enrolled"]}
        set_courses(updated)

    def matches(c):
        if tab == "Enrolled":
            return c["enrolled"]
        if tab == "Completed":
            return c["progress"] == 100
        return True

    shown = [(i, c) for i, c in enumerate(courses) if matches(c)]

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn="tab" + (" active" if t == tab else ""),
                     oc=lambda t=t: set_tab(t), t) for t in TABS],
        ),
        div(cn="course-list",
            *[div(key=i, cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="enroll-btn", oc=lambda i=i: toggle(i),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for i, c in shown],
        ),
    )
```
