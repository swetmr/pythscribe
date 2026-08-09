<!-- exp=macrov2-sonnet task=macro_course_cards cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0453741 -->
```python
"use client"
R*

def is_completed(course):
    return course["progress"] == 100

@c
def CourseCatalog():
    courses, set_courses = us([
        {"id": 1, "title": "Intro to Python", "provider": "University of Michigan", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Machine Learning", "provider": "Stanford University", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Web Design Basics", "provider": "Google", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Data Structures", "provider": "UC San Diego", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Cloud Computing", "provider": "IBM", "progress": 20, "enrolled": False},
        {"id": 6, "title": "Digital Marketing", "provider": "Meta", "progress": 70, "enrolled": True},
    ])
    tab, set_tab = us("All")

    def toggle_enroll(course_id):
        set_courses([{**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c for c in courses])

    if tab == "Enrolled":
        filtered = [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        filtered = [c for c in courses if is_completed(c)]
    else:
        filtered = courses

    return div(cn="course-catalog",
        div(cn="tabs",
            *[button(key=t, cn="tab tab-active" if tab == t else "tab",
                     oc=lambda: set_tab(t), t)
              for t in ["All", "Enrolled", "Completed"]]
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar-track",
                    div(cn="progress-bar-fill", st={"width": f"{c['progress']}%"}),
                ),
                span(cn="progress-label", f"{c['progress']}%"),
                button(cn="enroll-btn", oc=lambda: toggle_enroll(c["id"]),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in filtered]
        ),
    )
```
