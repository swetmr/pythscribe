<!-- exp=macrov2-sonnet task=macro_course_cards cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.07751910000000001 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Machine Learning", "provider": "Stanford", "progress": 100, "enrolled": True},
    {"id": 2, "title": "Python for Everybody", "provider": "University of Michigan", "progress": 45, "enrolled": True},
    {"id": 3, "title": "Deep Learning Specialization", "provider": "DeepLearning.AI", "progress": 0, "enrolled": False},
    {"id": 4, "title": "Google Data Analytics", "provider": "Google", "progress": 100, "enrolled": True},
    {"id": 5, "title": "UX Design Fundamentals", "provider": "University of Michigan", "progress": 20, "enrolled": False},
    {"id": 6, "title": "Financial Markets", "provider": "Yale", "progress": 0, "enrolled": False},
    {"id": 7, "title": "AWS Cloud Practitioner", "provider": "Amazon Web Services", "progress": 70, "enrolled": True},
]

TABS = ["All", "Enrolled", "Completed"]

def filter_courses(courses, tab):
    if tab == "Enrolled":
        return [c for c in courses if c["enrolled"]]
    elif tab == "Completed":
        return [c for c in courses if c["progress"] == 100]
    else:
        return courses

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

    filtered = filter_courses(courses, active_tab)

    return div(cn="course-catalog",
        h1("Course Catalog"),
        div(cn="tabs",
            *[button(key=tab, cn="tab" + (" tab-active" if tab == active_tab else ""),
                     oc=lambda: set_active_tab(tab), tab)
              for tab in TABS],
        ),
        div(cn="course-list",
            *[div(key=course["id"], cn="course-card",
                  h3(cn="course-title", course["title"]),
                  p(cn="course-provider", course["provider"]),
                  div(cn="progress-bar",
                      div(cn="progress-fill", st={"width": f"{course['progress']}%"}),
                  ),
                  span(cn="progress-label", f"{course['progress']}%"),
                  button(cn="enroll-btn",
                         oc=lambda: toggle_enroll(course["id"]),
                         "Unenroll" if course["enrolled"] else "Enroll"),
              )
              for course in filtered],
        ),
    )
```
