<!-- exp=baseline-sonnet task=macro_course_cards cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0427218 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses, set_courses = us([
        {"id": 1, "title": "Intro to Machine Learning", "provider": "Stanford", "progress": 45, "enrolled": True},
        {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Data Structures and Algorithms", "provider": "UC San Diego", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Web Development Bootcamp", "provider": "Meta", "progress": 30, "enrolled": False},
        {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 70, "enrolled": True},
    ])
    tab, set_tab = us("All")

    def toggle_enroll(course_id):
        updated = [{**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c for c in courses]
        set_courses(updated)

    def is_completed(c):
        return c["progress"] == 100

    if tab == "Enrolled":
        filtered = [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        filtered = [c for c in courses if is_completed(c)]
    else:
        filtered = courses

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=t, cn="tab tab-active" if tab == t else "tab",
                     oc=lambda t=t: set_tab(t), t)
              for t in ["All", "Enrolled", "Completed"]],
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h2(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-track",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="unenroll-btn" if c["enrolled"] else "enroll-btn",
                       oc=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in filtered],
        ),
    )
```
