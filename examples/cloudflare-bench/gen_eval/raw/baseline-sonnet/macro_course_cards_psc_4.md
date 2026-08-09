<!-- exp=baseline-sonnet task=macro_course_cards cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0454068 -->
```python
"use client"
R*

def is_completed(course):
    return course["progress"] == 100

@c
def CourseCatalog():
    courses, set_courses = us([
        {"id": 1, "title": "Intro to Python", "provider": "Coursera", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Data Structures", "provider": "Stanford Online", "progress": 45, "enrolled": True},
        {"id": 3, "title": "Machine Learning", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Web Development", "provider": "Meta", "progress": 100, "enrolled": True},
        {"id": 5, "title": "Cloud Computing", "provider": "Google", "progress": 20, "enrolled": False},
        {"id": 6, "title": "UX Design Basics", "provider": "IBM", "progress": 0, "enrolled": False},
    ])
    tab, set_tab = us("All")

    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)

    if tab == "Enrolled":
        filtered = [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        filtered = [c for c in courses if is_completed(c)]
    else:
        filtered = courses

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=name,
                     cn="tab active" if tab == name else "tab",
                     oc=lambda name=name: set_tab(name),
                     name)
              for name in ["All", "Enrolled", "Completed"]],
        ),
        div(cn="course-list",
            *[div(key=c["id"], cn="course-card",
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-label", f"{c['progress']}% complete"),
                button(cn="unenroll-btn" if c["enrolled"] else "enroll-btn",
                       oc=lambda cid=c["id"]: toggle_enroll(cid),
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in filtered],
        ),
    )
```
